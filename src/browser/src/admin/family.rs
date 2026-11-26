use crate::common::{Action, Field, flag, form_check, load, num, t, text};
use blog_interface::components::notify;
use blog_interface::prelude::*;
use serde_json::{Value, json};

fn empty_family() -> Value {
    json!({"bg_cover":"","man_cover":"","woman_cover":"","man_name":"","woman_name":"","timing":"","countdown_title":"","countdown_time":"","family_info":"","status":true})
}

fn family_form(item: &Value) -> Value {
    let mut form = empty_family();
    for key in [
        "bg_cover",
        "man_cover",
        "woman_cover",
        "man_name",
        "woman_name",
        "timing",
        "countdown_title",
        "countdown_time",
        "family_info",
    ] {
        form[key] = json!(text(item, key));
    }
    form["status"] = json!(flag(item, "status"));
    form
}

#[component]
pub(super) fn FamilySettings() -> impl IntoView {
    let data = load(Signal::derive(|| "/api/v1/content/family".into()));
    let form = RwSignal::new(empty_family());
    let editing = RwSignal::new(None::<i64>);
    let save = Action::new();
    Effect::new(move |_| {
        if let Some(v) = data.current() {
            if let Some(item) = v.as_array().and_then(|a| a.first()) {
                editing.set(Some(num(item, "id")));
                form.set(family_form(item));
            } else {
                editing.set(None);
                form.set(empty_family());
            }
        }
    });
    View::new(
        blog_interface::html::section()
            .aria_label(t("恋爱笔记", "Love journal"))
            .child((
                blog_interface::html::p().class("blog-row").child(t(
                    "这里仅由管理员创建并维护一份家的资料。",
                    "The administrator creates and maintains one home here.",
                )),
                data.admin_feedback(None),
                save.feedback(),
                blog_interface::html::fieldset()
                    .child(
                        blog_interface::html::form()
                            .child((
                                blog_interface::html::h2().class("blog-row").child(move || {
                                    if editing.get().is_some() {
                                        t("编辑家的资料", "Edit home details")
                                    } else {
                                        t("创建我的家", "Create my home")
                                    }
                                }),
                                blog_interface::html::div()
                                    .child((
                                        Field::RequiredLine(32).bind(
                                            form,
                                            "man_name",
                                            t("男生昵称", "His name"),
                                        ),
                                        Field::RequiredLine(32).bind(
                                            form,
                                            "woman_name",
                                            t("女生昵称", "Her name"),
                                        ),
                                    ))
                                    .class("form-grid"),
                                blog_interface::html::div()
                                    .child((
                                        Field::Line(32).bind(
                                            form,
                                            "timing",
                                            t("起始时间", "Start date"),
                                        ),
                                        Field::Line(32).bind(
                                            form,
                                            "countdown_title",
                                            t("倒计时标题", "Countdown title"),
                                        ),
                                    ))
                                    .class("form-grid"),
                                Field::Line(32).bind(
                                    form,
                                    "countdown_time",
                                    t("倒计时时间", "Countdown date"),
                                ),
                                Field::Line(Field::DEFAULT_LIMIT).bind(
                                    form,
                                    "bg_cover",
                                    t("背景图片", "Background image"),
                                ),
                                blog_interface::html::div()
                                    .child((
                                        Field::Line(Field::DEFAULT_LIMIT).bind(
                                            form,
                                            "man_cover",
                                            t("男生头像", "His avatar"),
                                        ),
                                        Field::Line(Field::DEFAULT_LIMIT).bind(
                                            form,
                                            "woman_cover",
                                            t("女生头像", "Her avatar"),
                                        ),
                                    ))
                                    .class("form-grid"),
                                Field::Paragraph(1024).bind(
                                    form,
                                    "family_info",
                                    t("简介", "Introduction"),
                                ),
                                form_check(form, "status", t(" 公开显示", " Show publicly")),
                                blog_interface::html::div()
                                    .child((
                                        blog_interface::html::button()
                                            .child(t("保存", "Save"))
                                            .class("blog-button")
                                            .r#type("submit"),
                                        move || {
                                            editing.get().map(|id| {
                                                View::new(
                                                    blog_interface::html::button()
                                                        .child(t(
                                                            "删除家的资料",
                                                            "Delete home details",
                                                        ))
                                                        .class("blog-button blog-danger")
                                                        .r#type("button")
                                                        .on(blog_interface::ev::click, move |_| {
                                                            if window()
                                                                .confirm_with_message(&t(
                                                                    "确定删除家的资料？",
                                                                    "Delete your home details?",
                                                                ))
                                                                .unwrap_or(false)
                                                            {
                                                                save.run(
                        format!("/api/v1/content/family/{id}"),
                        "DELETE",
                        None,
                        move |_| data.reload(),
                    );
                                                            }
                                                        }),
                                                )
                                            })
                                        },
                                    ))
                                    .class("blog-actions blog-flow"),
                            ))
                            .class("editor-form blog-frame blog-frame--padded blog-stack")
                            .on(blog_interface::ev::submit, move |ev| {
                                ev.prevent_default();
                                if data.with_current_untracked(|value| value.is_none()) {
                                    return;
                                }
                                let mut payload = form.get_untracked();
                                if let Some(o) = payload.as_object_mut() {
                                    o.remove("id");
                                }
                                for k in [
                                    "bg_cover",
                                    "man_cover",
                                    "woman_cover",
                                    "countdown_title",
                                    "countdown_time",
                                    "family_info",
                                ] {
                                    if text(&payload, k).is_empty() {
                                        payload[k] = Value::Null;
                                    }
                                }
                                let path = editing
                                    .get_untracked()
                                    .map_or("/api/v1/content/family".into(), |id| {
                                        format!("/api/v1/content/family/{id}")
                                    });
                                save.run(
                                    path,
                                    if editing.get_untracked().is_some() {
                                        "PUT"
                                    } else {
                                        "POST"
                                    },
                                    Some(payload),
                                    move |v| {
                                        editing.set(Some(num(&v, "id")));
                                        form.set(family_form(&v));
                                        notify(t("家的资料已保存", "Home details saved"));
                                    },
                                );
                            }),
                    )
                    .class("blog-fieldset blog-stack")
                    .disabled(move || {
                        save.busy.get() || data.with_current(|value| value.is_none())
                    }),
            ))
            .class("blog-content-stack blog-stack"),
    )
}
