use super::{
    PageQuery, PageResult,
    input::{IdempotencyKey, ShortContent},
};
use crate::{ApiResult, AppError, AppState, absent, invalid};
use axum::{Json, extract::State, http::HeaderMap};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::error::HttpStatus;
use blog_application::server_cli::{ContractPath as Path, ContractQuery as Query};
use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct Note {
    id: i64,
    user_id: Option<i64>,
    username: Option<String>,
    content: String,
    image_path: Option<String>,
    like_count: i64,
    is_public: i64,
    create_time: Option<String>,
}
pub(super) async fn note_page(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<Note>> {
    let pagination = query.bounds(10, 50);
    let mut transaction = state.pool.begin().await?;
    let author = crate::site_author_id(&mut *transaction).await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM wei_yan WHERE type='friend' AND user_id=? AND is_public=1",
    )
    .bind(author)
    .fetch_one(&mut *transaction)
    .await?;
    let items=sqlx::query_as::<_,Note>("SELECT w.id,w.user_id,u.username,w.content,w.image_path,w.like_count,w.is_public,w.create_time FROM wei_yan w LEFT JOIN user u ON u.id=w.user_id WHERE w.type='friend' AND w.user_id=? AND w.is_public=1 ORDER BY w.id DESC LIMIT ? OFFSET ?")
        .bind(author).bind(pagination.size).bind(pagination.offset())
        .fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct AdminNote {
    id: i64,
    user_id: Option<i64>,
    username: Option<String>,
    content: String,
    image_path: Option<String>,
    is_public: i64,
    create_time: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdminNoteInput {
    content: ShortContent,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    image_path: Option<String>,
    is_public: bool,
}

const ADMIN_NOTE_SQL: &str = "SELECT w.id,w.user_id,u.username,w.content,w.image_path,w.is_public,w.create_time FROM wei_yan w LEFT JOIN user u ON u.id=w.user_id";

async fn validate_note_image(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    image_path: Option<&str>,
) -> Result<(), AppError> {
    let Some(path) = image_path else {
        return Ok(());
    };
    if !crate::media::is_media_image_path(path) {
        return Err(invalid("动态图片必须选择已上传的本地图片"));
    }
    let registered: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM resource WHERE path=? AND type='image' AND store_type='local'
         AND status=1 AND mime_type IN ('image/png','image/jpeg','image/gif','image/webp'))",
    )
    .bind(path)
    .fetch_one(&mut **transaction)
    .await?;
    if !registered {
        return Err(invalid("动态图片必须选择已上传的本地图片"));
    }
    Ok(())
}

async fn load_admin_note(
    connection: &mut sqlx::SqliteConnection,
    id: i64,
) -> Result<AdminNote, AppError> {
    sqlx::query_as::<_, AdminNote>(sqlx::AssertSqlSafe(format!(
        "{ADMIN_NOTE_SQL} WHERE w.id=? AND w.type='friend'"
    )))
    .bind(id)
    .fetch_optional(connection)
    .await?
    .ok_or_else(absent)
}

pub(super) async fn create_admin_note(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    headers: HeaderMap,
    crate::ContractJson(input): crate::ContractJson<AdminNoteInput>,
) -> ApiResult<AdminNote> {
    // Bind the initial normalized draft, independently of later edits to its row.
    let request = IdempotencyKey::from_headers(&headers)?
        .map(|key| key.bind_payload(&(input.content.as_str(), &input.image_path, input.is_public)));
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    if let Some(request) = &request
        && let Some((hash, id)) = sqlx::query_as::<_, (Vec<u8>, Option<i64>)>(
            "SELECT request_hash,note_id FROM note_submission WHERE request_id=?",
        )
        .bind(request.key.as_str())
        .fetch_optional(&mut *transaction)
        .await?
    {
        if hash != request.hash {
            return Err(AppError::Public(
                HttpStatus::Conflict,
                "动态请求标识已用于不同内容",
            ));
        }
        let id = id.ok_or(AppError::Public(
            HttpStatus::Conflict,
            "原动态已删除，不会重复发表",
        ))?;
        // Recovery returns the current row even if the original image was removed.
        let note = load_admin_note(&mut transaction, id).await?;
        transaction.commit().await?;
        return Ok(Json(note));
    }
    let author = crate::site_author_id(&mut *transaction).await?;
    validate_note_image(&mut transaction, input.image_path.as_deref()).await?;
    let id = sqlx::query(
        "INSERT INTO wei_yan(user_id,content,image_path,type,is_public) VALUES(?,?,?,'friend',?)",
    )
    .bind(author)
    .bind(input.content.as_str())
    .bind(input.image_path)
    .bind(i64::from(input.is_public))
    .execute(&mut *transaction)
    .await?
    .last_insert_rowid();
    if let Some(request) = request {
        sqlx::query("INSERT INTO note_submission(request_id,request_hash,note_id) VALUES(?,?,?)")
            .bind(request.key.as_str())
            .bind(request.hash.as_slice())
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    let note = load_admin_note(&mut transaction, id).await?;
    transaction.commit().await?;
    Ok(Json(note))
}

pub(super) async fn update_admin_note(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
    crate::ContractJson(input): crate::ContractJson<AdminNoteInput>,
) -> ApiResult<AdminNote> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM wei_yan WHERE id=? AND type='friend')")
            .bind(id)
            .fetch_one(&mut *transaction)
            .await?;
    if !exists {
        return Err(absent());
    }
    validate_note_image(&mut transaction, input.image_path.as_deref()).await?;
    sqlx::query(
        "UPDATE wei_yan SET content=?,image_path=?,is_public=? WHERE id=? AND type='friend'",
    )
    .bind(input.content.as_str())
    .bind(input.image_path)
    .bind(i64::from(input.is_public))
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    let note = load_admin_note(&mut transaction, id).await?;
    transaction.commit().await?;
    Ok(Json(note))
}
pub(super) async fn content_notes(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<AdminNote>> {
    let pagination = query.admin_bounds();
    let mut transaction = state.pool.begin().await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wei_yan WHERE type='friend'")
        .fetch_one(&mut *transaction)
        .await?;
    let items = sqlx::query_as::<_,AdminNote>("SELECT w.id,w.user_id,u.username,w.content,w.image_path,w.is_public,w.create_time FROM wei_yan w LEFT JOIN user u ON u.id=w.user_id WHERE w.type='friend' ORDER BY w.id DESC LIMIT ? OFFSET ?")
        .bind(pagination.size).bind(pagination.offset()).fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}
pub(super) async fn delete_note(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let changed = sqlx::query("DELETE FROM wei_yan WHERE id=? AND type='friend'")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if changed.rows_affected() == 0 {
        return Err(absent());
    }
    transaction.commit().await?;
    Ok(Json(serde_json::json!({"deleted":true})))
}
