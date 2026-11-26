//! Favorite links, friend links and music browsing.
use super::appearance::{background_style, site_media_url};
use super::shell::{Chrome, site_data};
use crate::common::{
    BrowsePager, Data, browse, load, page_items, page_number, param, safe_url, search_signal, t,
    text,
};
use crate::models::Site;
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::ev;
use blog_interface::html;
use blog_interface::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;

const PAGE_SIZE: u64 = 24;

#[component]
pub(super) fn Toolbox() -> impl IntoView {
    let state = search_signal();
    let tab = Signal::derive(move || {
        let value = param(state, "tab");
        if value == "friends" || value == "music" {
            value
        } else {
            "favorites".into()
        }
    });
    let data = load(Signal::derive(move || {
        let kind = match tab.get().as_str() {
            "friends" => "friendUrl",
            "music" => "funny",
            _ => "favorites",
        };
        format!(
            "/api/v1/links/page?kind={kind}&page={}&size={PAGE_SIZE}",
            page_number(state)
        )
    }));
    let site = site_data();
    View::new(Chrome(
        component_props_builder(&Chrome)
            .plain(true)
            .title(Signal::derive(|| t("百宝箱", "Toolbox")))
            .children(ToChildren::to_children(move || {
                html::div().class("favorite-page").child((
                    toolbox_banner(site, tab, state),
                    html::div().class("favorite-groups").child((
                        data.public_feedback(Some(t("内容加载失败", "Unable to load links"))),
                        move || match tab.get().as_str() {
                            "music" => data
                                .current_items()
                                .map(|items| {
                                    Music(component_props_builder(&Music).items(items).build())
                                })
                                .into_any(),
                            tab => link_groups(data, site, tab == "friends"),
                        },
                        BrowsePager(
                            component_props_builder(&BrowsePager)
                                .data(data)
                                .state(state)
                                .size(PAGE_SIZE)
                                .label(t("百宝箱分页", "Toolbox pages"))
                                .build(),
                        ),
                    )),
                ))
            }))
            .build(),
    ))
}

fn toolbox_banner(site: Data<Site>, tab: Signal<String>, state: RwSignal<String>) -> impl IntoView {
    let background = move || site_media_url(site, |site| site.media.toolbox_background.as_deref());
    let reduced_motion = window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    let tabs = [
        (
            "favorites",
            t("收藏夹", "Favorites"),
            t(
                "将本网站添加到您的收藏夹吧",
                "Add this site to your favorites",
            ),
        ),
        (
            "music",
            t("音乐", "Music"),
            t(
                "一曲肝肠断，天涯何处觅知音",
                "Music brings kindred spirits together",
            ),
        ),
        (
            "friends",
            t("友人帐", "Friends"),
            t("看看朋友们的网站", "Explore our friends’ websites"),
        ),
    ];
    html::section()
        .class("favorite-banner")
        .style(move || background_style(background()))
        .child((
            move || {
                site_media_url(site, |site| site.media.toolbox_video.as_deref()).map(|url| {
                    html::video()
                        .autoplay(!reduced_motion)
                        .muted(true)
                        .r#loop(!reduced_motion)
                        .playsinline(true)
                        .controls(reduced_motion)
                        .preload(if reduced_motion { "none" } else { "metadata" })
                        .poster(background())
                        .src(url)
                })
            },
            html::div().class("favorite-banner-overlay").child((
                html::small().child(t("记录", "Articles")),
                html::h1().child(t("百宝箱", "Toolbox")),
                html::div().class("favorite-feature-links").child(
                    tabs.into_iter()
                        .map(|(id, label, description)| {
                            html::button()
                                .child((
                                    html::strong().child(label),
                                    html::span().child(description),
                                ))
                                .class(move || if tab.get() == id { "active" } else { "" })
                                .aria_pressed(move || (tab.get() == id).to_string())
                                .on(ev::click, move |_| {
                                    browse(state, &[("tab", Some(id.into())), ("page", None)]);
                                })
                        })
                        .collect_view(),
                ),
            )),
        ))
}

fn site_info_row(label: String, value: impl IntoView) -> impl IntoView {
    html::p().child((html::b().child(label), value))
}

fn friend_site_info(site: Data<Site>) -> impl IntoView {
    let (name, avatar, description) = site.with_current(|value| {
        value.map_or_else(
            || {
                (
                    "blog".to_owned(),
                    None,
                    t("有诗意地记录生活", "Record life through poetry"),
                )
            },
            |value| {
                (
                    value.web_name.clone().unwrap_or_default(),
                    value.avatar.as_deref().and_then(|url| safe_url(url, false)),
                    value.web_title.clone().unwrap_or_default(),
                )
            },
        )
    });
    let origin = window().location().origin().unwrap_or_default();
    html::section().class("friend-site-info").child((
        html::h2().child(t("🌸 本站信息", "🌸 Site information")),
        html::div().child((
            site_info_row(t("网站名称", "Site name"), html::span().child(name)),
            site_info_row(
                t("网址", "URL"),
                html::a().href(origin.clone()).child(origin),
            ),
            avatar.map(|url| {
                site_info_row(
                    t("头像", "Avatar"),
                    html::a()
                        .child(t("查看头像", "View avatar"))
                        .href(url)
                        .target("_blank")
                        .rel("noopener noreferrer"),
                )
            }),
            site_info_row(t("描述", "Description"), html::span().child(description)),
        )),
    ))
}

fn link_groups(data: Data, site: Data<Site>, friends: bool) -> AnyView {
    data.with_current(|value| {
        let items = value.map(page_items);
        let featured = [
            t("♥️青出于蓝", "♥️ Inspiration"),
            t("🥇友情链接", "🥇 Friend links"),
        ];
        let fallback = if friends {
            featured[1].clone()
        } else {
            t("我的收藏", "My favorites")
        };
        let mut groups = BTreeMap::<&str, Vec<&Value>>::new();
        for item in items.unwrap_or_default() {
            let name = item["classify"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(&fallback);
            groups.entry(name).or_default().push(item);
        }
        let first = friends.then(|| {
            featured.map(|name| {
                let items = groups.remove(name.as_str()).unwrap_or_default();
                favorite_group(name, items)
            })
        });
        let groups = first
            .into_iter()
            .flatten()
            .chain(
                groups
                    .into_iter()
                    .map(|(name, items)| favorite_group(name.to_owned(), items)),
            )
            .collect_view();
        if friends {
            html::div()
                .class("friend-page")
                .child((friend_site_info(site), groups))
                .into_any()
        } else {
            (
                groups,
                items.is_some_and(<[Value]>::is_empty).then(|| {
                    html::p()
                        .child(t("还没有收藏的内容。", "No favorites yet."))
                        .class("favorite-empty")
                }),
            )
                .into_any()
        }
    })
}

fn favorite_group(name: String, items: Vec<&Value>) -> impl IntoView + use<> {
    let empty = items.is_empty();
    html::section().class("favorite-link-group").child((
        html::h2().child(name),
        html::div()
            .class("favorite-link-grid")
            .child(items.into_iter().map(favorite_link).collect_view()),
        empty.then(|| {
            html::p()
                .child(t("这里还没有友链。", "No friend links yet."))
                .class("favorite-group-empty")
        }),
    ))
}

fn favorite_link(item: &Value) -> impl IntoView + use<> {
    let href = safe_url(&text(item, "url"), false).map(|url| crate::common::href(&url));
    let target = href.is_some().then_some("_blank");
    let cover = safe_url(&text(item, "cover"), false).map_or_else(
        || html::span().class("favorite-link-placeholder").into_any(),
        |url| html::img().src(url).alt("").loading("lazy").into_any(),
    );
    let title = text(item, "title");
    html::a()
        .class("favorite-link-card")
        .href(href)
        .target(target)
        .rel("noopener noreferrer")
        .child((
            cover,
            html::span().child((
                html::strong().child(if title.is_empty() {
                    t("未命名", "Untitled")
                } else {
                    title
                }),
                html::small().child(text(item, "introduction")),
            )),
        ))
}

#[component]
fn Music(items: Vec<Value>) -> impl IntoView {
    let items = StoredValue::new(
        items
            .into_iter()
            .filter(|i| safe_url(&text(i, "url"), false).is_some())
            .collect::<Vec<_>>(),
    );
    let selected = RwSignal::new(0usize);
    View::new(
        html::div()
            .child((
                html::div()
                    .child((
                        items.get_value().is_empty().then(|| {
                            View::new(html::h2().child(t("选择一首曲目", "Select a track")))
                        }),
                        move || {
                            items.get_value().get(selected.get()).cloned().map(|item| {
                                View::new((
                                    html::h2().child(text(&item, "title")),
                                    safe_url(&text(&item, "cover"), false).map(|u| {
                                        View::new(html::img().class("music-cover").src(u).alt(""))
                                    }),
                                    html::audio()
                                        .controls(true)
                                        .src(safe_url(&text(&item, "url"), false))
                                        .on(ev::ended, move |_| {
                                            selected.update(|i| {
                                                *i = (*i + 1)
                                                    .min(items.get_value().len().saturating_sub(1));
                                            });
                                        }),
                                ))
                            })
                        },
                    ))
                    .class("wall-card"),
                html::div()
                    .child((
                        html::h2().child(t("播放列表", "Playlist")),
                        items
                            .get_value()
                            .into_iter()
                            .enumerate()
                            .map(|(i, item)| {
                                View::new(
                                    html::button()
                                        .child((
                                            text(&item, "title"),
                                            html::small().child(text(&item, "classify")),
                                        ))
                                        .class(
                                            move || {
                                                if selected.get() == i { "selected" } else { "" }
                                            },
                                        )
                                        .on(ev::click, move |_| selected.set(i)),
                                )
                            })
                            .collect_view(),
                        items
                            .get_value()
                            .is_empty()
                            .then(|| View::new(html::p().child(t("暂无曲目。", "No tracks yet.")))),
                    ))
                    .class("wall-card"),
            ))
            .class("music-layout"),
    )
}
