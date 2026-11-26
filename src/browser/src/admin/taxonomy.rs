use crate::common::{Action, Data, Field, load, nullable_num, num, t, text, validation_message};
use blog_interface::component::component_props_builder;
use blog_interface::components::notify;
use blog_interface::prelude::*;
use blog_interface::{ev, html};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) enum TaxonomyKind {
    Categories,
    Labels,
}

impl TaxonomyKind {
    fn labels(self) -> bool {
        matches!(self, Self::Labels)
    }

    fn wording(self, categories: (&str, &str), labels: (&str, &str)) -> String {
        let (zh, en) = if self.labels() { labels } else { categories };
        t(zh, en)
    }
}

#[derive(Clone, Copy)]
struct TaxonomyEditor {
    kind: TaxonomyKind,
    endpoint: &'static str,
    data: Data,
    categories: Data,
    category_names: Memo<BTreeMap<i64, String>>,
    form: RwSignal<Value>,
    editing: RwSignal<Option<i64>>,
    priority: RwSignal<String>,
    priority_input: NodeRef<html::Input>,
    validation: RwSignal<String>,
    save: Action,
    deletion: Action,
    deleting: RwSignal<Option<i64>>,
}

const CATEGORIES: &str = "/api/v1/categories";

fn category_rows(value: Option<&Value>) -> &[Value] {
    value.map_or(&[], |rows| {
        rows.as_array()
            .expect("invariant: categories passed the list response contract")
    })
}

fn fields(name: &str, description: &str, sort_id: i64) -> Value {
    json!({"name":name,"description":description,"sort_id":sort_id})
}

impl TaxonomyEditor {
    fn new(kind: TaxonomyKind) -> Self {
        let labels = kind.labels();
        let endpoint = if labels {
            "/api/v1/content/labels"
        } else {
            "/api/v1/content/categories"
        };
        let data = load(Signal::derive(move || {
            if labels { endpoint } else { CATEGORIES }.into()
        }));
        let categories = load(Signal::derive(move || {
            if labels { CATEGORIES } else { "" }.into()
        }));
        let category_names = Memo::new(move |_| {
            categories.with_current(|value| {
                let mut names = BTreeMap::new();
                for category in category_rows(value) {
                    names
                        .entry(num(category, "id"))
                        .or_insert_with(|| text(category, "sort_name"));
                }
                names
            })
        });
        Self {
            kind,
            endpoint,
            data,
            categories,
            category_names,
            form: RwSignal::new(fields("", "", 0)),
            editing: RwSignal::new(None),
            priority: RwSignal::new("0".into()),
            priority_input: NodeRef::new(),
            validation: RwSignal::new(String::new()),
            save: Action::new(),
            deletion: Action::new(),
            deleting: RwSignal::new(None),
        }
    }

    fn prefill(self, id: Option<i64>, rank: i64, name: &str, description: &str, sort_id: i64) {
        self.editing.set(id);
        self.priority.set(rank.to_string());
        self.validation.set(String::new());
        self.form.set(fields(name, description, sort_id));
    }

    fn reset(self) {
        self.prefill(None, 0, "", "", 0);
    }

    fn save_blocked(self) -> bool {
        self.save.busy.get()
            || self.deletion.busy.get() && self.deleting.get() == self.editing.get()
    }

    fn submit(self, ev: &web_sys::SubmitEvent) {
        ev.prevent_default();
        if untrack(|| self.save_blocked()) {
            return;
        }
        let target = self.editing.get_untracked();
        let current = self.form.get_untracked();
        if self.kind.labels()
            && !self
                .category_names
                .with_untracked(|names| names.contains_key(&num(&current, "sort_id")))
        {
            return;
        }
        let description = current["description"]
            .as_str()
            .filter(|value| !value.is_empty());
        let mut body = json!({"name":current["name"],"description":description});
        if self.kind.labels() {
            body["sort_id"] = current["sort_id"].clone();
        } else {
            let rank = self
                .priority
                .with_untracked(|priority| priority.trim().parse::<i64>().ok());
            let Some(rank) =
                rank.filter(|rank| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(rank))
            else {
                self.validation.set(t(
                    "优先级须为 -9007199254740991 至 9007199254740991 之间的整数。",
                    "Priority must be a whole number between -9007199254740991 and 9007199254740991.",
                ));
                if let Some(input) = self.priority_input.get_untracked()
                    && let Err(error) = input.focus()
                {
                    web_sys::console::warn_2(&"Unable to move browser focus".into(), &error);
                }
                return;
            };
            body["priority"] = json!(rank);
        }
        self.validation.set(String::new());
        let endpoint = self.endpoint;
        let path = target.map_or_else(|| endpoint.into(), |id| format!("{endpoint}/{id}"));
        self.save.run(
            path,
            if target.is_some() { "PUT" } else { "POST" },
            Some(body),
            move |_| {
                self.reset();
                self.data.reload();
                notify(self.kind.wording(
                    ("分类已保存", "Category saved"),
                    ("标签已保存", "Tag saved"),
                ));
            },
        );
    }

    fn remove(self, id: i64) {
        if self.save.busy.get_untracked()
            || !window()
                .confirm_with_message(&self.kind.wording(
                    ("确定删除这个分类？", "Delete this category?"),
                    ("确定删除这个标签？", "Delete this tag?"),
                ))
                .unwrap_or(false)
        {
            return;
        }
        self.deleting.set(Some(id));
        self.deletion.run(
            format!("{}/{id}", self.endpoint),
            "DELETE",
            None,
            move |_| {
                if self.editing.get_untracked() == Some(id) {
                    self.reset();
                }
                self.data.update_latest(|value| {
                    value
                        .as_array_mut()
                        .expect("invariant: taxonomy passed the list response contract")
                        .retain(|row| num(row, "id") != id);
                });
                self.data.reload();
                notify(self.kind.wording(
                    ("分类已删除", "Category deleted"),
                    ("标签已删除", "Tag deleted"),
                ));
            },
        );
    }

    fn category_input(self) -> impl IntoView {
        let label = t("分类", "Category");
        html::label().class("blog-form-field blog-stack").child((
            html::span().class("blog-field-label blog-row").child(label),
            crate::select::Select(
                component_props_builder(&crate::select::Select)
                    .label(Signal::derive(|| t("分类", "Category")))
                    .value(Signal::derive(move || {
                        self.form.with(|value| num(value, "sort_id")).to_string()
                    }))
                    .options(Signal::derive(move || {
                        self.categories.with_current(|categories| {
                            let options = category_rows(categories)
                                .iter()
                                .map(|row| (num(row, "id").to_string(), text(row, "sort_name")));
                            std::iter::once(("0".into(), t("选择分类", "Select a category")))
                                .chain(options)
                                .collect()
                        })
                    }))
                    .change(Callback::new(move |value: String| {
                        let id = value
                            .parse::<i64>()
                            .expect("invariant: category selector options are integer IDs");
                        self.form.update(|f| f["sort_id"] = json!(id));
                    }))
                    .required(true)
                    .disabled(Signal::derive(move || {
                        self.categories.with_current(|value| value.is_none())
                    }))
                    .build(),
            ),
        ))
    }

    fn priority_input(self) -> impl IntoView {
        html::label().class("blog-form-field blog-stack").child((
            html::span()
                .class("blog-field-label blog-row")
                .child(t("优先级", "Priority")),
            html::input()
                .class("blog-input")
                .node_ref(self.priority_input)
                .name("priority")
                .r#type("text")
                .inputmode("numeric")
                .prop("value", move || self.priority.get())
                .on(ev::input, move |ev| {
                    self.priority.set(event_target_value(&ev));
                    self.validation.set(String::new());
                }),
        ))
    }

    fn form(self) -> impl IntoView {
        let kind = self.kind;
        html::form()
            .class("inline-form blog-frame blog-frame--padded blog-flow blog-form-flow")
            .child((
                Field::RequiredLine(if kind.labels() { 32 } else { 64 }).bind(
                    self.form,
                    "name",
                    kind.wording(("分类名称", "Category name"), ("标签名称", "Tag name")),
                ),
                Field::Line(200).bind(
                    self.form,
                    "description",
                    kind.wording(
                        ("分类描述", "Category description"),
                        ("标签描述", "Tag description"),
                    ),
                ),
                if kind.labels() {
                    self.category_input().into_any()
                } else {
                    self.priority_input().into_any()
                },
                html::button()
                    .class("blog-button")
                    .r#type("submit")
                    .child(move || {
                        if self.save.busy.get() {
                            t("保存中…", "Saving…")
                        } else if self.editing.get().is_some() {
                            t("保存修改", "Save changes")
                        } else {
                            kind.wording(("创建分类", "Create category"), ("新增标签", "Add tag"))
                        }
                    })
                    .disabled(move || self.save_blocked()),
                move || {
                    self.editing.get().is_some().then(|| {
                        html::button()
                            .class("blog-button")
                            .r#type("button")
                            .child(t("取消编辑", "Cancel editing"))
                            .on(ev::click, move |_| self.reset())
                    })
                },
            ))
            .on(ev::submit, move |ev| self.submit(&ev))
    }

    fn row(self, item: &Value) -> impl IntoView + use<> {
        let id = num(item, "id");
        let labels = self.kind.labels();
        let (name, description, sort_id, rank) = if labels {
            (
                text(item, "label_name"),
                text(item, "label_description"),
                num(item, "sort_id"),
                0,
            )
        } else {
            (
                text(item, "sort_name"),
                text(item, "sort_description"),
                0,
                nullable_num(item, "priority").unwrap_or(0),
            )
        };
        let ordering = if labels {
            self.category_names
                .with(|names| names.get(&sort_id).cloned())
                .unwrap_or_default()
        } else {
            rank.to_string()
        };
        html::tr().child((
            labels.then(|| html::td().child(id)),
            html::td().class("blog-title-cell").child(name.clone()),
            html::td().class("comment-cell").child(description.clone()),
            html::td().child(ordering),
            html::td().child(
                html::div().class("blog-actions blog-flow").child((
                    html::button()
                        .class("blog-button")
                        .child(t("编辑", "Edit"))
                        .on(ev::click, move |_| {
                            self.prefill(Some(id), rank, &name, &description, sort_id);
                        }),
                    html::button()
                        .class("blog-button blog-danger")
                        .child(t("删除", "Delete"))
                        .disabled(move || self.deletion.busy.get())
                        .on(ev::click, move |_| self.remove(id)),
                )),
            ),
        ))
    }

    fn table(self) -> impl IntoView {
        let kind = self.kind;
        move || {
            self.data.with_latest(|value| {
                let rows = value.and_then(Value::as_array)?;
                let headers = [
                    kind.wording(("名称", "Name"), ("标签", "Tags")),
                    t("描述", "Description"),
                    kind.wording(("排序", "Order"), ("分类", "Category")),
                    t("操作", "Actions"),
                ]
                .map(|label| html::th().child(label));
                let id_header = kind.labels().then(|| html::th().child("ID"));
                let header = html::thead().child(html::tr().child((id_header, headers)));
                let rows =
                    html::tbody().child(rows.iter().map(|item| self.row(item)).collect_view());
                let label = kind.wording(("分类列表", "Category list"), ("标签列表", "Tag list"));
                let table = html::table()
                    .class("blog-table")
                    .aria_label(label)
                    .child((header, rows));
                Some(
                    html::div()
                        .class("blog-table-scroll blog-frame")
                        .tabindex("0")
                        .role("region")
                        .aria_label(t("数据表格", "Data table"))
                        .child(table),
                )
            })
        }
    }
}

#[component]
pub(super) fn Taxonomy(kind: TaxonomyKind) -> impl IntoView {
    let editor = TaxonomyEditor::new(kind);
    html::section()
        .class("blog-content-stack blog-stack")
        .aria_label(kind.wording(("分类管理", "Categories"), ("标签管理", "Tags")))
        .child((
            editor.data.admin_feedback(None),
            editor
                .categories
                .admin_feedback(Some(t("分类加载失败", "Unable to load categories"))),
            editor.save.feedback(),
            editor.deletion.feedback(),
            validation_message(editor.validation),
            html::fieldset()
                .class("blog-fieldset blog-stack blog-stack--spaced")
                .disabled(move || editor.save.busy.get())
                .child((editor.form(), editor.table())),
        ))
}
