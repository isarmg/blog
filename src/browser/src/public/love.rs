//! Couple journal, shared photo and note panels, and letter interactions.
use super::appearance::{avatar, background_style, site_media_url};
use super::journal::note_cards;
use super::photos::Collection;
use super::shell::{Chrome, site_data};
use crate::comments::{CommentKind, Comments};
use crate::common::{Lightbox, Pager, Petals, href, list, load, page_items, safe_url, t, text};
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::ev;
use blog_interface::html;
use blog_interface::prelude::*;
use blog_interface::svg;
use blog_interface::thread_bound::SendWrapper;
use serde_json::Value;

fn portrait(name: Signal<String>, family: Signal<Value>, key: &'static str) -> impl IntoView {
    html::div().child((
        move || {
            family.with(|value| {
                avatar(
                    &name.get(),
                    value[key].as_str().unwrap_or_default(),
                    "love-avatar",
                    String::new(),
                )
            })
        },
        html::strong().child(move || name.get()),
    ))
}

fn cover_art(source: Option<String>, placeholder: &'static str) -> AnyView {
    match source {
        Some(source) => {
            View::new(html::img().class("love-tab-icon").src(source).alt("")).into_any()
        }
        None => View::new(
            html::span()
                .class("love-tab-icon love-tab-fallback")
                .aria_hidden("true")
                .child(placeholder),
        )
        .into_any(),
    }
}

#[component]
pub(super) fn Letter() -> impl IntoView {
    let data = load(Signal::derive(|| "/api/v1/letter".into()));
    let open = RwSignal::new(false);
    let front = NodeRef::<html::Button>::new();
    let close = NodeRef::<html::Button>::new();
    View::new(Chrome(
        component_props_builder(&Chrome)
            .plain(true)
            .title(Signal::derive(|| t("信笺", "Letter")))
            .children(ToChildren::to_children(move || {
                html::div()
                    .child((
                        html::h1().child(t("信笺", "Letter")),
                        html::div()
                            .child("✿　❀　✿　❀　✿")
                            .class("letter-sakura")
                            .aria_hidden("true"),
                        data.public_feedback(Some(t("信笺加载失败", "Unable to load the letter"))),
                        move || {
                            data.current().map(|letter| {
                                if letter.is_null() {
                                    View::new(
                                        html::section()
                                            .child(html::h2().child(t(
                                                "暂未公开信笺",
                                                "No letter is published yet",
                                            )))
                                            .class("public-empty-state"),
                                    )
                                    .into_any()
                                } else {
                                    View::new(
    html::div()
        .child(
            (
                    html::div()
                    .child((html::span()
                    .child( format!("{}{}",
                    t("致 ", "To "), text(& letter, "recipient"))),
                    html::button()
                    .child( t("打开信封",
                    "Open letter")).node_ref(front).r#type("button")
                    .on(ev::click, move | _ | { open
                    .set(true); blog_interface::timers::callback::Timeout::new(0, move || { if
                    let Some(close) = close.get() && let Err(error) = close.focus() { web_sys::console::warn_2(&"Unable to move browser focus".into(), &error); } })
                    .forget(); }),)).class("letter-envelope-front")
                    .aria_hidden(move
                    | | open.get().to_string())
                    .inert(move
                    | | open.get()),
                    html::div()
                    .child((html::h2()
                    .child( format!("{}{}",
                    t("致 ", "To "), text(& letter, "recipient"))),
                    html::p()
                    .child( text(& letter,
                    "content")).class("letter-content"),
                    html::strong()
                    .child( text(& letter,
                    "sender")),
                    html::button()
                    .child( t("合上信封",
                    "Close letter")).node_ref(close).r#type("button")
                    .on(ev::click, move | _ | { open
                    .set(false); blog_interface::timers::callback::Timeout::new(0, move || { if
                    let Some(front) = front.get() && let Err(error) = front.focus() { web_sys::console::warn_2(&"Unable to move browser focus".into(), &error); } })
                    .forget(); }),)).class("letter-paper")
                    .aria_hidden(move
                    | | (! open.get()).to_string())
                    .inert(move
                    | | ! open.get())
                ,),
        )
        .class(
            move || if open.get() { "letter-envelope opened" } else { "letter-envelope" },
        ),
) .into_any()
                                }
                            })
                        },
                    ))
                    .class("letter-page")
            }))
            .build(),
    ))
}

fn browser_now() -> Option<chrono::DateTime<chrono::Utc>> {
    String::from(js_sys::Date::new_0().to_iso_string())
        .parse()
        .ok()
}

fn countdown_label(days: i64) -> String {
    match days.cmp(&0) {
        std::cmp::Ordering::Less => format!(
            "{} {} {}",
            t("已过去", ""),
            days.unsigned_abs(),
            t("天", "days ago")
        ),
        std::cmp::Ordering::Equal => t("就是今天", "Today is the day"),
        std::cmp::Ordering::Greater => {
            format!("{} {days} {}", t("还有", ""), t("天", "days to go"))
        }
    }
}

fn love_clock(elapsed: Option<crate::dates::Together>, has_start: bool) -> AnyView {
    let Some(e) = elapsed else {
        return html::div()
            .class("love-timer-empty")
            .child((
                html::strong().child(if has_start {
                    t("新的旅程即将开始", "A new journey begins soon")
                } else {
                    t("从第一份回忆开始", "Start with your first memory")
                }),
                html::p().child(t(
                    "留下相识的日期，让时间替我们记住每一个日子。",
                    "Add the day you met and keep track of each day together.",
                )),
            ))
            .into_any();
    };
    let units = [
        ("years", i64::from(e.years), "年", "year", "years"),
        ("months", i64::from(e.months), "月", "month", "months"),
        ("days", e.days, "日", "day", "days"),
        ("hours", e.hours, "时", "hour", "hours"),
        ("minutes", e.minutes, "分", "minute", "minutes"),
        ("seconds", e.seconds, "秒", "second", "seconds"),
    ];
    html::div()
        .class("love-clock")
        .aria_label(t("相伴时长", "Time spent together"))
        .child((
            html::span()
                .class("love-clock-prefix")
                .child(t("第", "Together for")),
            units.map(|(key, value, zh, unit, units)| {
                html::div().attr("data-unit", key).child((
                    html::strong().child(value),
                    html::span().child(t(zh, if value == 1 { unit } else { units })),
                ))
            }),
        ))
        .into_any()
}

#[component]
pub(super) fn Love() -> impl IntoView {
    let site = site_data();
    let family = load(Signal::derive(|| "/api/v1/family".into()));
    let current = Signal::derive(move || {
        list(family.current())
            .into_iter()
            .next()
            .unwrap_or(Value::Null)
    });
    let tab = RwSignal::new("photos".to_string());
    let now = RwSignal::new(browser_now());
    let interval = SendWrapper::new(blog_interface::timers::callback::Interval::new(
        1000,
        move || now.set(browser_now()),
    ));
    on_cleanup(move || drop(interval));
    let content = NodeRef::<html::Div>::new();
    let his = Signal::derive(move || {
        let n = text(&current.get(), "man_name");
        if n.is_empty() { t("他", "Him") } else { n }
    });
    let her = Signal::derive(move || {
        let n = text(&current.get(), "woman_name");
        if n.is_empty() { t("她", "Her") } else { n }
    });
    let started = Memo::new(move |_| crate::dates::parse(&text(&current.get(), "timing")));
    let elapsed = Signal::derive(move || crate::dates::together(started.get()?, now.get()?));
    let notes_page = RwSignal::new(1u64);
    let notes = load(Signal::derive(move || {
        if tab.get() == "notes" {
            format!("/api/v1/notes/page?page={}&size=10", notes_page.get())
        } else {
            String::new()
        }
    }));
    let lightbox = RwSignal::new(None::<String>);
    View::new(Chrome(component_props_builder(&Chrome)
                .plain( true)
                .title(

                    Signal::derive(|| t("恋爱笔记", "Love journal")),
                )
                .children(ToChildren::to_children(move || (
                            Petals(),
                            html::div()
                            .child((html::section()
                            .child((html::div()
                            .child( html::div()
                            .child((html::div()
                            .child((portrait(his, current, "man_cover"),
                            svg::svg()
                            .class("love-heart").attr("viewBox", "0 0 24 24").attr("aria-hidden", "true")
                            .child(svg::path().attr("d", "M12 21 3.6 12.6A5.5 5.5 0 0 1 11.4 4.8L12 5.4l.6-.6a5.5 5.5 0 0 1 7.8 7.8Z")),
                            portrait(her, current, "woman_cover"),)).class("love-pair"),
                            html::p()
                            .child( move | | if crate ::dates::parse(& text(& current.get(),
                            "timing")).is_some() { format!("{}{}", t("故事开始于 ",
                            "Our story began on "), crate ::dates::format(& text(&
                            current.get(), "timing"))) } else {
                            t("等待第一份属于我们的回忆",
                            "A place for our first memory together") }),))
                            .class("love-pair-card")).class("love-hero-inner"),
                            svg::svg()
                            .child((svg::path()
                            .attr("opacity", ".45").attr("d",
                            "M0 35C180 100 280 0 440 45S700 95 870 40S1170 0 1440 45V110H0Z"),
                            svg::path()
                            .attr("opacity", ".7").attr("d",
                            "M0 55C150 15 280 95 450 60S730 10 920 65S1210 80 1440 35V110H0Z"),
                            svg::path()
                            .attr("d",
                            "M0 70C180 100 340 30 530 75S850 90 1060 55S1300 80 1440 65V110H0Z"),))
                            .class("love-wave").attr("viewBox", "0 0 1440 110")
                            .attr("preserveAspectRatio", "none").attr("aria-hidden",
                            "true"),)).style(move | |
                            background_style(safe_url(text(&current.get(), "bg_cover").trim(), true)))
                            .class("love-hero"),
                            html::div()
                            .child((family.public_feedback(Some(t("故事加载失败",
                            "Unable to load our story"))),
                            html::section()
                            .child((html::h1()
                            .child( t("这是我们一起走过的", "Our journey together")),
                            move || love_clock(elapsed.get(), started.get().is_some()),
                            move || {
                                let family = current.get();
                                let target = crate::dates::parse(&text(&family, "countdown_time"))?;
                                let days = (target.date_naive() - now.get()?.date_naive()).num_days();
                                Some(html::p().class("love-countdown").child((
                                    html::span().child((text(&family, "countdown_title"), "：")),
                                    html::strong().child(countdown_label(days)),
                                )))
                            },))
                            .class("love-timer"),
                            html::a()
                            .child( html::span()
                            .child( t("飞车传信", "A letter for you")))
                            .class("love-letter")
                            .style(move || background_style(site_media_url(site, |site| site.media.love_letter_background.as_deref())))
                            .href(href("/letter")),
                            html::details()
                            .child((html::summary()
                            .child( t("我们的故事", "Our story")),
                            html::p()
                            .child( move | | { let story = text(& current.get(), "family_info");
                            if story.is_empty() {
                            t("把平常的日子写下来，让照片和文字慢慢填满这个家。",
                            "Make a home for everyday moments, one photo and one story at a time.")
                            } else { story } }),
                            html::a()
                            .child((t("看看日常记录", "Explore our journal"),
                            html::span()
                            .child( "↗")
                            .aria_hidden("true"),))
                            .href(href("/jotting")),))
                            .class("love-story-card"),
                            html::div()
                            .child((html::nav()
                            .child( move || [("notes",
                            t("点点滴滴", "Moments"), site_media_url(site, |site| site.media.love_moments_cover.as_deref()), "☀",
                            t("☀️ 今朝有酒今朝醉",
                            "☀️ Little moments of joy")), ("photos",
                            t("时光相册", "Photos"), site_media_url(site, |site| site.media.love_photos_cover.as_deref()), "▧",
                            t("📸 记录美好瞬间",
                            "📸 Keep beautiful memories")), ("wishes", t("祝福板",
                            "Wishes"), site_media_url(site, |site| site.media.love_wishes_cover.as_deref()), "♡",
                            t("📋 写下你们的祝福", "📋 Leave your wishes"))]
                            .into_iter().map(| (id, l, icon, placeholder, desc) | View::new(
    html::button()
        .child(
            (
                    cover_art(icon, placeholder),
                    html::span()
                    .child((html::strong()
                    .child( l),
                    html::small()
                    .child( desc),))
                ,),
        )
        .class(
            move || if tab.get() == id { "active" } else { "" },
        )
        .r#type("button")
        .aria_pressed(
            move || (tab.get() == id).to_string(),
        )
        .aria_controls("love-panel")
        .on(
            ev::click,
            move |_| {
                tab.set(id.into());
                if let Some(c) = content.get() {
                    c.scroll_into_view();
                }
            },
        ),
)).collect_view()).class("love-tabs")
                            .aria_label(t("恋爱笔记内容",
                            "Love journal content")),
                            html::section()
                            .child((html::div()
                            .child( Collection(component_props_builder(& Collection)
                            .love( true)
                            .active( Signal::derive(move | |
                            tab.get() == "photos")).build()))
                            .hidden(move
                            | | tab.get() != "photos"),
                            html::div()
                            .child((html::div()
                            .child((html::div()
                            .child((html::h2()
                            .child( t("点点滴滴", "Moments")),
                            html::p()
                            .child( t("记录那些平常又特别的小事。",
                            "A place for the little things that make our days.")),)),
                            html::a()
                            .child((t("浏览更多记录", "Browse more entries"),
                            html::span()
                            .child( "↗")
                            .aria_hidden("true"),))
                            .href(href("/jotting")),))
                            .class("love-section-heading"),
                            notes.public_feedback(None),
                            move | | notes.busy.get().then(| | View::new(
    html::p()
        .child(

            t("正在加载记录…", "Loading entries…"),
        )
        .class("public-loading")
        .role("status"),
)),
                            html::div()
                            .child((note_cards(notes, lightbox, true),
                            move | | notes.with_current(| v | v.is_some_and(| v | page_items(v).is_empty())).then(| | View::new(
    html::div()
        .child(
            (
                    html::span()
                    .child( "✎").aria_hidden("true"),
                    html::h3()
                    .child( t("还没有记录。", "No entries yet.")),
                    html::p()
                    .child( t("写下此刻的想法，留住每一个值得纪念的日常。",
                    "Write a thought and keep a little piece of today."))
                ,),
        )
        .class("public-empty-state"),
)),)).class("love-moments-grid"),
                            Pager(component_props_builder(& Pager)
                            .data( notes)
                            .page( notes_page)
                            .public( true)
                            .label( t("记录分页",
                            "Entry pages")).build()),))
                            .hidden(move
                            | | tab.get() != "notes"),
                            move | | (tab.get() == "wishes").then(| | View::new(
    html::div()
        .child(

            Comments(component_props_builder(
                                    &Comments,
                                )
                                .kind(CommentKind::Love)
                                .title( t("祝福板", "Wishes"))
                                .build()),
        )
        .class("love-wishes"),
)),))
                            .class("love-panel").id("love-panel")
                            .aria_label(move
                            | | match tab.get().as_str() { "notes" => t("点点滴滴",
                            "Moments"), "wishes" => t("祝福板", "Wishes"), _ =>
                            t("时光相册", "Photos") }),)).class("love-workspace")
                            .node_ref(content),)).class("love-content"),))
                            .class("love-page"),
                            Lightbox(component_props_builder(& Lightbox)
                            .src( lightbox).build())
                        ,)))
                .build()))
}
