//! Chronological articles and short journal notes.
use super::shell::{Chrome, site_data};
use crate::common::{
    BrowsePager, Data, Lightbox, Petals, Sliding, href, load, num, page_items, page_number,
    safe_url, search_signal, t, text,
};
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::ev;
use blog_interface::html;
use blog_interface::prelude::*;
use serde_json::Value;

fn journey_grid(page: &Value) -> impl IntoView + use<> {
    let items = page_items(page);
    let first = num(page, "total") - (num(page, "page") - 1) * num(page, "size");
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let offset = i64::try_from(i).expect("the response contract limits page size");
            journey_card(item, i, i + 1 < items.len(), first - offset)
        })
        .collect_view()
}

fn journey_card(item: &Value, index: usize, has_next: bool, number: i64) -> impl IntoView + use<> {
    let row = index / 5;
    let column = index % 5;
    let reverse = row % 2 == 1;
    let time = text(item, "create_time");
    let cover = safe_url(&text(item, "article_cover"), false)
        .map(|url| html::img().src(url).alt("").loading("lazy"));
    let connector = has_next.then(|| {
        html::span()
            .class(if column == 4 {
                "journey-connector journey-turn"
            } else {
                "journey-connector"
            })
            .aria_hidden("true")
    });

    html::a()
        .child((
            html::span().class("journey-image").child((
                html::span().class("journey-picture").child(cover),
                connector,
            )),
            html::strong().child(Sliding(
                component_props_builder(&Sliding)
                    .title(text(item, "article_title"))
                    .build(),
            )),
            html::small().child((
                html::time()
                    .child(crate::dates::format(&time))
                    .datetime(time),
                html::span().child(number),
            )),
        ))
        .style(format!(
            "grid-column:{};grid-row:{}",
            if reverse { 5 - column } else { column + 1 },
            row + 1
        ))
        .class(if reverse {
            "journey-item journey-reverse"
        } else {
            "journey-item"
        })
        .href(href(&format!("/article/{}", num(item, "id"))))
}

#[component]
pub(super) fn Journey() -> impl IntoView {
    let site = site_data();
    let state = search_signal();
    let data = load(Signal::derive(move || {
        format!("/api/v1/articles?page={}&size=20", page_number(state))
    }));
    View::new(Chrome(
        component_props_builder(&Chrome)
            .title(Signal::derive(|| "blog".into()))
            .cover(Signal::derive(move || {
                site.current()
                    .and_then(|site| site.media.journey_background)
                    .unwrap_or_default()
            }))
            .inherit_background(false)
            .children(ToChildren::to_children(move || {
                (
                    Petals(),
                    html::div()
                        .child((
                            html::header().child((
                                html::strong().child(t("时间线", "Timeline")),
                                html::small().child(t("灵魂在路上", "The soul is walking")),
                            )),
                            data.public_feedback(None),
                            html::div()
                                .child(move || data.with_current(|value| value.map(journey_grid)))
                                .class("journey-grid"),
                            BrowsePager(
                                component_props_builder(&BrowsePager)
                                    .data(data)
                                    .state(state)
                                    .size(20)
                                    .build(),
                            ),
                        ))
                        .class("journey-page"),
                )
            }))
            .build(),
    ))
}

#[component]
pub(super) fn Journal() -> impl IntoView {
    let site = site_data();
    let state = search_signal();
    let data = load(Signal::derive(move || {
        format!("/api/v1/notes/page?page={}", page_number(state))
    }));
    let lightbox = RwSignal::new(None::<String>);
    View::new(Chrome(
        component_props_builder(&Chrome)
            .title(Signal::derive(|| "blog".into()))
            .cover(Signal::derive(move || {
                site.current()
                    .and_then(|site| site.media.journal_background)
                    .unwrap_or_default()
            }))
            .inherit_background(false)
            .children(ToChildren::to_children(move || {
                (
                    html::div()
                        .child(
                            html::div()
                                .child((
                                    data.public_feedback(None),
                                    note_cards(data, lightbox, false),
                                    move || {
                                        data.with_current(|v| {
                                            v.is_some_and(|v| page_items(v).is_empty())
                                        })
                                        .then(|| {
                                            View::new(
    html::div()
        .child(
            (
                    html::h3()
                    .child( t("还没有记录。", "No entries yet.")),
                    html::p()
                    .child( t("新的日常记录正在路上。",
                    "New everyday stories are on their way."))
                ,),
        )
        .class("public-empty-state"),
)
                                        })
                                    },
                                    BrowsePager(
                                        component_props_builder(&BrowsePager)
                                            .data(data)
                                            .state(state)
                                            .size(10)
                                            .label(t("记录分页", "Entry pages"))
                                            .build(),
                                    ),
                                ))
                                .class("wall-list"),
                        )
                        .class("wall-layout wall-reading-layout"),
                    Lightbox(component_props_builder(&Lightbox).src(lightbox).build()),
                )
            }))
            .build(),
    ))
}

pub(super) fn note_cards(
    data: Data,
    lightbox: RwSignal<Option<String>>,
    love: bool,
) -> impl IntoView {
    move || {
        data.with_current(|v| {
            v.map_or_else(Vec::new, |page| {
                page_items(page)
                    .iter()
                    .map(|item| note_card(item, lightbox, love))
                    .collect_view()
            })
        })
    }
}

fn note_card(
    item: &Value,
    lightbox: RwSignal<Option<String>>,
    love: bool,
) -> impl IntoView + use<> {
    let field = |key: &str| item[key].as_str().unwrap_or_default();
    let name = field("username");
    let author = if name.is_empty() {
        t("站长", "Site owner")
    } else {
        name.to_owned()
    };
    let image = safe_url(field("image_path"), true).map(|url| {
        let click = url.clone();
        html::button()
            .child(
                html::img()
                    .src(url)
                    .alt(if love {
                        t("随笔图片", "Note image")
                    } else {
                        t("动态图片", "Post image")
                    })
                    .loading("lazy"),
            )
            .class("wall-photo-button")
            .r#type("button")
            .aria_label(t("放大图片", "Enlarge image"))
            .on(ev::click, move |_| lightbox.set(Some(click.clone())))
    });
    html::article()
        .child((
            html::small().child((author, " · ", crate::dates::format(field("create_time")))),
            html::p().child(field("content").to_owned()),
            image,
        ))
        .class(if love {
            "love-moment-card"
        } else {
            "wall-card"
        })
}
