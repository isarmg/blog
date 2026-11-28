use crate::common::{
    Action, Data, Field, ListState, Pager, enc, flag, form_check, framed_table, load, num,
    page_items, t, text,
};
use crate::select::Select;
use blog_interface::component::component_props_builder;
use blog_interface::components::notify;
use blog_interface::prelude::*;
use blog_interface::{ev, html};
use serde_json::{Value, json};

const LINKS: &str = "/api/v1/content/links";
const OPTIONAL: [&str; 4] = ["classify", "cover", "url", "introduction"];
const INPUTS: [(&str, &str, &str, Field); 4] = [
    ("title", "标题", "Title", Field::RequiredLine(64)),
    (
        "cover",
        "图片地址",
        "Image URL",
        Field::Line(Field::DEFAULT_LIMIT),
    ),
    (
        "url",
        "目标地址",
        "Target URL",
        Field::Line(Field::DEFAULT_LIMIT),
    ),
    (
        "introduction",
        "简介",
        "Introduction",
        Field::Line(Field::DEFAULT_LIMIT),
    ),
];

fn empty_link() -> Value {
    json!({"title":"","classify":"","cover":"","url":"","introduction":"","link_type":"favorites","status":true})
}

fn link_types() -> Vec<(String, String)> {
    [
        ("favorites", t("收藏", "Favorites")),
        ("friendUrl", t("友链", "Friend links")),
        ("lovePhoto", t("照片", "Photos")),
        ("funny", t("音乐与趣味", "Music and fun")),
    ]
    .map(|(kind, label)| (kind.into(), label))
    .into()
}

#[derive(Clone, Copy)]
enum RowChange {
    Approve,
    Delete,
}

#[derive(Clone, Copy)]
struct LinkManager {
    data: Data,
    draft: RwSignal<Value>,
    editing: RwSignal<Option<i64>>,
    saving: Action,
    changing: Action,
    changed_id: RwSignal<Option<i64>>,
}

impl LinkManager {
    fn new(data: Data) -> Self {
        Self {
            data,
            draft: RwSignal::new(empty_link()),
            editing: RwSignal::new(None),
            saving: Action::new(),
            changing: Action::new(),
            changed_id: RwSignal::new(None),
        }
    }

    fn reset(self) {
        self.draft.set(empty_link());
        self.editing.set(None);
    }

    fn busy(self) -> bool {
        self.saving.busy.get_untracked() || self.changing.busy.get_untracked()
    }

    fn edit(self, id: i64, draft: &Value) {
        if self.saving.busy.get_untracked()
            || self.changing.busy.get_untracked() && self.changed_id.get_untracked() == Some(id)
        {
            return;
        }
        self.editing.set(Some(id));
        self.draft.set(draft.clone());
    }

    fn submit(self) {
        if self.busy() {
            return;
        }
        let mut draft = self.draft.get_untracked();
        for key in OPTIONAL {
            if draft[key].as_str().unwrap_or_default().is_empty() {
                draft[key] = Value::Null;
            }
        }
        let (path, method) = self.editing.get_untracked().map_or_else(
            || (LINKS.to_owned(), "POST"),
            |id| (format!("{LINKS}/{id}"), "PUT"),
        );
        self.saving.run(path, method, Some(draft), move |_| {
            self.reset();
            self.data.reload();
            notify(t("资源已保存", "Resource saved"));
        });
    }

    fn change(self, id: i64, change: RowChange) {
        if self.busy() {
            return;
        }
        let (path, method, body) = match change {
            RowChange::Approve => (
                format!("{LINKS}/{id}/status"),
                "PUT",
                Some(json!({"active": true})),
            ),
            RowChange::Delete => {
                if !window()
                    .confirm_with_message(&t("确定删除这条资源？", "Delete this resource?"))
                    .unwrap_or(false)
                {
                    return;
                }
                (format!("{LINKS}/{id}"), "DELETE", None)
            }
        };
        self.changed_id.set(Some(id));
        self.changing.run(path, method, body, move |_| {
            if self.editing.get_untracked() == Some(id) {
                match change {
                    RowChange::Approve => self
                        .draft
                        .update(|draft| draft["status"] = Value::Bool(true)),
                    RowChange::Delete => self.reset(),
                }
            }
            self.data.reload();
        });
    }
}

fn labelled_select(
    label: fn() -> String,
    value: Signal<String>,
    options: Signal<Vec<(String, String)>>,
    change: Callback<String>,
) -> impl IntoView {
    html::label().class("blog-form-field blog-stack").child((
        html::span()
            .class("blog-field-label blog-row")
            .child(label()),
        Select(
            component_props_builder(&Select)
                .label(Signal::derive(label))
                .value(value)
                .options(options)
                .change(change)
                .build(),
        ),
    ))
}

fn link_filters(
    page: RwSignal<u64>,
    kind: RwSignal<String>,
    status: RwSignal<i64>,
) -> impl IntoView {
    html::div()
        .class("inline-form blog-flow blog-form-flow")
        .child((
            labelled_select(
                || t("类型筛选", "Filter by type"),
                Signal::derive(move || kind.get()),
                Signal::derive(|| {
                    std::iter::once((String::new(), t("全部", "All")))
                        .chain(link_types())
                        .collect()
                }),
                Callback::new(move |value| {
                    kind.set(value);
                    page.set(1);
                }),
            ),
            labelled_select(
                || t("状态筛选", "Filter by status"),
                Signal::derive(move || status.get().to_string()),
                Signal::derive(|| {
                    vec![
                        ("-1".into(), t("全部", "All")),
                        ("0".into(), t("待审核/隐藏", "Pending review / hidden")),
                        ("1".into(), t("已公开", "Published")),
                    ]
                }),
                Callback::new(move |value: String| {
                    status.set(value.parse().unwrap_or(-1));
                    page.set(1);
                }),
            ),
        ))
}

fn link_form(manager: LinkManager) -> impl IntoView {
    let LinkManager {
        draft,
        editing,
        changing,
        changed_id,
        ..
    } = manager;
    html::form()
        .class("editor-form blog-frame blog-frame--padded blog-stack")
        .child(
            html::fieldset()
                .class("blog-fieldset blog-stack")
                .disabled(move || changing.busy.get() && changed_id.get() == editing.get())
                .child((
                    html::h2().class("blog-row").child(move || {
                        if editing.get().is_some() {
                            t("编辑资源", "Edit resource")
                        } else {
                            t("新增资源", "Add resource")
                        }
                    }),
                    html::div().class("form-grid").child((
                        labelled_select(
                            || t("类型", "Type"),
                            Signal::derive(move || draft.with(|draft| text(draft, "link_type"))),
                            Signal::derive(link_types),
                            Callback::new(move |value| {
                                draft.update(|draft| draft["link_type"] = Value::String(value));
                            }),
                        ),
                        Field::Line(32).bind(draft, "classify", t("分类", "Category")),
                    )),
                    INPUTS.map(|(key, zh, en, field)| field.bind(draft, key, t(zh, en))),
                    form_check(draft, "status", t(" 公开显示", " Show publicly")),
                    html::div().class("blog-actions blog-flow").child((
                        html::button()
                            .class("blog-button")
                            .r#type("submit")
                            .child(move || {
                                if editing.get().is_some() {
                                    t("保存修改", "Save changes")
                                } else {
                                    t("新增资源", "Add resource")
                                }
                            })
                            .disabled(move || changing.busy.get()),
                        move || {
                            editing.get().map(|_| {
                                html::button()
                                    .child(t("取消编辑", "Cancel editing"))
                                    .class("blog-button")
                                    .r#type("button")
                                    .on(ev::click, move |_| manager.reset())
                            })
                        },
                    )),
                )),
        )
        .on(ev::submit, move |event| {
            event.prevent_default();
            manager.submit();
        })
}

fn link_row(manager: LinkManager, item: &Value) -> impl IntoView + use<> {
    let id = num(item, "id");
    let kind = text(item, "link_type");
    let active = flag(item, "status");
    let can_approve = kind == "friendUrl" && !active;
    let draft = json!({
        "title": text(item, "title"),
        "classify": text(item, "classify"),
        "cover": text(item, "cover"),
        "url": text(item, "url"),
        "introduction": text(item, "introduction"),
        "link_type": kind,
        "status": active,
    });
    let LinkManager {
        changing,
        changed_id,
        ..
    } = manager;
    html::tr().child((
        html::td()
            .class("blog-title-cell")
            .child(text(&draft, "title")),
        html::td().child(text(&draft, "link_type")),
        html::td().child(if active {
            t("公开", "Public")
        } else {
            t("待审核/隐藏", "Pending review / hidden")
        }),
        html::td().child(
            html::div().class("blog-actions blog-flow").child((
                html::button()
                    .child(t("编辑", "Edit"))
                    .class("blog-button")
                    .disabled(move || changing.busy.get() && changed_id.get() == Some(id))
                    .on(ev::click, move |_| manager.edit(id, &draft)),
                can_approve.then(|| {
                    html::button()
                        .child(t("审核通过", "Approve"))
                        .class("blog-button")
                        .disabled(move || changing.busy.get())
                        .on(ev::click, move |_| manager.change(id, RowChange::Approve))
                }),
                html::button()
                    .child(t("删除", "Delete"))
                    .class("blog-button blog-danger")
                    .disabled(move || changing.busy.get())
                    .on(ev::click, move |_| manager.change(id, RowChange::Delete)),
            )),
        ),
    ))
}

fn links_table(manager: LinkManager) -> impl IntoView {
    move || {
        manager.data.with_current(|value| {
            let rows = page_items(value?)
                .iter()
                .map(|item| link_row(manager, item))
                .collect_view();
            let headers = [
                t("标题", "Title"),
                t("类型", "Type"),
                t("状态", "Status"),
                t("操作", "Actions"),
            ]
            .map(|label| html::th().child(label));
            Some(framed_table(
                t("内容资源列表", "Content resource list"),
                headers,
                rows,
            ))
        })
    }
}

#[component]
pub(super) fn Links() -> impl IntoView {
    let page = RwSignal::new(1u64);
    let kind = RwSignal::new(String::new());
    let status = RwSignal::new(-1i64);
    let data = load(Signal::derive(move || {
        let kind = kind.get();
        let status = status.get();
        let kind_query = if kind.is_empty() {
            String::new()
        } else {
            format!("&kind={}", enc(&kind))
        };
        let status_query = if status < 0 {
            String::new()
        } else {
            format!("&status={status}")
        };
        format!(
            "{LINKS}?page={}&size=20{kind_query}{status_query}",
            page.get()
        )
    }));
    let manager = LinkManager::new(data);
    html::section()
        .aria_label(t("内容资源", "Content resources"))
        .child((
            html::p().class("blog-row").child(t(
                "维护友链、收藏、照片和音乐条目。",
                "Manage friend links, favorites, photos, and music.",
            )),
            data.admin_feedback(None),
            manager.saving.feedback(),
            manager.changing.feedback(),
            html::div().class("blog-stack blog-stack--spaced").child((
                link_filters(page, kind, status),
                html::fieldset()
                    .class("blog-fieldset blog-stack blog-stack--spaced")
                    .disabled(move || manager.saving.busy.get())
                    .child((
                        link_form(manager),
                        links_table(manager),
                        ListState(
                            component_props_builder(&ListState)
                                .data(data)
                                .empty(t("暂无内容资源", "No content resources yet"))
                                .build(),
                        ),
                        Pager(
                            component_props_builder(&Pager)
                                .data(data)
                                .page(page)
                                .build(),
                        ),
                    )),
            )),
        ))
        .class("blog-content-stack blog-stack")
}
