use crate::common::{
    Action, ControlledField, Feedback, Field, FilePicker, load, safe_url, t, text,
    validation_message,
};
use blog_interface::component::component_props_builder;
use blog_interface::components::notify;
use blog_interface::prelude::*;
use blog_interface::{ev, html};
use serde_json::{Map, Value};
use wasm_bindgen::JsCast;

fn media_field_value(form: &Value, key: &str, nested: bool) -> String {
    text(if nested { &form["media"] } else { form }, key)
}

fn set_media_field(form: &mut Value, key: &str, nested: bool, value: Option<String>) {
    let target = if nested {
        let media = &mut form["media"];
        if !media.is_object() {
            *media = Value::Object(Map::new());
        }
        media
    } else {
        form
    };
    target[key] = value.map_or(Value::Null, Value::String);
}

#[component]
fn SiteMediaField(
    label: String,
    key: &'static str,
    form: RwSignal<Value>,
    upload: Action,
    disabled: Signal<bool>,
    #[prop(default = true)] nested: bool,
    #[prop(default = false)] video: bool,
) -> impl IntoView {
    let value = Signal::derive(move || form.with(|form| media_field_value(form, key, nested)));
    let filename = RwSignal::new(String::new());
    Effect::new(move |_| {
        if !upload.busy.get() {
            filename.set(String::new());
        }
    });
    let preview_label = format!("{}{}", label, t("预览", " preview"));
    let clear_label = format!("{}{}", t("清除：", "Clear "), label);
    let picker_label = format!("{}{}", t("上传：", "Upload "), label);
    View::new(
        blog_interface::html::section()
            .class("site-media-field")
            .child((
                ControlledField(
                    component_props_builder(&ControlledField)
                        .label(label)
                        .value(value)
                        .max(2048)
                        .change(Callback::new(move |next: String| {
                            if !disabled.get_untracked() {
                                form.update(|form| set_media_field(form, key, nested, Some(next)));
                            }
                        }))
                        .build(),
                ),
                FilePicker(
                    component_props_builder(&FilePicker)
                        .label(picker_label)
                        .disabled(disabled)
                        .filename(Signal::derive(move || filename.get()))
                        .accept(
                            if video {
                                "video/mp4,video/webm"
                            } else {
                                "image/png,image/jpeg,image/gif,image/webp"
                            }
                            .into(),
                        )
                        .choose(Callback::new(
                            move |file: blog_interface::thread_bound::SendWrapper<
                                web_sys::File,
                            >| {
                                if disabled.get_untracked() {
                                    return;
                                }
                                let previous = value.get_untracked();
                                let name = file.name();
                                let done = move |path: String| {
                                    form.update(|form| {
                                        if media_field_value(form, key, nested) == previous {
                                            set_media_field(form, key, nested, Some(path));
                                        }
                                    });
                                    filename.set(String::new());
                                };
                                if video {
                                    upload.upload_video(file, done);
                                } else {
                                    upload.upload(file, done);
                                }
                                if upload.busy.get_untracked() {
                                    filename.set(name);
                                }
                            },
                        ))
                        .build(),
                ),
                blog_interface::html::button()
                    .class("blog-button")
                    .r#type("button")
                    .child(t("清除", "Clear"))
                    .aria_label(clear_label)
                    .disabled(move || disabled.get() || value.get().is_empty())
                    .on(blog_interface::ev::click, move |_| {
                        if !disabled.get_untracked() {
                            form.update(|form| set_media_field(form, key, nested, None));
                            filename.set(String::new());
                        }
                    }),
                move || {
                    safe_url(&value.get(), false).map(|url| {
                        if video {
                            View::new(
                                blog_interface::html::video()
                                    .class("site-media-preview")
                                    .src(url)
                                    .controls(true)
                                    .preload("metadata")
                                    .aria_label(preview_label.clone()),
                            )
                            .into_any()
                        } else {
                            View::new(
                                blog_interface::html::img()
                                    .class("site-media-preview")
                                    .src(url)
                                    .alt(preview_label.clone())
                                    .loading("lazy"),
                            )
                            .into_any()
                        }
                    })
                },
            )),
    )
}

fn settings_panel(title: String, content: impl IntoView) -> impl IntoView {
    html::section()
        .class("blog-content-panel blog-frame blog-frame--padded blog-stack")
        .child((html::h2().class("blog-row").child(title), content))
}

fn settings_help(message: String) -> impl IntoView {
    html::p().class("blog-field-help blog-row").child(message)
}

fn site_appearance(form: RwSignal<Value>, upload: Action, disabled: Signal<bool>) -> impl IntoView {
    let root_images = [
        ("background_image", "背景图片", "Background image"),
        ("avatar", "站长头像", "Site owner avatar"),
    ];
    let nested_images = [
        ("logo", "网站标识", "Site logo"),
        ("journey_background", "时间轴背景", "Journey background"),
        ("journal_background", "随笔背景", "Journal background"),
        (
            "guestbook_background",
            "留言板背景",
            "Message board background",
        ),
        (
            "love_letter_background",
            "恋爱信笺背景",
            "Love letter background",
        ),
        ("love_moments_cover", "点点滴滴封面", "Love moments cover"),
        ("love_photos_cover", "时光相册封面", "Love photos cover"),
        ("love_wishes_cover", "祝福墙封面", "Love wishes cover"),
        (
            "toolbox_background",
            "百宝箱背景图片",
            "Toolbox background image",
        ),
    ];
    let media = root_images
        .into_iter()
        .map(|field| (field, false, false))
        .chain(nested_images.into_iter().map(|field| (field, true, false)))
        .chain([(
            (
                "toolbox_video",
                "百宝箱背景视频",
                "Toolbox background video",
            ),
            true,
            true,
        )]);
    settings_panel(
        t("外观媒体", "Appearance media"),
        (
            settings_help(t(
                "填写图片或视频地址，或直接上传文件。留空时使用主题配色或文字，不加载默认媒体。恋爱页头图和双方头像在恋爱笔记中设置。图片最大 10 MB，MP4 / WebM 视频最大 50 MB。",
                "Enter an image or video URL, or upload a file. Leave empty to use theme colors or text, without bundled media. Set the love journal header and couple avatars in Love journal. Images: up to 10 MB; MP4 / WebM videos: up to 50 MB.",
            )),
            html::div().class("form-grid site-media-grid").child(
                media
                    .map(move |((key, chinese, english), nested, video)| {
                        SiteMediaField(
                            component_props_builder(&SiteMediaField)
                                .label(t(chinese, english))
                                .key(key)
                                .form(form)
                                .upload(upload)
                                .disabled(disabled)
                                .nested(nested)
                                .video(video)
                                .build(),
                        )
                    })
                    .collect_view(),
            ),
            html::p()
                .class("blog-row")
                .role("status")
                .aria_live("polite")
                .child(move || {
                    if upload.busy.get() {
                        t("正在上传媒体…", "Uploading media…")
                    } else {
                        String::new()
                    }
                }),
        ),
    )
}

fn basic_settings(form: RwSignal<Value>) -> impl IntoView {
    let fields = [
        (
            "web_name",
            t("网站名称", "Site name"),
            Field::RequiredLine(Field::DEFAULT_LIMIT),
        ),
        (
            "web_title",
            t("网站标题", "Site title"),
            Field::Line(Field::DEFAULT_LIMIT),
        ),
        (
            "author_name",
            t("站长显示名", "Site owner display name"),
            Field::RequiredLine(100),
        ),
    ]
    .map(|(key, label, field)| field.bind(form, key, label));
    let [promotion_title, promotion_cover, promotion_link] =
        ["推送标题：", "推送封面：", "推送链接："].map(|label| html::code().child(label));
    settings_panel(
        t("基本信息", "Basic information"),
        (
            html::div().child(fields).class("form-grid"),
            settings_help(t(
                "用于文章和随笔署名；管理员登录名与密码在右上角头像的账号设置中修改。",
                "Used as the author of articles and notes. Change administrator sign-in credentials through the account settings avatar.",
            )),
            Field::Paragraph(Field::DEFAULT_LIMIT).bind(form, "notices", t("公告", "Notices")),
            settings_help(t(
                "使用 JSON 字符串数组或每行一条公告。",
                "Use a JSON string array or one notice per line.",
            )),
            html::details().child((
                html::summary()
                    .class("blog-row")
                    .child(t("推送格式", "Promotion format")),
                html::p().class("blog-row").child((
                    promotion_title,
                    " ",
                    promotion_cover,
                    " ",
                    promotion_link,
                )),
            )),
        ),
    )
}

fn save_site(
    form: RwSignal<Value>,
    save: Action,
    validation: RwSignal<String>,
    form_node: NodeRef<html::Form>,
) {
    let mut payload = form.get_untracked();
    let name = text(&payload, "author_name").trim().to_owned();
    if !(1..=100).contains(&name.chars().count()) {
        validation.set(t(
            "站长显示名须包含 1–100 个字符。",
            "Site owner display name must contain 1–100 characters.",
        ));
        if let Some(field) = form_node
            .get_untracked()
            .and_then(|node| node.query_selector("[name=author_name]").ok().flatten())
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            && let Err(error) = field.focus()
        {
            web_sys::console::warn_2(&"Unable to move browser focus".into(), &error);
        }
        return;
    }
    validation.set(String::new());
    payload["author_name"] = Value::String(name);
    save.run(
        "/api/v1/content/site".into(),
        "PUT",
        Some(payload),
        move |saved| {
            form.set(saved);
            notify(t("站点设置已保存", "Site settings saved"));
        },
    );
}

fn settings_form(
    form: RwSignal<Value>,
    save: Action,
    upload: Action,
    validation: RwSignal<String>,
) -> impl IntoView {
    let form_node = NodeRef::<html::Form>::new();
    let disabled = Signal::derive(move || save.busy.get() || upload.busy.get());
    let covers = t("随机封面", "Random covers");
    html::form()
        .node_ref(form_node)
        .child((
            html::fieldset()
                .child((
                    basic_settings(form),
                    site_appearance(form, upload, disabled),
                    settings_panel(
                        covers.clone(),
                        (
                            settings_help(t(
                                "使用 JSON 图片地址数组或每行一个图片地址。",
                                "Use a JSON array of image URLs or one image URL per line.",
                            )),
                            Field::Paragraph(Field::DEFAULT_LIMIT).bind(
                                form,
                                "random_cover",
                                covers,
                            ),
                        ),
                    ),
                ))
                .class("blog-fieldset blog-stack blog-stack--spaced")
                .disabled(move || disabled.get())
                .aria_label(t("网站设置", "Site settings")),
            upload.feedback(),
            html::button()
                .class("blog-button")
                .r#type("button")
                .child(t("取消上传", "Cancel upload"))
                .disabled(move || !upload.busy.get())
                .on(ev::click, move |_| upload.abort()),
            html::div()
                .child(
                    html::button()
                        .child(t("保存设置", "Save settings"))
                        .class("blog-button")
                        .r#type("submit")
                        .disabled(move || disabled.get()),
                )
                .class("blog-actions blog-flow"),
        ))
        .class("blog-settings-form blog-stack")
        .on(ev::input, move |_| validation.set(String::new()))
        .on(ev::submit, move |event| {
            event.prevent_default();
            if !disabled.get_untracked() {
                save_site(form, save, validation, form_node);
            }
        })
}

#[component]
pub(super) fn SiteSettings() -> impl IntoView {
    let data = load(Signal::derive(|| "/api/v1/content/site".into()));
    let form = RwSignal::new(Value::Null);
    let save = Action::new();
    let upload = Action::new();
    let validation = RwSignal::new(String::new());
    let ready = Memo::new(move |_| form.with(|value| !value.is_null()));
    Effect::new(move |_| {
        if let Some(v) = data.current() {
            upload.abort();
            validation.set(String::new());
            form.set(v);
        }
    });
    html::section()
        .aria_label(t("网站设置", "Site settings"))
        .child((
            Feedback(
                component_props_builder(&Feedback)
                    .error(data.error)
                    .retry(Some(Callback::new(move |()| {
                        upload.abort();
                        data.reload();
                    })))
                    .build(),
            ),
            save.feedback(),
            validation_message(validation),
            move || {
                ready
                    .get()
                    .then(|| settings_form(form, save, upload, validation))
            },
        ))
        .class("blog-content-stack blog-stack")
}
