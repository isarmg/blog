//! Photo browsing, category filters and paged collection state.
use super::shell::Chrome;
use crate::common::{
    Data, Lightbox, enc, last_page, load, load_typed, merge_page, page_items, safe_url, t, text,
    unsigned_field,
};
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::prelude::*;
use blog_interface::view_tree::either::Either;
use blog_interface::{ev, html};
use serde_json::Value;

#[derive(serde::Deserialize)]
struct PhotoClass {
    classify: String,
    count: i64,
}

struct PhotoPages {
    items: Vec<Value>,
    number: u64,
    total: u64,
    has_more: bool,
}

#[derive(Clone, Copy)]
struct Album {
    classes: Data<Vec<PhotoClass>>,
    photos: Data,
    selected: RwSignal<String>,
    pages: RwSignal<Option<PhotoPages>>,
    all_total: RwSignal<Option<u64>>,
    advance: Callback<()>,
}

impl Album {
    fn load(love: bool, active: Signal<bool>) -> Self {
        let classes = load_typed::<Vec<PhotoClass>>(Signal::derive(|| {
            "/api/v1/links/classes?kind=lovePhoto".into()
        }));
        let selected = RwSignal::new(String::new());
        let requested = RwSignal::new(1u64);
        let pages = RwSignal::new(None::<PhotoPages>);
        let all_total = RwSignal::new(None::<u64>);
        Effect::new(move |_| {
            if !love {
                let first = classes.with_current(|value| {
                    value
                        .and_then(|entries| entries.first())
                        .map_or_default(|entry| entry.classify.clone())
                });
                if !first.is_empty() && selected.with_untracked(String::is_empty) {
                    selected.set(first);
                }
            }
        });
        Effect::new(move |_| {
            selected.track();
            requested.set(1);
            pages.set(None);
        });
        let photos = load(Signal::derive(move || {
            if !active.get() {
                return String::new();
            }
            let number = requested.get();
            let mut path = format!("/api/v1/links/page?kind=lovePhoto&page={number}&size=12");
            selected.with(|name| {
                if !name.is_empty() {
                    path.push_str("&classify=");
                    path.push_str(&enc(name));
                }
            });
            path
        }));
        Effect::new(move |_| {
            let response = photos.with_current(|value| {
                let value = value?;
                let next = page_items(value);
                let number = unsigned_field(value, "page");
                let size = unsigned_field(value, "size");
                let total = unsigned_field(value, "total");
                // Check the server page before merging any overlapping photos.
                let has_more =
                    usize::try_from(size) == Ok(next.len()) && number < last_page(total, size);
                Some((number, total, has_more, next.to_vec()))
            });
            let Some((number, total, has_more, next)) = response else {
                return;
            };
            if selected.with_untracked(String::is_empty) {
                all_total.set(Some(total));
            }
            pages.update(|pages| {
                let mut items = pages.take().map_or_else(Vec::new, |last| last.items);
                merge_page(&mut items, number, next);
                *pages = Some(PhotoPages {
                    items,
                    number,
                    total,
                    has_more,
                });
            });
        });
        let advance = Callback::new(move |()| {
            requested.set(
                pages.with_untracked(|pages| pages.as_ref().map_or(1, |last| last.number + 1)),
            );
            photos.reload();
        });
        Self {
            classes,
            photos,
            selected,
            pages,
            all_total,
            advance,
        }
    }
}

fn photo_card(item: &Value, lightbox: RwSignal<Option<String>>) -> impl IntoView + use<> {
    let cover = safe_url(&text(item, "cover"), true);
    let disabled = cover.is_none();
    let title = ["title", "classify"]
        .into_iter()
        .find_map(|key| item[key].as_str().filter(|value| !value.is_empty()))
        .map_or_else(|| t("相册照片", "Album photo"), str::to_owned);
    let label = format!("{}{}", t("查看照片：", "View photo: "), title);
    let date = text(item, "create_time")
        .get(..10)
        .unwrap_or_default()
        .to_owned();
    let image = if let Some(url) = cover.clone() {
        html::img()
            .src(url)
            .alt("")
            .loading("lazy")
            .decoding("async")
            .into_any()
    } else {
        html::span()
            .child(t("记录美好瞬间", "Capture beautiful moments"))
            .into_any()
    };
    html::button()
        .child((
            html::span().child(image).class("travel-photo-image"),
            html::span()
                .child((
                    html::strong().child(title),
                    html::small().child((t("日期（UTC）：", "Date (UTC): "), date)),
                ))
                .class("travel-photo-body"),
        ))
        .class("travel-photo-card")
        .r#type("button")
        .disabled(disabled)
        .aria_label(label)
        .on(ev::click, move |_| lightbox.set(cover.clone()))
}

fn photo_grid(
    pages: RwSignal<Option<PhotoPages>>,
    lightbox: RwSignal<Option<String>>,
) -> impl IntoView {
    html::div()
        .child(move || {
            pages.with(|pages| {
                if let Some(page) = pages.as_ref().filter(|page| !page.items.is_empty()) {
                    return Either::Left(
                        page.items
                            .iter()
                            .map(|item| photo_card(item, lightbox))
                            .collect_view(),
                    );
                }
                Either::Right(
                    html::div()
                        .child((
                            html::span().child("▧").aria_hidden("true"),
                            html::h3()
                                .child(t("这一组还没有照片", "No photos in this collection yet")),
                            html::p().child(t(
                                "切换其它相册，或稍后回来看看新的回忆。",
                                "Try another collection, or come back for new memories.",
                            )),
                        ))
                        .class("travel-photo-empty public-empty-state"),
                )
            })
        })
        .class("travel-grid")
}

fn album_empty(pages: RwSignal<Option<PhotoPages>>) -> bool {
    pages.with(|pages| pages.as_ref().is_none_or(|last| last.items.is_empty()))
}

fn category_button(
    selected: RwSignal<String>,
    category: String,
    label: String,
    count: impl IntoView,
) -> impl IntoView {
    let active = category.clone();
    let is_current = move || selected.with(|name| *name == active);
    let pressed = is_current.clone();
    html::button()
        .child((label, " ", html::small().child(count)))
        .class(move || if is_current() { "active" } else { "" })
        .r#type("button")
        .aria_pressed(move || pressed().to_string())
        .on(ev::click, move |_| selected.set(category.clone()))
}

fn filters(album: Album, love: bool) -> impl IntoView {
    let label = if love {
        t("全部", "All")
    } else {
        t("全部照片", "All photos")
    };
    html::div()
        .child((
            category_button(album.selected, String::new(), label, move || {
                album.all_total.get().map_or_default(|n| n.to_string())
            }),
            move || {
                album.classes.with_current(|value| {
                    value
                        .into_iter()
                        .flatten()
                        .map(|entry| {
                            let name = entry.classify.clone();
                            category_button(album.selected, name.clone(), name, entry.count)
                        })
                        .collect_view()
                })
            },
        ))
        .class(if love {
            "original-tag-panel love-photo-filters"
        } else {
            "original-tag-panel"
        })
        .aria_label(t("相册分类", "Photo categories"))
}

fn heading(album: Album, love: bool) -> impl IntoView {
    html::div()
        .child((
            html::div().child((
                html::h2().child(move || {
                    let name = album.selected.get();
                    if !name.is_empty() {
                        name
                    } else if love {
                        t("时光相册", "Photo album")
                    } else {
                        t("全部照片", "All photos")
                    }
                }),
                love.then(|| {
                    html::p().child(t(
                        "收藏一起看过的风景。",
                        "Keep the places and moments we have shared.",
                    ))
                }),
            )),
            move || {
                if !love {
                    return None;
                }
                let total = album
                    .pages
                    .with(|pages| pages.as_ref().map(|last| last.total))?;
                Some(html::span().child(format!("{total} {}", t("张照片", "photos"))))
            },
        ))
        .class("love-section-heading")
}

fn pagination(album: Album, love: bool) -> impl IntoView {
    let label = if love {
        t("加载更多照片", "Load more photos")
    } else {
        t("下一页", "Next page")
    };
    move || {
        let has_more = album
            .pages
            .with(|pages| pages.as_ref().map(|last| last.has_more))?;
        Some((
            has_more.then(|| {
                html::button()
                    .child(label.clone())
                    .class("travel-more")
                    .r#type("button")
                    .disabled(move || album.photos.busy.get())
                    .on(ev::click, move |_| album.advance.run(()))
            }),
            (!has_more && !love).then(|| {
                html::p()
                    .child(t("~~到底啦~~", "~~You have reached the end~~"))
                    .class("travel-end")
            }),
        ))
    }
}

#[component]
pub(super) fn Photos() -> impl IntoView {
    View::new(Chrome(
        component_props_builder(&Chrome)
            .plain(true)
            .title(Signal::derive(|| t("时光相册", "Photo album")))
            .children(ToChildren::to_children(move || {
                html::div()
                    .child((
                        html::section()
                            .child(html::div().child((
                                html::h1().child(t("时光相册", "Photo album")),
                                html::h2().child(t(
                                    "每一张照片都是一次美好的记忆",
                                    "Every photo holds a memory",
                                )),
                            )))
                            .class("travel-banner"),
                        html::div()
                            .child(Collection(
                                component_props_builder(&Collection)
                                    .love(false)
                                    .active(Signal::derive(|| true))
                                    .build(),
                            ))
                            .class("travel-content"),
                    ))
                    .class("travel-page")
            }))
            .build(),
    ))
}

#[component]
pub(super) fn Collection(love: bool, active: Signal<bool>) -> impl IntoView {
    let album = Album::load(love, active);
    let lightbox = RwSignal::new(None::<String>);
    html::div()
        .child((
            filters(album, love),
            album.classes.public_feedback(Some(t(
                "相册分类加载失败",
                "Unable to load photo categories",
            ))),
            heading(album, love),
            album
                .photos
                .public_feedback(Some(t("相册加载失败", "Unable to load photos"))),
            Show(
                component_props_builder(&Show)
                    .when(move || {
                        !album_empty(album.pages)
                            || (!album.photos.busy.get()
                                && album.photos.error.with(Option::is_none))
                    })
                    .fallback(move || {
                        move || {
                            album.photos.busy.get().then(|| {
                                html::p()
                                    .child(t("正在加载照片…", "Loading photos…"))
                                    .class("public-loading")
                                    .role("status")
                            })
                        }
                    })
                    .children(ToChildren::to_children(move || {
                        photo_grid(album.pages, lightbox)
                    }))
                    .build(),
            ),
            pagination(album, love),
            Lightbox(component_props_builder(&Lightbox).src(lightbox).build()),
        ))
        .class(if love {
            "love-photo-collection"
        } else {
            "travel-photo-collection"
        })
}
