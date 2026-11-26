use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::error::HttpStatus;
mod media;

use crate::{ApiResult, AppError, AppState, invalid};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

const READ_SITE: &str =
    "SELECT web_name,web_title,notices,background_image,avatar,random_cover,media
                         FROM web_info ORDER BY id LIMIT 1";

#[derive(Default, Serialize, sqlx::FromRow)]
pub(super) struct SiteInfo {
    web_name: Option<String>,
    web_title: Option<String>,
    notices: Option<String>,
    background_image: Option<String>,
    avatar: Option<String>,
    random_cover: Option<String>,
    media: sqlx::types::Json<media::SiteMedia>,
}
pub(super) async fn site_info(State(state): State<AppState>) -> ApiResult<SiteInfo> {
    let site = sqlx::query_as(READ_SITE)
        .fetch_optional(&state.pool)
        .await?
        .unwrap_or_else(|| SiteInfo {
            web_name: Some("blog".into()),
            web_title: Some("相信记录的力量".into()),
            ..SiteInfo::default()
        });
    Ok(Json(site))
}

#[derive(Serialize)]
pub(super) struct AdminSite {
    author_name: String,
    #[serde(flatten)]
    site: SiteInfo,
}
pub(super) async fn admin_site(State(state): State<AppState>) -> ApiResult<AdminSite> {
    let mut transaction = state.pool.begin().await?;
    let author = crate::site_author_id(&mut *transaction).await?;
    let author_name: Option<String> = sqlx::query_scalar("SELECT username FROM user WHERE id=?")
        .bind(author)
        .fetch_one(&mut *transaction)
        .await?;
    let site = sqlx::query_as(READ_SITE)
        .fetch_optional(&mut *transaction)
        .await?
        .unwrap_or_default();
    transaction.commit().await?;
    Ok(Json(AdminSite {
        author_name: author_name.unwrap_or_default(),
        site,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SiteInput {
    author_name: String,
    web_name: String,
    web_title: Option<String>,
    notices: Option<String>,
    background_image: Option<String>,
    avatar: Option<String>,
    random_cover: Option<String>,
    media: media::SiteMedia,
}
pub(super) async fn update_site(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    crate::ContractJson(mut input): crate::ContractJson<SiteInput>,
) -> ApiResult<AdminSite> {
    input.media.normalize()?;
    media::normalize_url(&mut input.background_image)?;
    media::normalize_url(&mut input.avatar)?;
    media::normalize_covers(&mut input.random_cover)?;
    let author_name = input.author_name.trim();
    if author_name.is_empty()
        || author_name.chars().count() > 100
        || input.web_name.trim().is_empty()
        || input.web_name.chars().count() > 100
        || input
            .web_title
            .as_deref()
            .is_some_and(|v| v.chars().count() > 200)
        || [&input.notices, &input.random_cover]
            .iter()
            .any(|item| item.as_deref().is_some_and(|v| v.len() > 16384))
    {
        return Err(invalid("站点信息无效"));
    }
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let author = crate::site_author_id(&mut *transaction).await?;
    let author_updated = sqlx::query(
        "UPDATE user SET username=?1,update_time=CURRENT_TIMESTAMP WHERE id=?2
         AND NOT EXISTS (SELECT 1 FROM user WHERE username=?1 AND id<>?2)",
    )
    .bind(author_name)
    .bind(author)
    .execute(&mut *transaction)
    .await?;
    if author_updated.rows_affected() == 0 {
        return Err(AppError::Public(HttpStatus::Conflict, "作者展示名已被使用"));
    }
    // A NULL rowid creates the first site row; otherwise update the same row read by READ_SITE.
    let site = sqlx::query_as(
        "INSERT INTO web_info(id,web_name,web_title,notices,background_image,avatar,random_cover,media,status)
         VALUES((SELECT id FROM web_info ORDER BY id LIMIT 1),?,?,?,?,?,?,?,1)
         ON CONFLICT(id) DO UPDATE SET web_name=excluded.web_name,web_title=excluded.web_title,
         notices=excluded.notices,background_image=excluded.background_image,avatar=excluded.avatar,
         random_cover=excluded.random_cover,media=excluded.media
         RETURNING web_name,web_title,notices,background_image,avatar,random_cover,media",
    )
    .bind(input.web_name)
    .bind(input.web_title)
    .bind(input.notices)
    .bind(input.background_image)
    .bind(input.avatar)
    .bind(input.random_cover)
    .bind(sqlx::types::Json(input.media))
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(AdminSite {
        author_name: author_name.to_owned(),
        site,
    }))
}
