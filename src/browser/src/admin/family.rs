use crate::common::{Action, Data, Field, confirm, flag, form_check, load, num, t, text};
use blog_interface::components::notify;
use blog_interface::prelude::*;
use blog_interface::{ev, html};
use serde_json::{Map, Value};

const ENDPOINT: &str = "/api/v1/content/family";
const NICKNAME: Field = Field::RequiredLine(32);
const SHORT: Field = Field::Line(32);
const IMAGE: Field = Field::Line(Field::DEFAULT_LIMIT);
const INPUTS: [(&str, &str, &str, Field); 9] = [
    ("man_name", "男生昵称", "His name", NICKNAME),
    ("woman_name", "女生昵称", "Her name", NICKNAME),
    ("timing", "起始时间", "Start date", SHORT),
    ("countdown_title", "倒计时标题", "Countdown title", SHORT),
    ("countdown_time", "倒计时时间", "Countdown date", SHORT),
    ("bg_cover", "背景图片", "Background image", IMAGE),
    ("man_cover", "男生头像", "His avatar", IMAGE),
    ("woman_cover", "女生头像", "Her avatar", IMAGE),
    (
        "family_info",
        "简介",
        "Introduction",
        Field::Paragraph(1024),
    ),
];

fn family_form(item: Option<&Value>) -> Value {
    let mut form = INPUTS
        .into_iter()
        .map(|(key, _, _, _)| {
            let value = item.map_or_else(String::new, |item| text(item, key));
            (key.to_owned(), Value::String(value))
        })
        .collect::<Map<_, _>>();
    form.insert(
        "status".into(),
        Value::Bool(item.is_none_or(|item| flag(item, "status"))),
    );
    Value::Object(form)
}

fn submit(data: Data, form: RwSignal<Value>, editing: RwSignal<Option<i64>>, save: Action) {
    if data.with_current_untracked(|value| value.is_none()) {
        return;
    }
    let mut payload = form.get_untracked();
    for key in [
        "bg_cover",
        "man_cover",
        "woman_cover",
        "countdown_title",
        "countdown_time",
        "family_info",
    ] {
        if payload[key].as_str().unwrap_or_default().is_empty() {
            payload[key] = Value::Null;
        }
    }
    let (path, method) = editing.get_untracked().map_or_else(
        || (ENDPOINT.to_owned(), "POST"),
        |id| (format!("{ENDPOINT}/{id}"), "PUT"),
    );
    save.run(path, method, Some(payload), move |value| {
        editing.set(Some(num(&value, "id")));
        form.set(family_form(Some(&value)));
        notify(t("家的资料已保存", "Home details saved"));
    });
}

fn family_editor(
    data: Data,
    form: RwSignal<Value>,
    editing: RwSignal<Option<i64>>,
    save: Action,
) -> impl IntoView {
    let [
        his_name,
        her_name,
        start,
        countdown,
        deadline,
        background,
        his_avatar,
        her_avatar,
        info,
    ] = INPUTS.map(|(key, zh, en, field)| field.bind(form, key, t(zh, en)));
    let remove = move || {
        editing.get().map(|id| {
            html::button()
                .child(t("删除家的资料", "Delete home details"))
                .class("blog-button blog-danger")
                .r#type("button")
                .on(ev::click, move |_| {
                    if confirm(&t("确定删除家的资料？", "Delete your home details?")) {
                        save.run(format!("{ENDPOINT}/{id}"), "DELETE", None, move |_| {
                            data.reload();
                        });
                    }
                })
        })
    };
    html::form()
        .class("editor-form blog-frame blog-frame--padded blog-stack")
        .child((
            html::h2().class("blog-row").child(move || {
                if editing.get().is_some() {
                    t("编辑家的资料", "Edit home details")
                } else {
                    t("创建我的家", "Create my home")
                }
            }),
            html::div().class("form-grid").child((his_name, her_name)),
            html::div().class("form-grid").child((start, countdown)),
            deadline,
            background,
            html::div()
                .class("form-grid")
                .child((his_avatar, her_avatar)),
            info,
            form_check(form, "status", t(" 公开显示", " Show publicly")),
            html::div().class("blog-actions blog-flow").child((
                html::button()
                    .child(t("保存", "Save"))
                    .class("blog-button")
                    .r#type("submit"),
                remove,
            )),
        ))
        .on(ev::submit, move |event| {
            event.prevent_default();
            submit(data, form, editing, save);
        })
}

#[component]
pub(super) fn FamilySettings() -> impl IntoView {
    let data = load(Signal::derive(|| ENDPOINT.into()));
    let form = RwSignal::new(family_form(None));
    let editing = RwSignal::new(None::<i64>);
    let save = Action::new();
    Effect::new(move |_| {
        let projection = data.with_current(|value| {
            value.map(|value| {
                let item = value.as_array().and_then(|items| items.first());
                (item.map(|item| num(item, "id")), family_form(item))
            })
        });
        if let Some((id, draft)) = projection {
            editing.set(id);
            form.set(draft);
        }
    });
    html::section()
        .aria_label(t("恋爱笔记", "Love journal"))
        .class("blog-content-stack blog-stack")
        .child((
            html::p().class("blog-row").child(t(
                "这里仅由管理员创建并维护一份家的资料。",
                "The administrator creates and maintains one home here.",
            )),
            data.admin_feedback(None),
            save.feedback(),
            html::fieldset()
                .class("blog-fieldset blog-stack")
                .disabled(move || save.busy.get() || data.with_current(|value| value.is_none()))
                .child(family_editor(data, form, editing, save)),
        ))
}
