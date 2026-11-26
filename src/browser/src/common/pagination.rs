//! Pagination controls preserve the caller's page and read-state policy.
use super::navigation::replace_browse;
use super::paged_list::{PageMetadata, last_page};
use super::{Data, browse, page_number, t, tr};
use blog_interface::prelude::*;
use blog_interface::{ev, html};

fn page_metadata<T: PageMetadata>(value: Option<&T>) -> Option<(u64, u64)> {
    value.map(PageMetadata::counts)
}

#[component]
pub(crate) fn Pager<T: PageMetadata + Send + Sync + 'static>(
    data: Data<T>,
    page: RwSignal<u64>,
    #[prop(default=t("分页","Pagination"))] label: String,
    #[prop(default = false)] public: bool,
    #[prop(default = false)] preserve: bool,
    #[prop(default=Signal::derive(||false))] locked: Signal<bool>,
) -> impl IntoView {
    Effect::new(move |_| {
        if let Some((total, size)) = data.with_current(page_metadata) {
            let last = last_page(total, size);
            if page.get_untracked() > last {
                page.set(last);
            }
        }
    });
    move || {
        let metadata = if preserve {
            data.with_latest(page_metadata)
        } else {
            data.with_current(page_metadata)
        };
        metadata
            .filter(|(total, size)| total > size || page.get() > 1)
            .map(|(total, size)| {
                View::new(
                    blog_interface::html::nav()
                        .child((
                            blog_interface::html::button()
                                .child(t("上一页", "Previous page"))
                                .class(if public { "" } else { "blog-button" })
                                .r#type("button")
                                .disabled(move || {
                                    locked.get()
                                        || (!preserve && data.busy.get())
                                        || page.get() <= 1
                                })
                                .on(blog_interface::ev::click, move |_| {
                                    page.update(|p| *p = p.saturating_sub(1).max(1));
                                }),
                            blog_interface::html::span().child(tr(
                                "第 {0} 页 · 共 {1} 条",
                                "Page {0} · {1} items",
                                &[page.get().to_string(), total.to_string()],
                            )),
                            blog_interface::html::button()
                                .child(t("下一页", "Next page"))
                                .class(if public { "" } else { "blog-button" })
                                .r#type("button")
                                .disabled(move || {
                                    locked.get()
                                        || (!preserve && data.busy.get())
                                        || page.get() >= last_page(total, size)
                                })
                                .on(blog_interface::ev::click, move |_| page.update(|p| *p += 1)),
                        ))
                        .class("pager")
                        .aria_label(label.clone()),
                )
            })
    }
}

#[component]
pub(crate) fn BrowsePager<T: PageMetadata + Send + Sync + 'static>(
    data: Data<T>,
    state: RwSignal<String>,
    size: u64,
    #[prop(default=t("分页","Pagination"))] label: String,
    #[prop(default = false)] always: bool,
    #[prop(default = false)] empty: bool,
) -> impl IntoView {
    Effect::new(move |_| {
        if let Some((total, size)) = data.with_current(page_metadata) {
            let last = last_page(total, size);
            if untrack(move || page_number(state)) > last {
                replace_browse(state, &[("page", (last > 1).then(|| last.to_string()))]);
            }
        }
    });
    move || {
        data.with_current(page_metadata)
            .filter(|(total, _)| {
                empty || *total > if always { 0 } else { size } || page_number(state) > 1
            })
            .map(|(total, size)| {
                let last = last_page(total, size);
                View::new(
                    html::nav()
                        .child((
                            html::button()
                                .child(t("上一页", "Previous page"))
                                .r#type("button")
                                .disabled(move || data.busy.get() || page_number(state) <= 1)
                                .on(ev::click, move |_| {
                                    let page = page_number(state).saturating_sub(1).max(1);
                                    browse(
                                        state,
                                        &[("page", (page > 1).then(|| page.to_string()))],
                                    );
                                }),
                            html::span().child(tr(
                                "第 {0} 页",
                                "Page {0}",
                                &[page_number(state).to_string()],
                            )),
                            html::button()
                                .child(t("下一页", "Next page"))
                                .r#type("button")
                                .disabled(move || data.busy.get() || page_number(state) >= last)
                                .on(ev::click, move |_| {
                                    browse(
                                        state,
                                        &[("page", Some((page_number(state) + 1).to_string()))],
                                    );
                                }),
                        ))
                        .class("pager")
                        .aria_label(label.clone()),
                )
            })
    }
}
