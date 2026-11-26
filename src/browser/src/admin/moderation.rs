use super::Page;
use crate::common::{
    Action, Data, FilePicker, Lightbox, ListState, Pager, load, nullable_num, num, page_items,
    safe_url, t, table_time, text, tr,
};
use blog_interface::component::component_props_builder as props;
use blog_interface::components::notify;
use blog_interface::prelude::*;
use blog_interface::thread_bound::SendWrapper;
use blog_interface::{
    ev,
    html::{self, td},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ModerationKind {
    Comments,
    Messages,
    Files,
}
use ModerationKind::{Comments, Files, Messages};

impl ModerationKind {
    fn page(self) -> Page {
        match self {
            Comments => Page::Comments,
            Messages => Page::Messages,
            Files => Page::Files,
        }
    }

    fn columns(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Comments => &[
                ("来源", "Source"),
                ("用户", "Users"),
                ("内容", "Content"),
                ("时间", "Time"),
            ],
            Messages => &[
                ("ID", "ID"),
                ("作者", "Author"),
                ("内容", "Content"),
                ("图片", "Image"),
                ("时间", "Time"),
            ],
            Files => &[
                ("名称", "Name"),
                ("类型", "Type"),
                ("大小", "Size"),
                ("位置", "Location"),
            ],
        }
    }
}

fn moderation_author(item: &Value, kind: ModerationKind) -> String {
    if let Some(username) = item["username"].as_str().filter(|name| !name.is_empty()) {
        return username.to_owned();
    }
    match nullable_num(item, "user_id") {
        Some(user) if kind == Messages => tr("用户 {0}", "User {0}", &[user.to_string()]),
        Some(user) => user.to_string(),
        None => t("匿名访客", "Anonymous guest"),
    }
}

fn external_link(label: String, url: String) -> impl IntoView {
    html::a()
        .class("blog-button")
        .child(label)
        .href(url)
        .target("_blank")
        .rel("noopener noreferrer")
}

fn comment_source(item: &Value) -> AnyView {
    match text(item, "comment_type").as_str() {
        "message" => t("留言板", "Message board").into_any(),
        "love" => external_link(t("祝福板", "Wishes"), "/love".into()).into_any(),
        _ => {
            let source = num(item, "source");
            external_link(
                format!("{}{source}", t("文章 #", "Article #")),
                format!("/article/{source}"),
            )
            .into_any()
        }
    }
}

fn photo(item: &Value, lightbox: RwSignal<Option<String>>) -> impl IntoView + use<> {
    safe_url(&text(item, "image_path"), true).map(|url| {
        let image = html::img()
            .src(url.clone())
            .alt(t("查看留言图片", "View message image"));
        html::button()
            .class("moderation-photo")
            .r#type("button")
            .child(image)
            .on(ev::click, move |_| lightbox.set(Some(url.clone())))
    })
}

fn row(
    kind: ModerationKind,
    item: &Value,
    data: Data,
    action: Action,
    locked: Signal<bool>,
    lightbox: RwSignal<Option<String>>,
) -> impl IntoView + use<> {
    let id = num(item, "id");
    let path = text(item, "path");
    let content = match kind {
        Files => (
            td().class("blog-title-cell")
                .child(text(item, "original_name")),
            td().child(text(item, "resource_type")),
            td().child(nullable_num(item, "size").unwrap_or(0)),
            td().child(safe_url(&path, false).map(|url| external_link(t("打开", "Open"), url))),
        )
            .into_any(),
        Comments | Messages => {
            let message = kind == Messages;
            let source = if message {
                id.into_any()
            } else {
                comment_source(item)
            };
            let field = if message {
                "message"
            } else {
                "comment_content"
            };
            let content = text(item, field);
            let content = if message && content.is_empty() {
                t("图片留言", "Image message")
            } else {
                content
            };
            (
                td().child(source),
                td().child(moderation_author(item, kind)),
                td().class("comment-cell").child(content),
                message.then(|| td().child(photo(item, lightbox))),
                td().child(table_time(&text(item, "create_time"))),
            )
                .into_any()
        }
    };
    let missing_path = kind == Files && path.is_empty();
    html::tr().child((
        content,
        td().child(html::button().class("blog-button blog-danger").child(t("删除", "Delete"))
            .disabled(move || locked.get() || missing_path)
            .on(ev::click, move |_| {
                let prompt = match kind {
                    Comments => t("删除这条评论？如果是顶层评论，会同时删除该楼层的全部回复。", "Delete this comment? If it is a top-level comment, all replies in its thread will also be deleted."),
                    Files => t("确定删除这个文件及其媒体数据？", "Delete this file and its media data?"),
                    Messages => t("确定删除这条留言？", "Delete this message?"),
                };
                if window().confirm_with_message(&prompt).unwrap_or(false) {
                    let body = (kind == Files).then(|| json!({"path": path}));
                    action.run(format!("/api/v1/content/{}/{id}", kind.page().hash()), "DELETE", body, move |_| data.reload());
                }
            })),
    ))
}

fn moderation_table(
    kind: ModerationKind,
    data: Data,
    action: Action,
    locked: Signal<bool>,
    lightbox: RwSignal<Option<String>>,
) -> impl IntoView {
    let title = match kind {
        Comments => t("评论列表", "Comment list"),
        Messages => t("留言列表", "Message list"),
        Files => t("文件列表", "File list"),
    };
    move || {
        data.with_current(|value| {
            let rows = page_items(value?)
                .iter()
                .map(|item| row(kind, item, data, action, locked, lightbox))
                .collect_view();
            let headers = kind
                .columns()
                .iter()
                .copied()
                .chain([("操作", "Actions")])
                .map(|(zh, en)| html::th().child(t(zh, en)))
                .collect_view();
            let table = html::table()
                .class("blog-table")
                .aria_label(title.clone())
                .child((
                    html::thead().child(html::tr().child(headers)),
                    html::tbody().child(rows),
                ));
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

fn upload_form(data: Data, upload: Action, locked: Signal<bool>) -> impl IntoView {
    let chosen = RwSignal::new(SendWrapper::new(None::<web_sys::File>));
    let filename =
        move || chosen.with(|file| file.as_ref().map_or_else(String::new, web_sys::File::name));
    let uploaded = RwSignal::new(None::<String>);
    let picker = FilePicker(
        props(&FilePicker)
            .label(t(
                "上传图片（PNG、JPEG、GIF、WebP）",
                "Upload image (PNG, JPEG, GIF, WebP)",
            ))
            .filename(Signal::derive(filename))
            .disabled(locked)
            .choose(Callback::new(move |file: SendWrapper<web_sys::File>| {
                chosen.set(SendWrapper::new(Some(file.take())));
            }))
            .build(),
    );
    let button = html::button()
        .class("blog-button")
        .r#type("submit")
        .disabled(move || filename().is_empty() || upload.busy.get())
        .child(move || {
            if upload.busy.get() {
                t("上传中…", "Uploading…")
            } else {
                t("上传", "Upload")
            }
        });
    let form = html::form()
        .class("inline-form blog-frame blog-frame--padded blog-flow blog-form-flow")
        .child((picker, button))
        .on(ev::submit, move |event| {
            event.prevent_default();
            if let Some(file) = chosen.with_untracked(|file| file.as_ref().cloned()) {
                upload.upload(SendWrapper::new(file), move |path| {
                    uploaded.set((!path.is_empty()).then_some(path));
                    chosen.set(SendWrapper::new(None));
                    data.reload();
                    notify(t("图片已上传", "Image uploaded"));
                });
            }
        });
    (
        html::fieldset()
            .class("blog-fieldset blog-stack")
            .disabled(move || locked.get())
            .child(form),
        move || {
            uploaded.get().map(|url| {
                html::p().class("blog-row").child((
                    t("新图片地址：", "New image URL: "),
                    html::code().child(url),
                ))
            })
        },
    )
}

#[component]
pub(super) fn Moderation(kind: ModerationKind) -> impl IntoView {
    let endpoint = kind.page().hash();
    let page = RwSignal::new(1u64);
    let data = load(Signal::derive(move || {
        format!("/api/v1/content/{endpoint}?page={}&size=20", page.get())
    }));
    let action = Action::new();
    let upload = Action::new();
    let locked = Signal::derive(move || upload.busy.get() || action.busy.get());
    let lightbox = RwSignal::new(None::<String>);
    let empty = match kind {
        Comments => t("暂无评论", "No comments yet"),
        Messages => t("暂无留言", "No messages yet"),
        Files => t("暂无文件", "No files yet"),
    };
    html::section()
        .aria_label(kind.page().label())
        .class("blog-content-stack blog-stack")
        .child((
            data.admin_feedback(None),
            action.feedback(),
            upload.feedback(),
            html::div().class("blog-stack blog-stack--spaced").child((
                (kind == Files).then(|| upload_form(data, upload, locked)),
                moderation_table(kind, data, action, locked, lightbox),
                ListState(props(&ListState).data(data).empty(empty).build()),
                Pager(props(&Pager).data(data).page(page).build()),
            )),
            Lightbox(props(&Lightbox).src(lightbox).build()),
        ))
}
