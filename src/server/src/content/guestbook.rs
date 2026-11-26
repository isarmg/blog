use super::{PageQuery, PageResult};
use crate::{ApiResult, AppState, invalid};
use axum::{Json, extract::State};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::server_cli::{ContractPath as Path, ContractQuery as Query};
use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct TreeHole {
    id: i64,
    user_id: Option<i64>,
    username: Option<String>,
    avatar: Option<String>,
    message: String,
    image_path: Option<String>,
    create_time: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TreeHoleQuery {
    size: Option<i64>,
}

pub(super) async fn tree_hole(
    State(state): State<AppState>,
    Query(query): Query<TreeHoleQuery>,
) -> ApiResult<Vec<TreeHole>> {
    const MAX_SIZE: i64 = 100;
    Ok(Json(
        sqlx::query_as::<_, TreeHole>(
            "SELECT t.id,t.user_id,u.username,t.avatar,t.message,t.image_path,t.create_time FROM tree_hole t LEFT JOIN user u ON u.id=t.user_id ORDER BY t.id DESC LIMIT ?",
        )
        .bind(query.size.unwrap_or(MAX_SIZE).clamp(1, MAX_SIZE))
        .fetch_all(&state.pool)
        .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TreeHoleInput {
    message: String,
    image_path: Option<String>,
}
pub(super) async fn create_guest_tree_hole(
    State(state): State<AppState>,
    crate::ContractJson(input): crate::ContractJson<TreeHoleInput>,
) -> ApiResult<TreeHole> {
    let message = input.message.trim();
    if message.is_empty() || message.chars().count() > 60 || input.image_path.is_some() {
        return Err(invalid("留言不能为空、不能超过 60 个字，也不能包含图片"));
    }
    let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let entry = sqlx::query_as(
        "INSERT INTO tree_hole(message) VALUES(?)
         RETURNING id,user_id,NULL AS username,avatar,message,image_path,create_time",
    )
    .bind(message)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(entry))
}

pub(super) async fn content_tree_hole(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<TreeHole>> {
    let pagination = query.admin_bounds();
    let mut transaction = state.pool.begin().await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tree_hole")
        .fetch_one(&mut *transaction)
        .await?;
    let items = sqlx::query_as::<_, TreeHole>(
        "SELECT t.id,t.user_id,u.username,t.avatar,t.message,t.image_path,t.create_time FROM tree_hole t LEFT JOIN user u ON u.id=t.user_id ORDER BY t.id DESC LIMIT ? OFFSET ?",
    )
    .bind(pagination.size)
    .bind(pagination.offset())
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}
pub(super) async fn delete_tree_hole(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let changed = sqlx::query("DELETE FROM tree_hole WHERE id=?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    super::finish_delete(transaction, changed).await
}
