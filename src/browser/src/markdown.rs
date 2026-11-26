//! Markdown is parsed as structured `CommonMark` events, never as trusted HTML.
//! Raw HTML events become escaped text; URL destinations are admitted before
//! the upstream HTML renderer escapes text and attributes. No sanitizer accepts
//! arbitrary HTML here. A dedicated video fence emits only validated players.
use pulldown_cmark::autolink::{LinkFinder, LinkKind};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, html};

#[path = "markdown/video.rs"]
mod video;
pub(crate) use video::{video_block, video_html};

#[cfg(target_arch = "wasm32")]
fn scheme(value: &str) -> Option<String> {
    web_sys::Url::new_with_base(value, "https://markdown.invalid/")
        .ok()
        .map(|url| url.protocol().trim_end_matches(':').to_owned())
}
#[cfg(not(target_arch = "wasm32"))]
fn scheme(value: &str) -> Option<String> {
    url_encoding::address::Url::options()
        .base_url(Some(
            &url_encoding::address::Url::parse("https://markdown.invalid/")
                .expect("invariant: Markdown base URL is valid"),
        ))
        .parse(value)
        .ok()
        .map(|url| url.scheme().to_owned())
}
fn allowed_destination(value: &str, image: bool) -> bool {
    scheme(value).is_some_and(|scheme| {
        if image {
            ["http", "https"].contains(&scheme.as_str())
        } else {
            ["http", "https", "mailto", "ftp", "ftps", "tel"].contains(&scheme.as_str())
        }
    })
}

fn has_authority_scheme(value: &str) -> bool {
    // LinkFinder's explicit URL scanner recognizes `scheme://authority`.
    // A colon in a bare host's port, path or query is not a scheme delimiter.
    value.split_once("://").is_some_and(|(scheme, _)| {
        scheme
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
    })
}

fn append_video<'a>(events: &mut Vec<Event<'a>>, kind: CodeBlockKind<'a>, parser: &mut Parser<'a>) {
    let mut source = String::new();
    for event in parser {
        if event == Event::End(TagEnd::CodeBlock) {
            break;
        }
        let Event::Text(text) = event else {
            unreachable!("invariant: fenced code consists of text followed by a closing event")
        };
        source.push_str(&text);
    }
    // Only the renderer emits HTML. Invalid video declarations remain code.
    if let Some(player) = video_html(&source, None) {
        events.push(Event::Html(player.into()));
    } else {
        events.extend([
            Event::Start(Tag::CodeBlock(kind)),
            Event::Text(source.into()),
            Event::End(TagEnd::CodeBlock),
        ]);
    }
}

fn append_links<'a>(events: &mut Vec<Event<'a>>, finder: &LinkFinder, text: CowStr<'a>) {
    let mut start = 0;
    for link in finder.links(&text) {
        let destination = match link.kind() {
            LinkKind::Email => format!("mailto:{}", link.as_str()),
            LinkKind::Url if !has_authority_scheme(link.as_str()) => {
                format!("http://{}", link.as_str())
            }
            LinkKind::Url => link.as_str().to_owned(),
        };
        if !allowed_destination(&destination, false) {
            continue;
        }
        if start < link.start() {
            events.push(Event::Text(text[start..link.start()].to_owned().into()));
        }
        events.extend([
            Event::Start(Tag::Link {
                link_type: LinkType::Inline,
                dest_url: destination.into(),
                title: CowStr::Borrowed(""),
                id: CowStr::Borrowed(""),
            }),
            Event::Text(link.as_str().to_owned().into()),
            Event::End(TagEnd::Link),
        ]);
        start = link.end();
    }
    if start == 0 {
        events.push(Event::Text(text));
    } else if start < text.len() {
        events.push(Event::Text(text[start..].to_owned().into()));
    }
}

pub fn html(content: &str) -> String {
    let mut parser = Parser::new_ext(
        content,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    );
    let mut finder = LinkFinder::new();
    finder.url_must_have_scheme(false);
    let mut events = Vec::new();
    let mut links = Vec::new();
    let mut images = Vec::new();
    let mut code = false;
    while let Some(event) = parser.next() {
        match event {
            Event::Html(text) | Event::InlineHtml(text) => events.push(Event::Text(text)),
            Event::SoftBreak => events.push(Event::HardBreak),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.trim() == "video" => {
                append_video(&mut events, CodeBlockKind::Fenced(info), &mut parser);
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                code = true;
                events.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                code = false;
                events.push(Event::End(TagEnd::CodeBlock));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let allowed = if link_type == LinkType::Email {
                    allowed_destination(&format!("mailto:{dest_url}"), false)
                } else {
                    allowed_destination(&dest_url, false)
                };
                links.push(allowed);
                if allowed {
                    events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let allowed = allowed_destination(&dest_url, true);
                images.push(allowed);
                if allowed {
                    events.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                }
            }
            Event::End(tag @ (TagEnd::Link | TagEnd::Image)) => {
                let stack = if tag == TagEnd::Link {
                    &mut links
                } else {
                    &mut images
                };
                if stack
                    .pop()
                    .expect("invariant: parser link and image events are balanced")
                {
                    events.push(Event::End(tag));
                }
            }
            Event::Text(text) if !code && links.is_empty() && images.is_empty() => {
                append_links(&mut events, &finder, text);
            }
            event @ (Event::Start(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
            | Event::HardBreak
            | Event::Rule
            | Event::TaskListMarker(_)) => events.push(event),
        }
    }
    let mut output = String::with_capacity(content.len());
    html::push_html(&mut output, events.into_iter());
    output
}

#[path = "../../../test/browser/unit/markdown/tests.rs"]
#[cfg(test)]
mod tests;
