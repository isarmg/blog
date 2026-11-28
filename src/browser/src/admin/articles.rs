use crate::common::{
    Action, Data, Feedback, Pager, confirm, framed_table, load_typed, navigate, t,
};
use crate::models::{ArticleSummary, Page};
use blog_interface::children::ToChildren;
use blog_interface::component::component_props_builder;
use blog_interface::components::{HeaderActions, notify};
use blog_interface::prelude::*;
use blog_interface::{ev, html};

type ArticlesData = Data<Page<ArticleSummary>>;

#[component]
pub(super) fn Articles() -> impl IntoView {
    let page = RwSignal::new(1u64);
    let data = load_typed::<Page<ArticleSummary>>(Signal::derive(move || {
        format!("/api/v1/content/articles?page={}&size=15", page.get())
    }));
    let action = Action::new();
    html::section()
        .aria_label(t("文章管理", "Articles"))
        .child((
            HeaderActions(
                component_props_builder(&HeaderActions)
                    .children(ToChildren::to_children(move || {
                        let add = t("新增文章", "Add article");
                        let refresh = t("刷新", "Refresh");
                        (
                            html::button()
                                .child("+")
                                .class("blog-button")
                                .aria_label(add.clone())
                                .title(add)
                                .on(ev::click, move |_| navigate("new")),
                            html::button()
                                .child("↻")
                                .class("blog-button")
                                .aria_label(refresh.clone())
                                .title(refresh)
                                .disabled(move || data.busy.get())
                                .on(ev::click, move |_| data.reload()),
                        )
                    }))
                    .build(),
            ),
            html::p().class("blog-row").child(t(
                "管理公开文章与草稿。",
                "Manage published articles and drafts.",
            )),
            data.admin_feedback(Some(t("文章加载失败", "Unable to load articles"))),
            Feedback(
                component_props_builder(&Feedback)
                    .error(action.error)
                    .fallback(t(
                        "删除失败，请刷新后重试",
                        "Delete failed. Refresh and try again.",
                    ))
                    .build(),
            ),
            article_table(data, action),
            Pager(
                component_props_builder(&Pager)
                    .data(data)
                    .page(page)
                    .build(),
            ),
        ))
        .class("blog-content-stack blog-stack")
}

fn article_table(data: ArticlesData, action: Action) -> impl IntoView {
    move || {
        data.with_current(|response| {
            let page = response?;
            let titles = [
                t("标题", "Title"),
                t("状态", "Status"),
                t("浏览", "Views"),
                t("创建时间", "Created at"),
                t("操作", "Actions"),
            ]
            .map(|label| html::th().child(label));
            let rows = page
                .items
                .iter()
                .map(|item| article_row(item, data, action))
                .collect_view();
            Some(framed_table(t("文章列表", "Article list"), titles, rows))
        })
    }
}

fn article_row(item: &ArticleSummary, data: ArticlesData, action: Action) -> impl IntoView + use<> {
    let id = item.id;
    let (label, badge) = if item.view_status {
        (t("公开", "Public"), "badge success")
    } else {
        (t("加密", "Password protected"), "badge")
    };
    html::tr().child((
        html::td().class("blog-title-cell").child((
            html::strong().child(item.article_title.clone()),
            html::small().class("muted").child(format!("#{id}")),
        )),
        html::td().child(html::span().class(badge).child(label)),
        html::td().child(item.view_count),
        html::td().child(crate::dates::format(
            item.create_time.as_deref().unwrap_or_default(),
        )),
        html::td().child(
            html::div().class("blog-actions blog-flow").child((
                html::button()
                    .class("blog-button")
                    .child(t("编辑", "Edit"))
                    .on(ev::click, move |_| navigate(&format!("edit/{id}"))),
                html::button()
                    .class("blog-button blog-danger")
                    .child(t("删除", "Delete"))
                    .disabled(move || action.busy.get())
                    .on(ev::click, move |_| {
                        if confirm(&t("确定删除这篇文章？", "Delete this article?")) {
                            action.run(
                                format!("/api/v1/content/articles/{id}"),
                                "DELETE",
                                None,
                                move |_| {
                                    notify(t("文章已删除", "Article deleted"));
                                    data.reload();
                                },
                            );
                        }
                    }),
            )),
        ),
    ))
}
