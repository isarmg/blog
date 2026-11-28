use crate::common::{
    Pager, RequestOptions, confirm, load, merge_page, num, page_items, public_error, request,
    safe_url, t, text, unsigned_field,
};
use blog_interface::component::component_props_builder;
use blog_interface::prelude::*;
use blog_interface::thread_bound::SendWrapper;
use blog_interface::{ev, html};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentKind {
    Article,
    Message,
    Love,
}

impl CommentKind {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Article => "/api/v1/comments",
            Self::Message => "/api/v1/message-comments",
            Self::Love => "/api/v1/love-comments",
        }
    }
}
fn commenter(name: &str) -> String {
    if name.is_empty() {
        t("匿名访客", "Anonymous guest")
    } else {
        name.into()
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingSubmission {
    request_id: String,
    content: String,
    #[serde(deserialize_with = "required_parent")]
    parent_comment_id: Option<i64>,
    #[serde(
        default,
        deserialize_with = "present_article",
        skip_serializing_if = "Option::is_none"
    )]
    article_id: Option<i64>,
}

fn required_parent<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<i64>::deserialize(d)
}

fn present_article<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    i64::deserialize(d).map(Some)
}

impl PendingSubmission {
    fn parse(raw: &str, target: Option<i64>) -> Result<Self, PendingError> {
        struct PendingObject;
        impl<'de> serde::de::Visitor<'de> for PendingObject {
            type Value = PendingSubmission;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a pending comment object")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                map: M,
            ) -> Result<Self::Value, M::Error> {
                PendingSubmission::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let mut decoder = serde_json::Deserializer::from_str(raw);
        let submission = decoder.deserialize_map(PendingObject)?;
        decoder.end()?;
        let request_id = &submission.request_id;
        let valid_request = request_id.len() == 36
            && request_id
                .bytes()
                .enumerate()
                .all(|(index, byte)| match index {
                    8 | 13 | 18 | 23 => byte == b'-',
                    14 => byte == b'4',
                    19 => matches!(byte, b'8' | b'9' | b'a' | b'b'),
                    _ => byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'),
                });
        let valid_id = |id: i64| (1..=9_007_199_254_740_991).contains(&id);
        if !valid_request
            || submission.content.trim().is_empty()
            || submission.content.chars().count() > 1024
            || submission.parent_comment_id.is_some_and(|id| !valid_id(id))
            || submission.article_id.is_some_and(|id| !valid_id(id))
            || submission.article_id != target
        {
            return Err(PendingError::InvalidSubmission);
        }
        Ok(submission)
    }

    fn json(&self) -> Value {
        serde_json::to_value(self)
            .expect("invariant: a comment submission contains only JSON primitives")
    }

    fn stored_json(&self) -> String {
        serde_json::to_string(self)
            .expect("invariant: a comment submission contains only JSON primitives")
    }
}
#[derive(Debug, thiserror::Error)]
enum PendingError {
    #[error("session storage is unavailable")]
    Unavailable,
    #[error("session storage failed: {0:?}")]
    Browser(wasm_bindgen::JsValue),
    #[error("pending comment is not valid JSON")]
    Json(#[from] serde_json::Error),
    #[error("pending comment has an invalid submission shape")]
    InvalidSubmission,
}

fn storage() -> Result<web_sys::Storage, PendingError> {
    web_sys::window()
        .expect("invariant: comments run in a browser window")
        .session_storage()
        .map_err(PendingError::Browser)?
        .ok_or(PendingError::Unavailable)
}

fn clear_pending(key: &str) -> Result<(), PendingError> {
    storage()?.remove_item(key).map_err(PendingError::Browser)
}

fn read_pending(key: &str, target: Option<i64>) -> Result<Option<PendingSubmission>, PendingError> {
    let Some(raw) = storage()?.get_item(key).map_err(PendingError::Browser)? else {
        return Ok(None);
    };
    PendingSubmission::parse(&raw, target).map(Some)
}
#[derive(Clone)]
struct ReplyTarget {
    id: i64,
    username: String,
}

#[component]
pub(crate) fn Comments(
    #[prop(default = CommentKind::Article)] kind: CommentKind,
    #[prop(default = 0)] article_id: i64,
    #[prop(optional)] title: String,
) -> impl IntoView {
    let path = kind.endpoint();
    let target = match kind {
        CommentKind::Article => Some(article_id),
        CommentKind::Message | CommentKind::Love => None,
    };
    let key = format!("blog.comment-submission:{path}:{article_id}");
    let (restored, unavailable) = match read_pending(&key, target) {
        Ok(pending) => (pending, false),
        Err(_) => (None, true),
    };
    let draft = RwSignal::new(restored.as_ref().map_or_default(|v| v.content.clone()));
    let pending = RwSignal::new(restored);
    let storage_failure = RwSignal::new(unavailable);
    let reply = RwSignal::new(None::<ReplyTarget>);
    let failure = RwSignal::new(String::new());
    let feedback = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let page = RwSignal::new(1u64);
    let url = Signal::derive(move || {
        format!(
            "{path}?page={}&size=10{}",
            page.get(),
            if kind == CommentKind::Article {
                format!("&article_id={article_id}")
            } else {
                String::new()
            }
        )
    });
    let data = load(url);
    let controller = StoredValue::new(SendWrapper::new(None::<web_sys::AbortController>));
    on_cleanup(move || {
        controller.update_value(|c| {
            if let Some(c) = (**c).take() {
                c.abort();
            }
        });
    });
    let key = StoredValue::new(key);
    let choose = Callback::new(move |v: ReplyTarget| {
        if !busy.get_untracked()
            && pending.with_untracked(Option::is_none)
            && !storage_failure.get_untracked()
        {
            reply.set(Some(v));
        }
    });
    View::new(
        blog_interface::html::section()
            .child((
                blog_interface::html::h3().child((
                    if title.is_empty() {
                        t("评论", "Comments")
                    } else {
                        title
                    },
                    " | ",
                    move || {
                        format!(
                            "{} {}",
                            data.with_latest(|v| v.map_or(0, |v| num(v, "total"))),
                            t("条评论", "comments")
                        )
                    },
                )),
                move || {
                    let message = failure.get();
                    (!message.is_empty()).then(|| html::p().child(message).role("alert"))
                },
                data.public_feedback(Some(t("评论加载失败", "Unable to load comments"))),
                move || {
                    data.busy.get().then(|| {
                        View::new(
                            blog_interface::html::p()
                                .child(t("评论加载中…", "Loading comments…"))
                                .role("status"),
                        )
                    })
                },
                move || {
                    storage_failure.get().then(|| {
                        View::new(blog_interface::html::p().child(t("无法读取或保存待确认的提交。请先检查评论，再清除待确认状态；当前不会发送新评论。", "The pending submission could not be read or saved. Check the comments before clearing it. No new comment will be sent.")).role("alert"))
                    })
                },
                move || {
                    (pending.with(Option::is_some) && failure.with(String::is_empty)).then(|| {
                        View::new(blog_interface::html::p().child(t("有一条评论等待确认，请继续确认原提交。", "A comment is awaiting confirmation. Continue confirming the original submission.")).role("status"))
                    })
                },
                For(component_props_builder(&For)
                    .each(move || data.current_items().unwrap_or_default())
                    .key(|v| (num(v, "id"), num(v, "reply_count")))
                    .children(move |item| {
                        View::new(
                            blog_interface::html::div()
                                .child((
                                    comment_entry(&item, choose),
                                    Replies(
                                        component_props_builder(&Replies)
                                            .root(item)
                                            .kind(kind)
                                            .choose(choose)
                                            .refresh(data.revision)
                                            .build(),
                                    ),
                                ))
                                .class("comment-floor"),
                        )
                    })
                    .build()),
                move || {
                    data.with_current(|v| v.is_some_and(|v| page_items(v).is_empty()))
                        .then(|| {
                            View::new(
                                blog_interface::html::p()
                                    .child(t("还没有评论。", "No comments yet."))
                                    .class("muted"),
                            )
                        })
                },
                Pager(
                    component_props_builder(&Pager)
                        .data(data)
                        .page(page)
                        .public(true)
                        .preserve(true)
                        .locked(Signal::derive(move || busy.get()))
                        .label(t("评论分页", "Comment pages"))
                        .build(),
                ),
                blog_interface::html::form()
                    .child((
                        blog_interface::html::p()
                            .child(t("无需登录，以匿名访客身份发表评论。", "Post a comment as an anonymous guest. No account is needed."))
                            .class("comment-identity"),
                        move || {
                            let name = reply.with(|reply| reply.as_ref().map(|reply| commenter(&reply.username)))?;
                            Some(html::button()
                                .child(format!("{}{} · {}", t("回复 ", "Reply to "), name, t("取消", "Cancel")))
                                .class("text-action")
                                .r#type("button")
                                .disabled(move || busy.get() || pending.with(Option::is_some))
                                .on(ev::click, move |_| reply.set(None)))
                        },
                        blog_interface::html::textarea()
                            .disabled(move || {
                                busy.get() || pending.with(Option::is_some) || storage_failure.get()
                            })
                            .aria_label(t("写下你的想法", "Write your thoughts"))
                            .required(true)
                            .maxlength("2048")
                            .prop("value", move || draft.get())
                            .on(blog_interface::ev::input, move |ev| {
                                draft.set(event_target_value(&ev));
                            })
                            .placeholder(t("写下你的想法", "Write your thoughts")),
                        blog_interface::html::button()
                            .child(move || {
                                if busy.get() {
                                    t("发表中…", "Posting…")
                                } else if pending.with(Option::is_some) {
                                    t("确认原提交", "Confirm original submission")
                                } else {
                                    t("匿名发表评论", "Post anonymous comment")
                                }
                            })
                            .r#type("submit")
                            .disabled(move || {
                                busy.get()
                                    || storage_failure.get()
                                    || (pending.with(Option::is_none) && draft.with(|draft| draft.trim().is_empty()))
                            }),
                        move || {
                            (pending.with(Option::is_some) || storage_failure.get()).then(|| {
                                View::new(
                                    blog_interface::html::button()
                                        .child(t("清除待确认状态", "Clear pending submission"))
                                        .r#type("button")
                                        .disabled(move || busy.get())
                                        .on(blog_interface::ev::click, move |_| {
                                            if !confirm(&t("这条评论可能已经发表。请先检查评论列表；确定放弃确认原提交并清空草稿？", "This comment may already be posted. Check the comments first. Stop confirming this submission and clear its draft?"))
                                            {
                                                return;
                                            }
                                            if clear_pending(&key.get_value()).is_err()
                                            {
                                                storage_failure.set(true);
                                                return;
                                            }
                                            pending.set(None);
                                            draft.set(String::new());
                                            reply.set(None);
                                            failure.set(String::new());
                                            storage_failure.set(false);
                                        }),
                                )
                            })
                        },
                        move || {
                            let message = feedback.get();
                            (!message.is_empty()).then(|| html::p().child(message).role("status"))
                        },
                    ))
                    .class("comment-form")
                    .on(blog_interface::ev::submit, move |ev| {
                        ev.prevent_default();
                        if busy.get_untracked() || storage_failure.get_untracked() {
                            return;
                        }
                        let (submission, retry) = if let Some(submission) = pending.get_untracked() { (submission, true) } else {
                            let content = draft.with_untracked(|draft| draft.trim().to_owned());
                            if content.is_empty() {
                                return;
                            }
                            if content.chars().count() > 1024 {
                                failure.set(t(
                                    "评论正文须为 1–1024 个字符",
                                    "Comment content must contain 1–1024 characters",
                                ));
                                return;
                            }
                            let submission = PendingSubmission {
                                request_id: window().crypto().expect("invariant: the application window exposes browser crypto").random_uuid(),
                                content,
                                parent_comment_id: reply.with_untracked(|reply| reply.as_ref().map(|target| target.id)),
                                article_id: target,
                            };
                            (submission, false)
                        };
                        if storage()
                            .and_then(|s| {
                                s.set_item(&key.get_value(), &submission.stored_json())
                                    .map_err(PendingError::Browser)
                            })
                            .is_err()
                        {
                            storage_failure.set(true);
                            return;
                        }
                        pending.set(Some(submission.clone()));
                        busy.set(true);
                        failure.set(String::new());
                        feedback.set(String::new());
                        let c = web_sys::AbortController::new().expect("invariant: AbortController construction has no failure conditions");
                        let signal = c.signal();
                        controller.set_value(SendWrapper::new(Some(c)));
                        blog_interface::task::spawn_local(async move {
                            let result = request(
                                path,
                                "POST",
                                Some(submission.json()),
                                RequestOptions {
                                    signal: Some(signal.clone()),
                                    ..Default::default()
                                },
                            )
                            .await;
                            if signal.aborted() {
                                return;
                            }
                            match result {
                                Ok(v)
                                    if text(&v, "comment_content")
                                        == submission.content.trim() =>
                                {
                                    storage_failure.set(clear_pending(&key.get_value()).is_err());
                                    pending.set(None);
                                    draft.set(String::new());
                                    reply.set(None);
                                    page.set(1);
                                    data.reload();
                                    feedback
                                        .set(t("评论已发表。", "Your comment has been posted."));
                                }
                                Err(e)
                                    if !retry
                                        && [400, 401, 403, 404, 409, 413, 429]
                                            .contains(&e.status)
                                        && e.code != blog_interface::api::ApiErrorCode::Local(blog_interface::api::LocalErrorCode::InvalidErrorResponse) =>
                                {
                                    storage_failure.set(clear_pending(&key.get_value()).is_err());
                                    pending.set(None);
                                    failure.set(public_error(
                                        &e,
                                        &t("请求失败，请重试。", "Request failed. Please retry."),
                                    ));
                                }
                                Ok(_) | Err(_) => failure.set(t("评论提交结果尚未确认。请确认原提交；重试会使用同一请求，不会重复发表。", "Your comment submission is not yet confirmed. Confirm the original submission; retrying uses the same request and will not post it twice.")),
                            }
                            busy.set(false);
                            controller.set_value(SendWrapper::new(None));
                        });
                    }),
            ))
            .class("article-comments"),
    )
}
fn comment_entry(item: &Value, reply: Callback<ReplyTarget>) -> impl IntoView + use<> {
    let target = ReplyTarget {
        id: num(item, "id"),
        username: text(item, "username"),
    };
    let parent = text(item, "parent_username");
    html::article()
        .child((
            html::div()
                .child((
                    safe_url(&text(item, "avatar"), true).map(|url| html::img().src(url).alt("")),
                    html::div().child((
                        html::strong().child(commenter(&target.username)),
                        html::small().child(crate::dates::format(&text(item, "create_time"))),
                    )),
                ))
                .class("public-comment-head"),
            html::p().child((
                (!parent.is_empty()).then(|| {
                    html::span()
                        .child(format!("@{parent} "))
                        .class("comment-mention")
                }),
                text(item, "comment_content"),
            )),
            html::div()
                .child(
                    html::button()
                        .child(t("回复", "Reply"))
                        .r#type("button")
                        .on(ev::click, move |_| reply.run(target.clone())),
                )
                .class("comment-actions"),
        ))
        .class("public-comment")
}

#[component]
fn Replies(
    root: Value,
    kind: CommentKind,
    choose: Callback<ReplyTarget>,
    refresh: RwSignal<u64>,
) -> impl IntoView {
    let id = num(&root, "id");
    let reply_count = unsigned_field(&root, "reply_count");
    let page = RwSignal::new(1);
    let path = Signal::derive(move || {
        refresh.get();
        if reply_count == 0 {
            String::new()
        } else {
            format!(
                "{}/{id}/replies?page={}&size=5",
                kind.endpoint(),
                page.get()
            )
        }
    });
    let data = load(path);
    let items = RwSignal::new(Vec::<Value>::new());
    let expandable = Memo::new(move |_| {
        data.with_latest(|v| v.is_some_and(|v| num(v, "total") > num(v, "size")))
    });
    Effect::new(move |_| {
        if let Some(next) = data.current_items() {
            items.update(|current| merge_page(current, page.get_untracked(), next));
        }
    });
    let remaining = move |response: &Value| {
        let shown = items.with(|list| u64::try_from(list.len()).unwrap_or(u64::MAX));
        unsigned_field(response, "total").saturating_sub(shown)
    };
    let can_load = Signal::derive(move || {
        !data.busy.get() && data.with_current(|v| v.is_some_and(|v| remaining(v) > 0))
    });
    View::new(
        html::div()
            .child((
                data.public_feedback(None),
                move || {
                    items.with(|items| {
                        items
                            .iter()
                            .map(|item| comment_entry(item, choose))
                            .collect_view()
                    })
                },
                move || {
                    expandable.get().then(|| {
                        View::new(
                            html::button()
                                .child(move || {
                                    let remaining = data.with_latest(|v| v.map_or(0, remaining));
                                    if remaining == 0 {
                                        t("已展开全部回复", "All replies shown")
                                    } else {
                                        format!(
                                            "{} {} {}",
                                            t("展开剩余", "Show"),
                                            remaining,
                                            t("条回复", "more replies")
                                        )
                                    }
                                })
                                .class("comment-more")
                                .r#type("button")
                                .aria_disabled(move || (!can_load.get()).to_string())
                                .on(ev::click, move |_| {
                                    if can_load.get_untracked() {
                                        page.update(|p| *p += 1);
                                    }
                                }),
                        )
                    })
                },
            ))
            .class("comment-replies"),
    )
}
#[path = "../../../test/browser/unit/comments/tests.rs"]
#[cfg(test)]
mod tests;
