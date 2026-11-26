use super::{PageQuery, PageResult, input::ShortContent};
use crate::{ApiResult, AppError, AppState, absent, invalid};
use axum::{Json, extract::State};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::error::HttpStatus;
use blog_application::server_cli::{ContractPath as Path, ContractQuery as Query};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommentInput {
    request_id: String,
    article_id: i64,
    content: String,
    parent_comment_id: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessageCommentInput {
    request_id: String,
    content: String,
    parent_comment_id: Option<i64>,
}

pub(super) async fn create_comment(
    State(state): State<AppState>,
    crate::ContractJson(input): crate::ContractJson<CommentInput>,
) -> ApiResult<PublicComment> {
    create_anonymous_comment(
        &state,
        CommentTarget::Article(input.article_id),
        CommentSubmission::parse(MessageCommentInput {
            request_id: input.request_id,
            content: input.content,
            parent_comment_id: input.parent_comment_id,
        })?,
    )
    .await
}

pub(super) async fn create_message_comment(
    State(state): State<AppState>,
    crate::ContractJson(input): crate::ContractJson<MessageCommentInput>,
) -> ApiResult<PublicComment> {
    create_anonymous_comment(
        &state,
        CommentTarget::Message,
        CommentSubmission::parse(input)?,
    )
    .await
}

pub(super) async fn create_love_comment(
    State(state): State<AppState>,
    crate::ContractJson(input): crate::ContractJson<MessageCommentInput>,
) -> ApiResult<PublicComment> {
    let source = crate::site_author_id(&state.pool).await?;
    create_anonymous_comment(
        &state,
        CommentTarget::Love(source),
        CommentSubmission::parse(input)?,
    )
    .await
}

fn valid_request_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            14 => byte == b'4',
            19 => matches!(byte, b'8' | b'9' | b'a' | b'b'),
            _ => byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'),
        })
}

enum CommentTarget {
    Article(i64),
    Message,
    Love(i64),
}

impl CommentTarget {
    fn database_fields(&self) -> (i64, &'static str) {
        match *self {
            Self::Article(id) => (id, "article"),
            Self::Message => (0, "message"),
            Self::Love(id) => (id, "love"),
        }
    }
}

struct CommentSubmission {
    request_id: String,
    content: ShortContent,
    parent: i64,
}

impl CommentSubmission {
    fn parse(input: MessageCommentInput) -> Result<Self, AppError> {
        if !valid_request_id(&input.request_id) {
            return Err(invalid("评论请求标识必须为小写 UUIDv4"));
        }
        let parent = input.parent_comment_id.unwrap_or(0);
        if parent < 0 {
            return Err(invalid("评论内容或回复目标无效"));
        }
        let content =
            ShortContent::try_from(input.content).map_err(|_| invalid("评论内容或回复目标无效"))?;
        Ok(Self {
            request_id: input.request_id,
            content,
            parent,
        })
    }
}

async fn create_anonymous_comment(
    state: &AppState,
    target: CommentTarget,
    input: CommentSubmission,
) -> ApiResult<PublicComment> {
    let (source, kind) = target.database_fields();
    let content = input.content.as_str();
    let parent = input.parent;
    let mut hash = Sha256::new();
    hash.update(source.to_be_bytes());
    hash.update(kind.as_bytes());
    hash.update([0]);
    hash.update(content.as_bytes());
    hash.update(parent.to_be_bytes());
    let request_hash = hash.finalize().to_vec();
    // Serialize receipt lookup and insertion with the comment.
    // A lost response, concurrent retry or process restart cannot create a second row.
    let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    // Gate and insert share the writer boundary so a queued request observes
    // administrator changes to publication, comments, deletion or protection.
    if let CommentTarget::Article(_) = target {
        let enabled: Option<i64> = sqlx::query_scalar("SELECT comment_status FROM article WHERE id=? AND deleted=0 AND view_status=1 AND password IS NULL")
            .bind(source).fetch_optional(&mut *transaction).await?;
        if enabled != Some(1) {
            return Err(AppError::Public(HttpStatus::Forbidden, "文章不允许评论"));
        }
    }
    let previous: Option<(Vec<u8>, Option<i64>)> =
        sqlx::query_as("SELECT request_hash,comment_id FROM comment_submission WHERE request_id=?")
            .bind(&input.request_id)
            .fetch_optional(&mut *transaction)
            .await?;
    let id = if let Some((previous_hash, comment_id)) = previous {
        if previous_hash != request_hash {
            return Err(AppError::Public(
                HttpStatus::Conflict,
                "评论请求标识已用于不同内容",
            ));
        }
        comment_id.ok_or(AppError::Public(
            HttpStatus::Conflict,
            "原评论已删除，不会重复发表",
        ))?
    } else {
        let (floor, parent_user) = if parent > 0 {
            let row: Option<(Option<i64>, Option<i64>, Option<i64>)> = sqlx::query_as("SELECT parent_comment_id,floor_comment_id,user_id FROM comment WHERE id=? AND source=? AND type=?")
                .bind(parent).bind(source).bind(kind).fetch_optional(&mut *transaction).await?;
            let Some((parent_parent, parent_floor, user)) = row else {
                return Err(invalid("回复目标不存在"));
            };
            (
                Some(if parent_parent.unwrap_or(0) == 0 {
                    parent
                } else {
                    parent_floor.ok_or(AppError::Public(
                        HttpStatus::Conflict,
                        "回复目标的楼层信息无效",
                    ))?
                }),
                user,
            )
        } else {
            (None, None)
        };
        let id = sqlx::query("INSERT INTO comment(source,type,parent_comment_id,user_id,floor_comment_id,parent_user_id,comment_content) VALUES(?,?,?,NULL,?,?,?)")
            .bind(source).bind(kind).bind(parent).bind(floor).bind(parent_user).bind(content)
            .execute(&mut *transaction).await?.last_insert_rowid();
        sqlx::query(
            "INSERT INTO comment_submission(request_id,request_hash,comment_id) VALUES(?,?,?)",
        )
        .bind(&input.request_id)
        .bind(request_hash)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        id
    };
    let comment = sqlx::query_as::<_, PublicComment>(sqlx::AssertSqlSafe(format!(
        "{COMMENT_SQL} WHERE c.id=?"
    )))
    .bind(id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Json(comment))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommentsQuery {
    article_id: i64,
    page: Option<i64>,
    size: Option<i64>,
}
#[derive(Serialize, sqlx::FromRow)]
pub(super) struct PublicComment {
    id: i64,
    user_id: Option<i64>,
    username: Option<String>,
    avatar: Option<String>,
    comment_content: String,
    create_time: Option<String>,
    parent_comment_id: Option<i64>,
    parent_username: Option<String>,
    floor_comment_id: Option<i64>,
    reply_count: i64,
}
const COMMENT_SQL: &str = "SELECT c.id,c.user_id,u.username,u.avatar,c.comment_content,c.create_time,
    c.parent_comment_id,pu.username AS parent_username,c.floor_comment_id,
    (SELECT COUNT(*) FROM comment child WHERE child.floor_comment_id=c.id AND child.type=c.type) AS reply_count
    FROM comment c LEFT JOIN user u ON u.id=c.user_id LEFT JOIN user pu ON pu.id=c.parent_user_id";
async fn comment_page(
    mut transaction: sqlx::Transaction<'_, sqlx::Sqlite>,
    target: CommentTarget,
    floor: Option<i64>,
    query: PageQuery,
) -> ApiResult<PageResult<PublicComment>> {
    let (count_sql, items_sql, default_size) = if floor.is_some() {
        (
            "SELECT COUNT(*) FROM comment WHERE floor_comment_id=? AND source=? AND type=?",
            "WHERE c.floor_comment_id=? AND c.source=? AND c.type=?
            ORDER BY c.create_time,c.id LIMIT ? OFFSET ?",
            5,
        )
    } else {
        (
            "SELECT COUNT(*) FROM comment WHERE source=? AND type=?
            AND (parent_comment_id IS NULL OR parent_comment_id=0)",
            "WHERE c.source=? AND c.type=?
            AND (c.parent_comment_id IS NULL OR c.parent_comment_id=0)
            ORDER BY c.create_time DESC,c.id DESC LIMIT ? OFFSET ?",
            10,
        )
    };
    let pagination = query.bounds(default_size, 50);
    let (source, kind) = target.database_fields();
    let mut count = sqlx::query_scalar(count_sql);
    let mut data = sqlx::query_as(sqlx::AssertSqlSafe(format!("{COMMENT_SQL} {items_sql}")));
    if let Some(id) = floor {
        count = count.bind(id);
        data = data.bind(id);
    }
    let total = count
        .bind(source)
        .bind(kind)
        .fetch_one(&mut *transaction)
        .await?;
    let items = data
        .bind(source)
        .bind(kind)
        .bind(pagination.size)
        .bind(pagination.offset())
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}

pub(super) async fn comments(
    State(state): State<AppState>,
    Query(query): Query<CommentsQuery>,
) -> ApiResult<PageResult<PublicComment>> {
    // Authorization and returned comments must describe the same snapshot.
    let mut transaction = state.pool.begin().await?;
    super::articles::require_public_article(&mut transaction, query.article_id).await?;
    let pagination = PageQuery {
        page: query.page,
        size: query.size,
    };
    let target = CommentTarget::Article(query.article_id);
    comment_page(transaction, target, None, pagination).await
}

pub(super) async fn comment_replies(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<PublicComment>> {
    let mut transaction = state.pool.begin().await?;
    let article = sqlx::query_scalar(
        "SELECT c.source FROM comment c JOIN article a ON a.id=c.source
         WHERE c.id=? AND c.type='article' AND (c.parent_comment_id IS NULL OR c.parent_comment_id=0)
            AND a.deleted=0 AND a.view_status=1 AND a.password IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(absent)?;
    let target = CommentTarget::Article(article);
    comment_page(transaction, target, Some(id), query).await
}

pub(super) async fn message_comments(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<PublicComment>> {
    let transaction = state.pool.begin().await?;
    comment_page(transaction, CommentTarget::Message, None, query).await
}

pub(super) async fn message_comment_replies(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<PublicComment>> {
    site_comment_replies(state.pool.begin().await?, CommentTarget::Message, id, query).await
}

pub(super) async fn love_comments(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<PublicComment>> {
    let mut transaction = state.pool.begin().await?;
    let source = crate::site_author_id(&mut *transaction).await?;
    comment_page(transaction, CommentTarget::Love(source), None, query).await
}

pub(super) async fn love_comment_replies(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<PublicComment>> {
    let mut transaction = state.pool.begin().await?;
    let source = crate::site_author_id(&mut *transaction).await?;
    site_comment_replies(transaction, CommentTarget::Love(source), id, query).await
}

async fn site_comment_replies(
    mut transaction: sqlx::Transaction<'_, sqlx::Sqlite>,
    target: CommentTarget,
    id: i64,
    query: PageQuery,
) -> ApiResult<PageResult<PublicComment>> {
    let (source, kind) = target.database_fields();
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM comment WHERE id=? AND source=? AND type=?
            AND (parent_comment_id IS NULL OR parent_comment_id=0)",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(absent)?;
    comment_page(transaction, target, Some(id), query).await
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct AdminComment {
    id: i64,
    source: i64,
    comment_type: Option<String>,
    user_id: Option<i64>,
    username: Option<String>,
    comment_content: String,
    create_time: Option<String>,
}
pub(super) async fn admin_comments(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<AdminComment>> {
    let pagination = query.admin_bounds();
    let mut transaction = state.pool.begin().await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comment")
        .fetch_one(&mut *transaction)
        .await?;
    let items=sqlx::query_as::<_,AdminComment>("SELECT c.id,c.source,c.type AS comment_type,c.user_id,u.username,c.comment_content,c.create_time FROM comment c LEFT JOIN user u ON u.id=c.user_id ORDER BY c.id DESC LIMIT ? OFFSET ?")
        .bind(pagination.size).bind(pagination.offset()).fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}
pub(super) async fn delete_comment(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let root: Option<Option<i64>> =
        sqlx::query_scalar("SELECT parent_comment_id FROM comment WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?;
    let Some(parent) = root else {
        return Err(absent());
    };
    if parent.unwrap_or(0) == 0 {
        sqlx::query("DELETE FROM comment WHERE id=? OR floor_comment_id=?")
            .bind(id)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    } else {
        sqlx::query("DELETE FROM comment WHERE id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(Json(serde_json::json!({"deleted":true})))
}
