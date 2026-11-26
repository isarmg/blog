//! Pagination controls preserve the caller's page and read-state policy.
use super::navigation::replace_browse;
use super::paged_list::{PageMetadata, last_page};
use super::{Data, browse, page_number, t, tr};
use blog_interface::prelude::*;
use blog_interface::{ev, html};

fn page_metadata<T: PageMetadata>(value: Option<&T>) -> Option<(u64, u64)> {
    value.map(PageMetadata::counts)
}

fn controls(
    label: String,
    caption: String,
    page: Signal<u64>,
    last: u64,
    locked: Signal<bool>,
    change: Callback<u64>,
    button_class: Option<&'static str>,
) -> impl IntoView {
    html::nav()
        .child((
            html::button()
                .child(t("上一页", "Previous page"))
                .class(button_class)
                .r#type("button")
                .disabled(move || locked.get() || page.get() <= 1)
                .on(ev::click, move |_| {
                    change.run(page.get_untracked().saturating_sub(1).max(1));
                }),
            html::span().child(caption),
            html::button()
                .child(t("下一页", "Next page"))
                .class(button_class)
                .r#type("button")
                .disabled(move || locked.get() || page.get() >= last)
                .on(ev::click, move |_| change.run(page.get_untracked() + 1)),
        ))
        .class("pager")
        .aria_label(label)
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
    let current = Signal::derive(move || page.get());
    let blocked = Signal::derive(move || locked.get() || (!preserve && data.busy.get()));
    let change = Callback::new(move |value| page.set(value));
    move || {
        let metadata = if preserve {
            data.with_latest(page_metadata)
        } else {
            data.with_current(page_metadata)
        };
        metadata
            .filter(|(total, size)| total > size || page.get() > 1)
            .map(|(total, size)| {
                let caption = tr(
                    "第 {0} 页 · 共 {1} 条",
                    "Page {0} · {1} items",
                    &[page.get().to_string(), total.to_string()],
                );
                controls(
                    label.clone(),
                    caption,
                    current,
                    last_page(total, size),
                    blocked,
                    change,
                    Some(if public { "" } else { "blog-button" }),
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
    let current = Signal::derive(move || page_number(state));
    Effect::new(move |_| {
        if let Some((total, size)) = data.with_current(page_metadata) {
            let last = last_page(total, size);
            if current.get_untracked() > last {
                replace_browse(state, &[("page", (last > 1).then(|| last.to_string()))]);
            }
        }
    });
    let blocked = Signal::derive(move || data.busy.get());
    let change = Callback::new(move |value: u64| {
        browse(state, &[("page", (value > 1).then(|| value.to_string()))]);
    });
    move || {
        data.with_current(page_metadata)
            .filter(|(total, _)| {
                empty || *total > if always { 0 } else { size } || current.get() > 1
            })
            .map(|(total, size)| {
                let caption = tr("第 {0} 页", "Page {0}", &[current.get().to_string()]);
                controls(
                    label.clone(),
                    caption,
                    current,
                    last_page(total, size),
                    blocked,
                    change,
                    None,
                )
            })
    }
}
