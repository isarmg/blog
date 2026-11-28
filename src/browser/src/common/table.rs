use super::t;
use blog_interface::{html, prelude::*};

pub(crate) fn framed_table(
    label: String,
    headings: impl IntoView,
    rows: impl IntoView,
) -> impl IntoView {
    html::div()
        .class("blog-table-scroll blog-frame")
        .tabindex("0")
        .role("region")
        .aria_label(t("数据表格", "Data table"))
        .child(html::table().class("blog-table").aria_label(label).child((
            html::thead().child(html::tr().child(headings)),
            html::tbody().child(rows),
        )))
}
