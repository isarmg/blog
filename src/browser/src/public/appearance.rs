//! Configured public media, safe background styles and text avatars.
use crate::common::{Data, safe_url};
use crate::models::Site;
use blog_interface::html;
use blog_interface::prelude::*;

fn css_string(value: &str) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for character in value.chars() {
        match character {
            '"' | '\\' => {
                result.push('\\');
                result.push(character);
            }
            character if character.is_control() => {
                write!(&mut result, "\\{:x} ", character as u32).expect("write to a string");
            }
            character => result.push(character),
        }
    }
    result.push('"');
    result
}

pub(super) fn background_style(url: Option<String>) -> String {
    url.map_or_default(|url| {
        format!(
            "background-image:linear-gradient(#0b2c3e4d,#0b2c3e73),url({})",
            css_string(&url)
        )
    })
}

pub(super) fn site_media_url(site: Data<Site>, field: fn(&Site) -> Option<&str>) -> Option<String> {
    site.with_current(|site| {
        site.and_then(field)
            .and_then(|url| safe_url(url.trim(), false))
    })
}

pub(super) fn site_name(site: Option<&Site>) -> &str {
    site.and_then(|site| site.web_name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("blog")
}

pub(super) fn avatar(
    name: &str,
    src: &str,
    class: &'static str,
    alt: String,
) -> impl IntoView + use<> {
    let image = safe_url(src.trim(), true);
    let fallback_label = (image.is_none() && !alt.is_empty()).then(|| alt.clone());
    html::span()
        .class(class)
        .attr("role", fallback_label.as_ref().map(|_| "img"))
        .aria_label(fallback_label)
        .child(match image {
            Some(url) => html::img().src(url).alt(alt).into_any(),
            None => html::span()
                .child(name.trim_start().chars().next().unwrap_or('•'))
                .aria_hidden("true")
                .into_any(),
        })
}
