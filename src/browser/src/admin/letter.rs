use crate::common::{Action, Field, form_check, href, load, t, text, validation_message};
use blog_interface::components::notify;
use blog_interface::prelude::*;
use blog_interface::{ev, html};
use serde_json::Value;
use wasm_bindgen::JsCast;

const FIELDS: [(&str, &str, &str, Field); 3] = [
    ("recipient", "收信人", "Recipient", Field::RequiredLine(100)),
    ("sender", "署名", "Sender", Field::RequiredLine(100)),
    (
        "content",
        "信笺正文",
        "Letter content",
        Field::RequiredParagraph(16_384),
    ),
];

fn validate(draft: &Value, node: NodeRef<html::Form>, validation: RwSignal<String>) -> bool {
    for (key, zh, en, field) in FIELDS {
        let length = text(draft, key).trim().chars().count();
        if (1..=field.limit() as usize).contains(&length) {
            continue;
        }
        validation.set(crate::common::tr(
            "{0}须包含 1–{1} 个字符。",
            "{0} must contain 1–{1} characters.",
            &[t(zh, en), field.limit().to_string()],
        ));
        if let Some(input) = node
            .get_untracked()
            .and_then(|form| form.query_selector(&format!("[name={key}]")).ok().flatten())
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            && let Err(error) = input.focus()
        {
            web_sys::console::warn_2(&"Unable to move browser focus".into(), &error);
        }
        return false;
    }
    validation.set(String::new());
    true
}

#[component]
pub(super) fn LetterSettings() -> impl IntoView {
    let data = load(Signal::derive(|| "/api/v1/content/letter".into()));
    let form = RwSignal::new(Value::Null);
    let original = RwSignal::new(Value::Null);
    let save = Action::new();
    let form_node = NodeRef::<html::Form>::new();
    let validation = RwSignal::new(String::new());
    let ready = Memo::new(move |_| !form.get().is_null());
    Effect::new(move |_| {
        if let Some(v) = data.current() {
            form.set(v.clone());
            original.set(v);
            validation.set(String::new());
        }
    });
    html::section()
        .aria_label(t("信笺管理", "Letter"))
        .child((
            html::p().class("blog-row").child(t(
                "维护公开信笺的收信人、署名和正文。",
                "Manage the recipient, sender, and content of the public letter.",
            )),
            data.admin_feedback(None),
            save.feedback(),
            validation_message(validation),
            move || {
                ready.get().then(|| {
                    html::form()
                        .node_ref(form_node)
                        .child(
                            html::fieldset()
                                .child((
                                    FIELDS.map(|(key, zh, en, field)| {
                                        field.bind(form, key, t(zh, en))
                                    }),
                                    form_check(form, "is_public", t(" 公开显示", " Show publicly")),
                                    html::div()
                                        .child((
                                            html::button()
                                                .child(t("保存信笺", "Save letter"))
                                                .class("blog-button")
                                                .r#type("submit")
                                                .disabled(move || save.busy.get()),
                                            html::button()
                                                .child(t("撤销修改", "Reset changes"))
                                                .class("blog-button")
                                                .r#type("button")
                                                .disabled(move || {
                                                    save.busy.get() || form.get() == original.get()
                                                })
                                                .on(ev::click, move |_| {
                                                    form.set(original.get_untracked());
                                                    save.error.set(None);
                                                    validation.set(String::new());
                                                }),
                                            html::a()
                                                .class("blog-button")
                                                .child(t("查看公开页面", "View public page"))
                                                .href(href("/letter"))
                                                .target("_blank")
                                                .rel("noopener noreferrer"),
                                        ))
                                        .class("blog-actions blog-flow"),
                                ))
                                .class("blog-fieldset blog-stack")
                                .disabled(move || save.busy.get()),
                        )
                        .class("editor-form blog-frame blog-frame--padded blog-stack")
                        .on(ev::input, move |_| {
                            validation.set(String::new());
                        })
                        .on(ev::submit, move |ev| {
                            ev.prevent_default();
                            if save.busy.get_untracked() {
                                return;
                            }
                            let f = form.get_untracked();
                            if validate(&f, form_node, validation) {
                                save.run(
                                    "/api/v1/content/letter".into(),
                                    "PUT",
                                    Some(f),
                                    move |v| {
                                        form.set(v.clone());
                                        original.set(v);
                                        notify(t("信笺已保存", "Letter saved"));
                                    },
                                );
                            }
                        })
                })
            },
        ))
        .class("blog-content-stack blog-stack")
}
