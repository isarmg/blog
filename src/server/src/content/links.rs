use super::{PageQuery, PageResult};
use crate::{ApiResult, AppError, AppState, absent, invalid};
use axum::{Json, extract::State};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::server_cli::{ContractPath as Path, ContractQuery as Query};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum LinkKind {
    Favorites,
    FriendUrl,
    LovePhoto,
    Funny,
}

impl LinkKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Favorites => "favorites",
            Self::FriendUrl => "friendUrl",
            Self::LovePhoto => "lovePhoto",
            Self::Funny => "funny",
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct PublicLink {
    id: i64,
    title: Option<String>,
    classify: Option<String>,
    cover: Option<String>,
    url: Option<String>,
    introduction: Option<String>,
    link_type: Option<String>,
    create_time: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkQuery {
    kind: Option<LinkKind>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkPageQuery {
    kind: LinkKind,
    classify: Option<String>,
    page: Option<i64>,
    size: Option<i64>,
}
pub(super) async fn link_page(
    State(state): State<AppState>,
    Query(query): Query<LinkPageQuery>,
) -> ApiResult<PageResult<PublicLink>> {
    if query.classify.as_ref().is_some_and(|s| s.len() > 100) {
        return Err(invalid("资源筛选无效"));
    }
    let pagination = PageQuery {
        page: query.page,
        size: query.size,
    }
    .bounds(12, 100);
    let mut transaction = state.pool.begin().await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resource_path WHERE type=? AND status=1 AND (? IS NULL OR classify=?)")
        .bind(query.kind.as_str()).bind(&query.classify).bind(&query.classify)
        .fetch_one(&mut *transaction).await?;
    let items=sqlx::query_as::<_,PublicLink>("SELECT id,title,classify,cover,url,introduction,type AS link_type,create_time FROM resource_path WHERE type=? AND status=1 AND (? IS NULL OR classify=?) ORDER BY id DESC LIMIT ? OFFSET ?")
        .bind(query.kind.as_str()).bind(&query.classify).bind(&query.classify).bind(pagination.size).bind(pagination.offset())
        .fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}
#[derive(Serialize, sqlx::FromRow)]
pub(super) struct LinkClass {
    classify: String,
    count: i64,
}
pub(super) async fn link_classes(
    State(state): State<AppState>,
    Query(query): Query<LinkQuery>,
) -> ApiResult<Vec<LinkClass>> {
    let kind = query.kind.unwrap_or(LinkKind::LovePhoto);
    Ok(Json(sqlx::query_as::<_,LinkClass>("SELECT classify,COUNT(*) AS count FROM resource_path WHERE type=? AND status=1 AND classify IS NOT NULL AND classify<>'' GROUP BY classify ORDER BY MAX(id) DESC")
        .bind(kind.as_str()).fetch_all(&state.pool).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkStatusInput {
    active: bool,
}
#[derive(Serialize, sqlx::FromRow)]
pub(super) struct AdminLink {
    id: i64,
    title: Option<String>,
    classify: Option<String>,
    cover: Option<String>,
    url: Option<String>,
    introduction: Option<String>,
    link_type: Option<String>,
    status: Option<i64>,
    create_time: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdminLinkQuery {
    kind: Option<LinkKind>,
    status: Option<i64>,
    page: Option<i64>,
    size: Option<i64>,
}
pub(super) async fn content_links(
    State(state): State<AppState>,
    Query(query): Query<AdminLinkQuery>,
) -> ApiResult<PageResult<AdminLink>> {
    let pagination = PageQuery {
        page: query.page,
        size: query.size,
    }
    .admin_bounds();
    let kind = query.kind.map(LinkKind::as_str);
    if query.status.is_some_and(|status| !matches!(status, 0 | 1)) {
        return Err(invalid("资源状态无效"));
    }
    let mut transaction = state.pool.begin().await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM resource_path WHERE (? IS NULL OR type=?) AND (? IS NULL OR status=?)",
    )
    .bind(kind)
    .bind(kind)
    .bind(query.status)
    .bind(query.status)
    .fetch_one(&mut *transaction)
    .await?;
    let items = sqlx::query_as::<_, AdminLink>("SELECT id,title,classify,cover,url,introduction,type AS link_type,status,create_time FROM resource_path WHERE (? IS NULL OR type=?) AND (? IS NULL OR status=?) ORDER BY id DESC LIMIT ? OFFSET ?")
        .bind(kind).bind(kind).bind(query.status).bind(query.status).bind(pagination.size).bind(pagination.offset()).fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkInput {
    title: String,
    classify: Option<String>,
    cover: Option<String>,
    url: Option<String>,
    introduction: Option<String>,
    link_type: LinkKind,
    status: bool,
}
fn validate_link(input: &LinkInput) -> Result<(), AppError> {
    if input.title.trim().is_empty()
        || input.title.chars().count() > 64
        || input
            .classify
            .as_deref()
            .is_some_and(|v| v.chars().count() > 32)
        || input
            .introduction
            .as_deref()
            .is_some_and(|v| v.chars().count() > 1024)
        || input.cover.as_deref().is_some_and(|v| {
            v.len() > 2048
                || !(v.starts_with("https://") || v.starts_with("http://") || v.starts_with('/'))
        })
        || input.url.as_deref().is_some_and(|v| {
            v.len() > 2048 || !(v.starts_with("https://") || v.starts_with("http://"))
        })
    {
        return Err(invalid("资源内容无效"));
    }
    Ok(())
}
pub(super) async fn create_link(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    crate::ContractJson(input): crate::ContractJson<LinkInput>,
) -> ApiResult<AdminLink> {
    validate_link(&input)?;
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let link = sqlx::query_as(
        "INSERT INTO resource_path(title,classify,cover,url,introduction,type,status) VALUES(?,?,?,?,?,?,?)
         RETURNING id,title,classify,cover,url,introduction,type AS link_type,status,create_time",
    )
    .bind(input.title)
    .bind(input.classify)
    .bind(input.cover)
    .bind(input.url)
    .bind(input.introduction)
    .bind(input.link_type.as_str())
    .bind(i64::from(input.status))
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(link))
}
pub(super) async fn update_link(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
    crate::ContractJson(input): crate::ContractJson<LinkInput>,
) -> ApiResult<AdminLink> {
    validate_link(&input)?;
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let link = sqlx::query_as(
        "UPDATE resource_path SET title=?,classify=?,cover=?,url=?,introduction=?,type=?,status=? WHERE id=?
         RETURNING id,title,classify,cover,url,introduction,type AS link_type,status,create_time",
    )
    .bind(input.title)
    .bind(input.classify)
    .bind(input.cover)
    .bind(input.url)
    .bind(input.introduction)
    .bind(input.link_type.as_str())
    .bind(i64::from(input.status))
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(absent)?;
    transaction.commit().await?;
    Ok(Json(link))
}
pub(super) async fn change_link_status(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
    crate::ContractJson(input): crate::ContractJson<LinkStatusInput>,
) -> ApiResult<AdminLink> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let link = sqlx::query_as(
        "UPDATE resource_path SET status=? WHERE id=?
         RETURNING id,title,classify,cover,url,introduction,type AS link_type,status,create_time",
    )
    .bind(i64::from(input.active))
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(absent)?;
    transaction.commit().await?;
    Ok(Json(link))
}
pub(super) async fn delete_link(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let changed = sqlx::query("DELETE FROM resource_path WHERE id=?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    super::finish_delete(transaction, changed).await
}
