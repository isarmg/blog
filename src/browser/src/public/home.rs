//! Homepage aggregation and configured article sections.
use super::appearance::{avatar, site_name};
use super::articles::Card;
use super::shell::{Chrome, notices, site_data};
use crate::common::{Data, href, list, load, load_typed, query, safe_url, t, text};
use crate::models::{
    ArticleSummary, Category, HomeSection as HomeSectionModel, HomeSource, Label, Page, SiteStats,
};
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::ev;
use blog_interface::html;
use blog_interface::modal::ModalSurface;
use blog_interface::prelude::*;
use serde_json::{Value, json};

const RECENT_MESSAGES: usize = 9;

fn recommendations(data: Data<Page<ArticleSummary>>) -> impl IntoView {
    move || {
        data.with_current(|value| {
            let page = value.filter(|page| !page.items.is_empty())?;
            let featured = page.items.iter().take(2).map(|article| {
                recommendation(article, html::strong().child(article.article_title.clone()))
            });
            let all = page.items.iter().map(|article| {
                let stamp =
                    crate::dates::format(article.create_time.as_deref().unwrap_or_default());
                recommendation(
                    article,
                    html::span().child((
                        article.article_title.clone(),
                        html::small().child(format!("◷ {stamp}")),
                    )),
                )
            });
            Some((
                html::section()
                    .child((
                        html::h2().child(t("✧ 推荐位", "✧ Featured")),
                        featured.collect_view(),
                    ))
                    .class("aside-featured"),
                html::section()
                    .child((
                        html::h2().child(t("🔥 推荐文章", "🔥 Recommended articles")),
                        all.collect_view(),
                    ))
                    .class("aside-recommended"),
            ))
        })
    }
}

fn recommendation<V: IntoView>(article: &ArticleSummary, caption: V) -> impl IntoView + use<V> {
    let cover = safe_url(article.article_cover.as_deref().unwrap_or_default(), false)
        .map(|url| html::img().src(url).alt("").loading("lazy"));
    html::a()
        .child((cover, caption))
        .href(href(&format!("/article/{}", article.id)))
}

fn label_links(data: Data<Vec<Label>>) -> impl IntoView {
    move || {
        data.with_current(|labels| {
            labels
                .into_iter()
                .flatten()
                .take(28)
                .map(|label| {
                    html::a().child(label.name.clone()).href(href(&format!(
                        "/sort?sort_id={}&label_id={}",
                        label.sort_id, label.id
                    )))
                })
                .collect_view()
        })
    }
}

#[component]
pub(super) fn Home() -> impl IntoView {
    let site = site_data();
    let stats = load_typed::<SiteStats>(Signal::derive(|| "/api/v1/site/stats".into()));
    let sections =
        load_typed::<Vec<HomeSectionModel>>(Signal::derive(|| "/api/v1/home-sections".into()));
    let categories = load_typed::<Vec<Category>>(Signal::derive(|| "/api/v1/categories".into()));
    let recommended = load_typed::<Page<ArticleSummary>>(Signal::derive(|| {
        "/api/v1/articles?size=5&recommended=true".into()
    }));
    let labels = load_typed::<Vec<Label>>(Signal::derive(|| "/api/v1/labels".into()));
    let messages = load(Signal::derive(|| {
        format!("/api/v1/tree-hole?size={RECENT_MESSAGES}")
    }));
    let push = RwSignal::new(None::<Value>);
    Effect::new(move |_| {
        let items = site
            .current()
            .map_or_default(|v| notices(&v.notices.unwrap_or_default()));
        let get = |prefix: &str| {
            items
                .iter()
                .find_map(|s| s.strip_prefix(prefix).map(str::to_string))
                .unwrap_or_default()
        };
        let title = get("推送标题：");
        let link = get("推送链接：");
        if title.is_empty() || safe_url(&link, false).is_none() {
            return;
        }
        let key = format!("blog-push:{link}");
        let store = window().local_storage().ok().flatten();
        if store
            .as_ref()
            .is_some_and(|s| s.get_item(&key).ok().flatten().as_deref() == Some("seen"))
        {
            return;
        }
        let value = json!({"title":title,"link":link,"cover":get("推送封面：")});
        let timer = blog_interface::thread_bound::SendWrapper::new(
            blog_interface::timers::callback::Timeout::new(2000, move || {
                push.set(Some(value));
                if let Some(s) = store
                    && let Err(error) = s.set_item(&key, "seen")
                {
                    web_sys::console::debug_2(
                        &"Notice receipt remains limited to this document".into(),
                        &error,
                    );
                }
            }),
        );
        on_cleanup(move || drop(timer));
    });
    View::new(Chrome(
        component_props_builder(&Chrome)
            .home(true)
            .children(ToChildren::to_children(move || {
                (
                    site.public_feedback(Some(t(
                        "网站信息加载失败",
                        "Unable to load site information",
                    ))),
                    sections.public_feedback(Some(t(
                        "首页栏目加载失败",
                        "Unable to load home sections",
                    ))),
                    html::div()
                        .child((
                            html::aside()
                                .child((
                                    html::section()
                                        .child((
                                            html::div()
                                                .child(move || {
                                                    site.with_current(|site| {
                                                        avatar(
                                                            site_name(site),
                                                            site.and_then(|site| {
                                                                site.avatar.as_deref()
                                                            })
                                                            .unwrap_or_default(),
                                                            "aside-avatar",
                                                            t("站长头像", "Site owner avatar"),
                                                        )
                                                    })
                                                })
                                                .class("aside-avatar-wrap"),
                                            html::h2().child(move || {
                                                site.current()
                                                    .map(|v| v.web_name.unwrap_or_default())
                                                    .filter(|s| !s.is_empty())
                                                    .unwrap_or("blog".into())
                                            }),
                                            html::div()
                                                .child((
                                                    html::span().child((
                                                        t("📖 文章", "📖 Articles"),
                                                        html::strong().child(move || {
                                                            stats
                                                                .current()
                                                                .map_or(0, |v| v.article_count)
                                                        }),
                                                    )),
                                                    html::span().child((
                                                        t("📒 分类", "📒 Categories"),
                                                        html::strong().child(move || {
                                                            categories
                                                                .current()
                                                                .unwrap_or_default()
                                                                .len()
                                                        }),
                                                    )),
                                                    html::span().child((
                                                        t("🔥 访问量", "🔥 Views"),
                                                        html::strong().child(move || {
                                                            stats
                                                                .current()
                                                                .map_or(0, |v| v.view_count)
                                                        }),
                                                    )),
                                                ))
                                                .class("aside-stats"),
                                        ))
                                        .class("aside-profile"),
                                    html::form()
                                        .child((
                                            (!query("lang").is_empty()).then(|| {
                                                View::new(
                                                    html::input()
                                                        .r#type("hidden")
                                                        .name("lang")
                                                        .value(query("lang")),
                                                )
                                            }),
                                            html::label()
                                                .child(t("搜索", "Search"))
                                                .r#for("home-search"),
                                            html::div().child((
                                                html::input()
                                                    .id("home-search")
                                                    .name("q")
                                                    .maxlength("200")
                                                    .placeholder(t("搜索文章", "Search articles")),
                                                html::button()
                                                    .child("⌕")
                                                    .r#type("submit")
                                                    .aria_label(t("搜索", "Search")),
                                            )),
                                        ))
                                        .class("aside-search")
                                        .action("/search")
                                        .method("get"),
                                    recommendations(recommended),
                                    html::section()
                                        .child((
                                            html::h2().child(t("🏷 标签", "🏷 Tags")),
                                            html::div().child(label_links(labels)),
                                        ))
                                        .class("aside-labels"),
                                    html::section()
                                        .child((
                                            html::h2()
                                                .child(t("💬 最新弹幕", "💬 Recent messages")),
                                            html::div().child(move || {
                                                list(messages.current())
                                                    .into_iter()
                                                    .take(RECENT_MESSAGES)
                                                    .map(|m| {
                                                        View::new(
                                                            html::p().child(text(&m, "message")),
                                                        )
                                                    })
                                                    .collect_view()
                                            }),
                                        ))
                                        .class("aside-latest"),
                                    html::section()
                                        .child(move || {
                                            categories
                                                .current()
                                                .unwrap_or_default()
                                                .into_iter()
                                                .enumerate()
                                                .map(|(i, c)| {
                                                    View::new(
    html::a()
        .child(
            (
                    html::small()
                    .child( t("速览",
                    "Overview")),
                    html::strong()
                    .child( c.sort_name.clone()),
                    html::span()
                    .child( c.sort_description
                    .clone().unwrap_or_default())
                ,),
        )
        .style(
            format!(
                "background:{}", ["linear-gradient(110deg,#358bff,#15c6ff)",
                "linear-gradient(110deg,#18c9a7,#1eebeb)",
                "linear-gradient(110deg,#ff6655,#ffbf37)",
                "linear-gradient(110deg,#8c72e9,#d3a4f4)"] [i % 4]
            ),
        )
        .href(href(&format!("/sort?sort_id={}", c.id))),
)
                                                })
                                                .collect_view()
                                        })
                                        .class("aside-categories")
                                        .aria_label(t("分类速览", "Category overview")),
                                ))
                                .class("home-aside")
                                .aria_label(t("网站侧栏", "Site sidebar")),
                            html::div()
                                .child((
                                    html::div()
                                        .child((
                                            html::span().child("🔊").aria_hidden("true"),
                                            html::p().child(move || {
                                                site.current()
                                                    .map(|v| {
                                                        notices(&v.notices.unwrap_or_default())
                                                            .into_iter()
                                                            .filter(|s| {
                                                                ![
                                                                    "推送标题：",
                                                                    "推送封面：",
                                                                    "推送链接：",
                                                                ]
                                                                .iter()
                                                                .any(|p| s.starts_with(p))
                                                            })
                                                            .collect::<Vec<_>>()
                                                            .join(" · ")
                                                    })
                                                    .filter(|s| !s.is_empty())
                                                    .unwrap_or(t("欢迎光临", "Welcome"))
                                            }),
                                        ))
                                        .class("announcement"),
                                    For(component_props_builder(&For)
                                        .each(move || sections.current().unwrap_or_default())
                                        .key(|s| s.id)
                                        .children(move |section| {
                                            View::new(HomeSection(
                                                component_props_builder(&HomeSection)
                                                    .section(section)
                                                    .build(),
                                            ))
                                        })
                                        .build()),
                                ))
                                .class("home-content"),
                        ))
                        .class("home-layout"),
                    move || {
                        push.get().map(|recommendation| {
                            let close = Callback::new(move |()| push.set(None));
                            html::div()
                                .class("push-backdrop")
                                .role("presentation")
                                .child(ModalSurface(
                                    component_props_builder(&ModalSurface)
                                        .class("push-dialog")
                                        .label(t("每日推荐", "Daily recommendation"))
                                        .close(close)
                                        .dismiss_on_backdrop(true)
                                        .children(ToChildren::to_children(move || {
                                            (
                                                html::button()
                                                    .child("×")
                                                    .class("push-close")
                                                    .r#type("button")
                                                    .attr("data-blog-initial-focus", "")
                                                    .aria_label(t(
                                                        "关闭每日推荐",
                                                        "Close daily recommendation",
                                                    ))
                                                    .on(ev::click, move |_| close.run(())),
                                                html::h2()
                                                    .child(t("每日推荐", "Daily recommendation")),
                                                html::h3().child(text(&recommendation, "title")),
                                                safe_url(&text(&recommendation, "cover"), false)
                                                    .map(|url| {
                                                        html::img()
                                                            .src(url)
                                                            .alt(t("推荐封面", "Recommended cover"))
                                                    }),
                                                html::a()
                                                    .child(t("立即前往 →", "Visit now →"))
                                                    .href(
                                                        safe_url(
                                                            &text(&recommendation, "link"),
                                                            false,
                                                        )
                                                        .map(|url| href(&url)),
                                                    )
                                                    .target("_blank")
                                                    .rel("noopener noreferrer")
                                                    .on(ev::click, move |_| close.run(())),
                                            )
                                        }))
                                        .build(),
                                ))
                                .on(ev::click, move |event| {
                                    if event.target() == event.current_target() {
                                        close.run(());
                                    }
                                })
                        })
                    },
                )
            }))
            .build(),
    ))
}

#[component]
fn HomeSection(section: HomeSectionModel) -> impl IntoView {
    let filter = match section.source {
        HomeSource::Recommended => "&recommended=true".into(),
        HomeSource::Category { sort_id } => format!("&sort_id={sort_id}"),
        HomeSource::Latest => String::new(),
    };
    let path = format!("/api/v1/articles?size=6{filter}");
    let data = load_typed::<Page<ArticleSummary>>(Signal::derive(move || path.clone()));
    let more = match section.source {
        HomeSource::Recommended => "/sort?recommended=1".into(),
        HomeSource::Category { sort_id } => format!("/sort?sort_id={sort_id}"),
        HomeSource::Latest => "/sort".into(),
    };
    View::new(
        html::section()
            .child((
                html::div()
                    .child((
                        html::h2().child((
                            html::span().class("section-squares").aria_hidden("true"),
                            section.title.clone(),
                        )),
                        html::a()
                            .child((
                                html::span()
                                    .child("❯❯")
                                    .class("more-chevrons")
                                    .aria_hidden("true"),
                                " ",
                                t("更多", "More"),
                            ))
                            .href(href(&more)),
                    ))
                    .class("section-heading"),
                data.public_feedback(None),
                html::div()
                    .child(move || {
                        data.current()
                            .map_or_default(|v| v.items)
                            .into_iter()
                            .map(|item| {
                                View::new(Card(component_props_builder(&Card).item(item).build()))
                            })
                            .collect_view()
                    })
                    .class("article-grid"),
                move || {
                    data.current().filter(|v| v.items.is_empty()).map(|_| {
                        View::new(
    html::div()
        .child(
            (
                    html::span()
                    .child( "✎").aria_hidden("true"),
                    html::h3()
                    .child( t("这个栏目还没有文章。",
                    "No articles in this section yet.")),
                    html::p()
                    .child( t("浏览其它分类，或到留言页分享你的想法。",
                    "Browse other categories, or leave a thought on the message wall.")),
                    html::div()
                    .child((html::a()
                    .child((t("浏览文章", "Browse articles"),
                    html::span()
                    .child( "↗").aria_hidden("true"),))
                    .href(href("/sort")),
                    html::a()
                    .child((t("给我留言", "Leave a message"),
                    html::span()
                    .child( "↗").aria_hidden("true"),))
                    .href(href("/message")),))
                ,),
        )
        .class("public-empty-state home-empty-section"),
)
                    })
                },
            ))
            .class("home-section")
            .aria_label(section.title),
    )
}
