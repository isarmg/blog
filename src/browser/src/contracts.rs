use serde::Deserialize;
use serde_json::Value;

pub(crate) const MAX_PAGE: u64 = 100_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn fields(v: &Value, keys: &[&str], test: fn(&Value) -> bool) -> bool {
    v.as_object()
        .is_some_and(|object| keys.iter().all(|key| object.get(*key).is_some_and(test)))
}
fn nullable(v: &Value) -> bool {
    v.is_null() || v.is_string()
}
fn integer(v: &Value) -> bool {
    v.as_i64()
        .is_some_and(|n| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&n))
}
fn count(v: &Value) -> bool {
    v.as_i64()
        .is_some_and(|n| (0..=MAX_SAFE_INTEGER).contains(&n))
}
fn id(v: &Value) -> bool {
    v.as_i64()
        .is_some_and(|n| (1..=MAX_SAFE_INTEGER).contains(&n))
}
fn nullable_id(v: &Value) -> bool {
    v.is_null() || id(v)
}
fn flag(v: &Value) -> bool {
    v == 0 || v == 1
}
fn article_fields(v: &Value) -> bool {
    id(&v["id"])
        && v["article_title"].is_string()
        && fields(
            v,
            &[
                "sort_id",
                "label_id",
                "view_count",
                "like_count",
                "comment_count",
            ],
            count,
        )
        && fields(
            v,
            &["article_cover", "sort_name", "label_name", "create_time"],
            nullable,
        )
        && fields(v, &["recommend_status", "view_status"], flag)
}
pub fn article(v: &Value) -> bool {
    article_fields(v)
        && id(&v["user_id"])
        && v["article_content"].is_string()
        && fields(v, &["comment_status", "password_required"], flag)
        && fields(
            v,
            &["video_url", "username", "tips", "update_time"],
            nullable,
        )
}
pub fn summary(v: &Value) -> bool {
    article_fields(v) && fields(v, &["excerpt", "search_snippet"], nullable)
}
pub fn category(v: &Value) -> bool {
    id(&v["id"])
        && v["sort_name"].is_string()
        && fields(v, &["sort_description"], nullable)
        && v.get("priority").is_some_and(|n| n.is_null() || integer(n))
        && count(&v["article_count"])
}
pub fn label(v: &Value) -> bool {
    fields(v, &["id", "sort_id"], id)
        && v["label_name"].is_string()
        && fields(v, &["label_description"], nullable)
}
pub fn comment(v: &Value) -> bool {
    id(&v["id"])
        && v.get("user_id").is_some_and(nullable_id)
        && v["comment_content"].is_string()
        && fields(
            v,
            &["username", "avatar", "create_time", "parent_username"],
            nullable,
        )
        && count(&v["reply_count"])
}
fn admin_comment(v: &Value) -> bool {
    id(&v["id"])
        && count(&v["source"])
        && v.get("user_id").is_some_and(nullable_id)
        && v["comment_content"].is_string()
        && fields(v, &["comment_type", "username", "create_time"], nullable)
}
pub fn site(v: &Value) -> bool {
    // Derived Serde structs also accept sequences; this endpoint requires objects.
    v.is_object()
        && v.get("media").is_some_and(Value::is_object)
        && crate::models::Site::deserialize(v).is_ok()
}

pub fn note(v: &Value) -> bool {
    id(&v["id"])
        && v.get("user_id").is_some_and(nullable_id)
        && v["content"].is_string()
        && flag(&v["is_public"])
        && fields(v, &["username", "image_path", "create_time"], nullable)
}
pub fn wall(v: &Value) -> bool {
    id(&v["id"])
        && v.get("user_id").is_some_and(nullable_id)
        && v["message"].is_string()
        && fields(
            v,
            &["username", "avatar", "image_path", "create_time"],
            nullable,
        )
}
pub fn link(v: &Value) -> bool {
    id(&v["id"])
        && fields(
            v,
            &[
                "title",
                "classify",
                "cover",
                "url",
                "introduction",
                "link_type",
                "create_time",
            ],
            nullable,
        )
}
pub fn family(v: &Value) -> bool {
    id(&v["id"])
        && fields(
            v,
            &[
                "bg_cover",
                "man_cover",
                "woman_cover",
                "man_name",
                "woman_name",
                "timing",
                "countdown_title",
                "countdown_time",
                "family_info",
            ],
            nullable,
        )
}
pub fn news(v: &Value) -> bool {
    id(&v["id"]) && v["content"].is_string() && fields(v, &["create_time"], nullable)
}
pub fn home(v: &Value) -> bool {
    id(&v["id"])
        && v["title"].is_string()
        && ["latest", "recommended", "category"].contains(&v["kind"].as_str().unwrap_or_default())
        && integer(&v["priority"])
        && v["enabled"].is_boolean()
        && if v["kind"] == "category" {
            id(&v["sort_id"])
        } else {
            v["sort_id"].is_null()
        }
}
pub fn resource(v: &Value) -> bool {
    id(&v["id"])
        && v.get("size").is_some_and(|n| n.is_null() || count(n))
        && v.get("status").is_some_and(|n| n.is_null() || integer(n))
        && fields(
            v,
            &["original_name", "path", "resource_type", "create_time"],
            nullable,
        )
}
#[derive(Clone, Copy)]
pub(crate) enum UploadKind {
    Image,
    Video,
}

impl UploadKind {
    fn types(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Image => &[
                ("png", "image/png"),
                ("jpg", "image/jpeg"),
                ("gif", "image/gif"),
                ("webp", "image/webp"),
            ],
            Self::Video => &[("mp4", "video/mp4"), ("webm", "video/webm")],
        }
    }

    fn max_bytes(self) -> u32 {
        match self {
            Self::Image => 10 * 1024 * 1024,
            Self::Video => 50 * 1024 * 1024,
        }
    }

    pub(crate) fn accepts(self, size: f64, mime: &str) -> bool {
        size > 0.
            && size <= f64::from(self.max_bytes())
            && self.types().iter().any(|(_, allowed)| *allowed == mime)
    }

    pub(crate) fn uploaded_path(self, v: &Value) -> Option<&str> {
        let path = v["path"].as_str()?;
        let (hash, extension) = path.strip_prefix("/media/")?.split_once('.')?;
        let mime = v["mime_type"].as_str()?;
        (id(&v["id"])
            && (1..=u64::from(self.max_bytes())).contains(&v["size"].as_u64()?)
            && hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && self.types().contains(&(extension, mime)))
        .then_some(path)
    }
}

fn page(v: &Value, query: &str, max_size: u64, test: impl Fn(&Value) -> bool) -> bool {
    let (Some(items), Some(page), Some(size)) = (
        v["items"].as_array(),
        v["page"].as_u64(),
        v["size"].as_u64(),
    ) else {
        return false;
    };
    if !(1..=MAX_PAGE).contains(&page)
        || !(1..=max_size).contains(&size)
        || usize::try_from(size).is_ok_and(|size| items.len() > size)
        || !count(&v["total"])
    {
        return false;
    }

    let mut expected_page = Some(page);
    let mut expected_size = Some(size);
    for (key, value) in query.split('&').filter_map(|pair| pair.split_once('=')) {
        let expected = match key {
            "page" => expected_page.take(),
            "size" => expected_size.take(),
            _ => continue,
        };
        // Use the first occurrence of each pagination value.
        if let Some(expected) = expected
            && value.parse::<u64>() != Ok(expected)
        {
            return false;
        }
    }
    items.iter().all(test)
}
fn array(v: &Value, test: impl Fn(&Value) -> bool) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(test))
}
fn one_or_array(v: &Value, list: bool, test: impl Fn(&Value) -> bool) -> bool {
    if list { array(v, test) } else { test(v) }
}

fn publication_status(v: &Value) -> bool {
    v.get("status")
        .is_some_and(|status| status.is_null() || flag(status))
}

pub fn validate(path: &str, method: &str, v: &Value) -> bool {
    let (base, query) = path.split_once('?').unwrap_or((path, ""));
    let managed = base.contains("/content/");
    let read = method == "GET";
    let check_link = |row: &Value| link(row) && (!managed || publication_status(row));
    match base {
        _ if method == "DELETE" => v["deleted"] == true,
        _ if base.ends_with("/upload") => UploadKind::Image.uploaded_path(v).is_some(),
        _ if base.ends_with("/upload-video") => UploadKind::Video.uploaded_path(v).is_some(),
        _ if base.ends_with("/site/stats") => fields(v, &["article_count", "view_count"], count),
        _ if base.ends_with("/statistics") => {
            fields(v, &["articles", "comments", "members", "categories"], count)
        }
        _ if base.ends_with("/site") => site(v) && (!managed || v["author_name"].is_string()),
        _ if base.ends_with("/home-sections") => array(v, home),
        _ if base.ends_with("/categories") => one_or_array(v, read, category),
        _ if base.contains("/categories/") => category(v),
        _ if base.ends_with("/labels") => one_or_array(v, read, |row| {
            label(row) && (!read || managed || count(&row["article_count"]))
        }),
        _ if base.contains("/labels/") => label(v),
        _ if base.ends_with("/letter") => {
            let valid = fields(v, &["recipient", "sender", "content"], Value::is_string);
            if managed {
                valid && v["is_public"].is_boolean()
            } else {
                v.is_null() || valid
            }
        }
        _ if base.contains("/news") => {
            if managed && read && base.ends_with("/news") {
                page(v, query, 100, news)
            } else {
                one_or_array(v, read, news)
            }
        }
        _ if base.ends_with("/access") => {
            flag(&v["password_required"]) && fields(v, &["tips"], nullable)
        }
        _ if base.ends_with("/articles") => {
            if read {
                page(v, query, 100, summary)
            } else {
                article(v)
            }
        }
        _ if base.contains("/articles/") => article(v),
        _ if base.ends_with("/links/classes") => {
            array(v, |v| v["classify"].is_string() && count(&v["count"]))
        }
        _ if base.ends_with("/links/page") || read && base.ends_with("/content/links") => {
            page(v, query, 100, check_link)
        }
        _ if base.contains("/links/") || base.ends_with("/links") => check_link(v),
        _ if base.ends_with("/family") => one_or_array(v, read, |row| {
            family(row) && (read && !managed || publication_status(row))
        }),
        _ if base.contains("/family/") => family(v) && publication_status(v),
        _ if base.ends_with("/notes/page") || read && base.ends_with("/content/notes") => {
            let limit = if base == "/api/v1/notes/page" {
                50
            } else {
                100
            };
            page(v, query, limit, |row| {
                note(row) && (managed || count(&row["like_count"]))
            })
        }
        _ if base.contains("/notes/") || base.ends_with("/notes") => note(v),
        _ if base.ends_with("/content/resources") => page(v, query, 100, resource),
        _ if base.ends_with("/content/comments") => page(v, query, 50, admin_comment),
        _ if base.ends_with("/content/tree-hole") => page(v, query, 100, wall),
        _ if base.ends_with("/tree-hole") => array(v, wall),
        _ if base.ends_with("/tree-hole/guest") => wall(v),
        _ if base.contains("comments") => {
            if read {
                page(v, query, 50, comment)
            } else {
                comment(v)
            }
        }
        _ => false,
    }
}
#[path = "../../../test/browser/unit/contracts/tests.rs"]
#[cfg(test)]
mod tests;
