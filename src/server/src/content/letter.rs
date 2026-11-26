use crate::{ApiResult, AppError, AppState, invalid};
use axum::{Json, extract::State};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::error::HttpStatus;
use serde::{Deserialize, Serialize};

const MAX_STORED_LETTER_BYTES: i64 = 128 * 1024;
const INVALID_LETTER: &str = "信笺收件人与署名应为 1 至 100 字，正文应为 1 至 16384 字";

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LetterFields {
    recipient: String,
    sender: String,
    content: String,
}

#[derive(Deserialize, Serialize)]
#[serde(try_from = "LetterFields")]
pub(super) struct LetterContent(LetterFields);

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManagedLetter {
    recipient: String,
    sender: String,
    content: String,
    is_public: bool,
}

#[derive(sqlx::FromRow)]
struct StoredLetter {
    id: i64,
    content: Option<String>,
    is_public: Option<i64>,
    image_path: Option<String>,
    source: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
#[error("{INVALID_LETTER}")]
struct LetterContentError;

impl TryFrom<LetterFields> for LetterContent {
    type Error = LetterContentError;

    fn try_from(fields: LetterFields) -> Result<Self, Self::Error> {
        if fields.recipient.trim().is_empty()
            || fields.recipient.chars().count() > 100
            || fields.sender.trim().is_empty()
            || fields.sender.chars().count() > 100
            || fields.content.trim().is_empty()
            || fields.content.chars().count() > 16_384
        {
            return Err(LetterContentError);
        }
        Ok(Self(fields))
    }
}

impl LetterContent {
    fn into_managed(self, is_public: bool) -> ManagedLetter {
        ManagedLetter {
            recipient: self.0.recipient,
            sender: self.0.sender,
            content: self.0.content,
            is_public,
        }
    }
}

fn stored_content(stored: &StoredLetter) -> Result<LetterContent, AppError> {
    let malformed = || AppError::Public(HttpStatus::Conflict, "信笺存储内容无效");
    if !matches!(stored.is_public, Some(0 | 1))
        || stored.image_path.is_some()
        || stored.source.is_some()
    {
        return Err(malformed());
    }
    let raw = stored.content.as_deref().ok_or_else(malformed)?;
    serde_json::from_str(raw).map_err(AppError::StoredLetter)
}

async fn load_stored_letter(
    connection: &mut sqlx::SqliteConnection,
    author: i64,
) -> Result<Option<StoredLetter>, AppError> {
    // The reserved type is scoped to the current site author. Detect duplicate
    // rows rather than choosing a different visible letter after hiding one.
    let mut rows = sqlx::query_as::<_, StoredLetter>(
        "SELECT id,CASE WHEN length(CAST(content AS BLOB))<=? THEN content ELSE NULL END AS content,is_public,image_path,source FROM wei_yan WHERE type='letter' AND user_id=? ORDER BY id LIMIT 2",
    )
    .bind(MAX_STORED_LETTER_BYTES)
    .bind(author)
    .fetch_all(connection)
    .await?;
    if rows.len() > 1 {
        return Err(AppError::Public(
            HttpStatus::Conflict,
            "站点作者存在多份信笺",
        ));
    }
    Ok(rows.pop())
}

pub(super) async fn public_letter(
    State(state): State<AppState>,
) -> ApiResult<Option<LetterContent>> {
    let author = crate::site_author_id(&state.pool).await?;
    let mut connection = state.pool.acquire().await?;
    let Some(stored) = load_stored_letter(&mut connection, author).await? else {
        return Ok(Json(None));
    };
    let content = stored_content(&stored)?;
    Ok(Json((stored.is_public == Some(1)).then_some(content)))
}

pub(super) async fn admin_letter(State(state): State<AppState>) -> ApiResult<ManagedLetter> {
    let author = crate::site_author_id(&state.pool).await?;
    let mut connection = state.pool.acquire().await?;
    let Some(stored) = load_stored_letter(&mut connection, author).await? else {
        // A read has no side effects. An empty private draft is only a UI model.
        return Ok(Json(ManagedLetter::default()));
    };
    let content = stored_content(&stored)?;
    Ok(Json(content.into_managed(stored.is_public == Some(1))))
}

pub(super) async fn save_letter(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    crate::ContractJson(input): crate::ContractJson<ManagedLetter>,
) -> ApiResult<ManagedLetter> {
    let content = LetterContent::try_from(LetterFields {
        recipient: input.recipient.trim().to_owned(),
        sender: input.sender.trim().to_owned(),
        content: input.content.trim().to_owned(),
    })
    .map_err(|LetterContentError| invalid(INVALID_LETTER))?;
    // At most six JSON bytes per character keeps the validated fields below 128 KiB.
    let encoded =
        serde_json::to_string(&content).expect("invariant: letter strings serialize to JSON");
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let author = crate::site_author_id(&mut *transaction).await?;
    if let Some(stored) = load_stored_letter(&mut transaction, author).await? {
        stored_content(&stored)?;
        sqlx::query(
            "UPDATE wei_yan SET content=?,is_public=? WHERE id=? AND type='letter' AND user_id=?",
        )
        .bind(encoded)
        .bind(i64::from(input.is_public))
        .bind(stored.id)
        .bind(author)
        .execute(&mut *transaction)
        .await?;
    } else {
        // BEGIN IMMEDIATE serializes check-and-insert, providing singleton
        // semantics without changing the immutable database schema.
        sqlx::query("INSERT INTO wei_yan(user_id,content,type,is_public) VALUES(?,?,'letter',?)")
            .bind(author)
            .bind(encoded)
            .bind(i64::from(input.is_public))
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(Json(content.into_managed(input.is_public)))
}
