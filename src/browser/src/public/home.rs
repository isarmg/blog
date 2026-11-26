//! Homepage aggregation and configured article sections.
use super::appearance::{avatar, site_name};
use super::articles::Card;
use super::shell::{Chrome, notices, site_data};
use crate::common::{Data, href, load, load_typed, query, safe_url, t, text};
use crate::models::{
    ArticleSummary, Category, HomeSection as HomeSectionModel, HomeSource, Label, Page, Site,
    SiteStats,
};
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::ev;
use blog_interface::html;
use blog_interface::modal::ModalSurface;
use blog_interface::prelude::*;

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

#[derive(Clone)]
struct PushNotice {
    title: String,
    cover: String,
    link: String,
}

fn home_notices(site: Data<Site>) -> (RwSignal<String>, RwSignal<Option<PushNotice>>) {
    let banner = RwSignal::new(String::new());
    let push = RwSignal::new(None);
    Effect::new(move |_| {
        let items = site.with_current(|value| {
            notices(
                value
                    .and_then(|site| site.notices.as_deref())
                    .unwrap_or_default(),
            )
        });
        let mut fields = [None; 3];
        let mut public = Vec::new();
        for item in &items {
            if let Some((index, value)) = ["推送标题：", "推送封面：", "推送链接："]
                .into_iter()
                .enumerate()
                .find_map(|(index, prefix)| item.strip_prefix(prefix).map(|value| (index, value)))
            {
                fields[index].get_or_insert(value);
            } else {
                public.push(item.as_str());
            }
        }
        banner.set(public.join(" · "));
        let [title, cover, link] = fields.map(Option::unwrap_or_default);
        if title.is_empty() || safe_url(link, false).is_none() {
            return;
        }
        let key = format!("blog-push:{link}");
        let store = window().local_storage().ok().flatten();
        if store
            .as_ref()
            .is_some_and(|store| store.get_item(&key).ok().flatten().as_deref() == Some("seen"))
        {
            return;
        }
        let notice = PushNotice {
            title: title.into(),
            cover: cover.into(),
            link: link.into(),
        };
        let timer = blog_interface::thread_bound::SendWrapper::new(
            blog_interface::timers::callback::Timeout::new(2000, move || {
                push.set(Some(notice));
                if let Some(store) = store
                    && let Err(error) = store.set_item(&key, "seen")
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
    (banner, push)
}

fn push_dialog(push: RwSignal<Option<PushNotice>>) -> impl IntoView {
    move || {
        push.get().map(|notice| {
            let close = Callback::new(move |()| push.set(None));
            let title = t("每日推荐", "Daily recommendation");
            html::div()
                .class("push-backdrop")
                .role("presentation")
                .child(ModalSurface(
                    component_props_builder(&ModalSurface)
                        .class("push-dialog")
                        .label(title.clone())
                        .close(close)
                        .dismiss_on_backdrop(true)
                        .children(ToChildren::to_children(move || {
                            (
                                html::button()
                                    .child("×")
                                    .class("push-close")
                                    .r#type("button")
                                    .attr("data-blog-initial-focus", "")
                                    .aria_label(t("关闭每日推荐", "Close daily recommendation"))
                                    .on(ev::click, move |_| close.run(())),
                                html::h2().child(title.clone()),
                                html::h3().child(notice.title.clone()),
                                safe_url(&notice.cover, false).map(|url| {
                                    html::img().src(url).alt(t("推荐封面", "Recommended cover"))
                                }),
                                html::a()
                                    .child(t("立即前往 →", "Visit now →"))
                                    .href(safe_url(&notice.link, false).map(|url| href(&url)))
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
    }
}

fn profile_stat(label: String, count: impl IntoView) -> impl IntoView {
    html::span().child((label, html::strong().child(count)))
}

fn home_profile(
    site: Data<Site>,
    stats: Data<SiteStats>,
    categories: Data<Vec<Category>>,
) -> impl IntoView {
    html::section().class("aside-profile").child((
        html::div().class("aside-avatar-wrap").child(move || {
            site.with_current(|site| {
                avatar(
                    site_name(site),
                    site.and_then(|site| site.avatar.as_deref())
                        .unwrap_or_default(),
                    "aside-avatar",
                    t("站长头像", "Site owner avatar"),
                )
            })
        }),
        html::h2().child(move || {
            site.with_current(|site| {
                site.and_then(|site| site.web_name.as_deref())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("blog")
                    .to_owned()
            })
        }),
        html::div().class("aside-stats").child((
            profile_stat(t("📖 文章", "📖 Articles"), move || {
                stats.with_current(|stats| stats.map_or(0, |stats| stats.article_count))
            }),
            profile_stat(t("📒 分类", "📒 Categories"), move || {
                categories.with_current(|rows| rows.map_or(0, Vec::len))
            }),
            profile_stat(t("🔥 访问量", "🔥 Views"), move || {
                stats.with_current(|stats| stats.map_or(0, |stats| stats.view_count))
            }),
        )),
    ))
}

fn home_search() -> impl IntoView {
    let language = query("lang");
    let label = t("搜索", "Search");
    html::form()
        .class("aside-search")
        .action("/search")
        .method("get")
        .child((
            (!language.is_empty())
                .then(|| html::input().r#type("hidden").name("lang").value(language)),
            html::label().child(label.clone()).r#for("home-search"),
            html::div().child((
                html::input()
                    .id("home-search")
                    .name("q")
                    .maxlength("200")
                    .placeholder(t("搜索文章", "Search articles")),
                html::button().child("⌕").r#type("submit").aria_label(label),
            )),
        ))
}

fn category_links(categories: Data<Vec<Category>>) -> impl IntoView {
    const GRADIENTS: [&str; 4] = [
        "linear-gradient(110deg,#358bff,#15c6ff)",
        "linear-gradient(110deg,#18c9a7,#1eebeb)",
        "linear-gradient(110deg,#ff6655,#ffbf37)",
        "linear-gradient(110deg,#8c72e9,#d3a4f4)",
    ];
    html::section()
        .class("aside-categories")
        .aria_label(t("分类速览", "Category overview"))
        .child(move || {
            categories.with_current(|value| {
                value
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(i, category)| {
                        html::a()
                            .href(href(&format!("/sort?sort_id={}", category.id)))
                            .style(format!("background:{}", GRADIENTS[i % GRADIENTS.len()]))
                            .child((
                                html::small().child(t("速览", "Overview")),
                                html::strong().child(category.sort_name.clone()),
                                html::span()
                                    .child(category.sort_description.clone().unwrap_or_default()),
                            ))
                    })
                    .collect_view()
            })
        })
}

fn home_aside(
    site: Data<Site>,
    stats: Data<SiteStats>,
    categories: Data<Vec<Category>>,
    recommended: Data<Page<ArticleSummary>>,
    labels: Data<Vec<Label>>,
    messages: Data,
) -> impl IntoView {
    html::aside()
        .class("home-aside")
        .aria_label(t("网站侧栏", "Site sidebar"))
        .child((
            home_profile(site, stats, categories),
            home_search(),
            recommendations(recommended),
            html::section().class("aside-labels").child((
                html::h2().child(t("🏷 标签", "🏷 Tags")),
                html::div().child(label_links(labels)),
            )),
            html::section().class("aside-latest").child((
                html::h2().child(t("💬 最新弹幕", "💬 Recent messages")),
                html::div().child(move || {
                    messages.with_current(|value| {
                        value
                            .into_iter()
                            .flat_map(|value| {
                                value.as_array().expect(
                                    "invariant: latest messages passed the list response contract",
                                )
                            })
                            .take(RECENT_MESSAGES)
                            .map(|message| html::p().child(text(message, "message")))
                            .collect_view()
                    })
                }),
            )),
            category_links(categories),
        ))
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
    let (banner, push) = home_notices(site);
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
                    html::div().class("home-layout").child((
                        home_aside(site, stats, categories, recommended, labels, messages),
                        html::div().class("home-content").child((
                            html::div().class("announcement").child((
                                html::span().child("🔊").aria_hidden("true"),
                                html::p().child(move || {
                                    let summary = banner.get();
                                    if summary.is_empty() {
                                        t("欢迎光临", "Welcome")
                                    } else {
                                        summary
                                    }
                                }),
                            )),
                            For(component_props_builder(&For)
                                .each(move || sections.current().unwrap_or_default())
                                .key(|section| section.id)
                                .children(move |section| {
                                    HomeSection(
                                        component_props_builder(&HomeSection)
                                            .section(section)
                                            .build(),
                                    )
                                })
                                .build()),
                        )),
                    )),
                    push_dialog(push),
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
