use super::{
    PageQuery, PageResult,
    input::{IdempotencyKey, ShortContent},
};
use crate::{ApiResult, AppError, AppState, absent, invalid};
use axum::{Json, extract::State, http::HeaderMap};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::server_cli::{ContractPath as Path, ContractQuery as Query};
use blog_application::{admin_auth::Password, error::HttpStatus};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Serialize, sqlx::FromRow)]
struct ArticlePresentation {
    id: i64,
    article_title: String,
    article_cover: Option<String>,
    sort_id: i64,
    label_id: i64,
    sort_name: Option<String>,
    label_name: Option<String>,
    view_count: i64,
    like_count: i64,
    comment_count: i64,
    recommend_status: i64,
    view_status: i64,
    create_time: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct ArticleSummary {
    #[sqlx(flatten)]
    #[serde(flatten)]
    presentation: ArticlePresentation,
    excerpt: Option<String>,
    search_snippet: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct ArticleDetail {
    #[sqlx(flatten)]
    #[serde(flatten)]
    presentation: ArticlePresentation,
    user_id: i64,
    article_content: String,
    video_url: Option<String>,
    username: Option<String>,
    comment_status: i64,
    password_required: i64,
    tips: Option<String>,
    update_time: Option<String>,
}

const ARTICLE_PRESENTATION_COLUMNS: &str = "id,article_title,article_cover,sort_id,label_id,
    (SELECT sort_name FROM sort WHERE sort.id=article.sort_id) AS sort_name,
    (SELECT label_name FROM label WHERE label.id=article.label_id) AS label_name,
    view_count,like_count,
    (SELECT COUNT(*) FROM comment WHERE source=article.id AND type='article') AS comment_count,
    recommend_status,view_status,create_time";
const ARTICLE_SUMMARY_COLUMNS: &str = r"CASE WHEN password IS NULL THEN substr(article_content,1,400) ELSE NULL END AS excerpt,
    CASE WHEN ?2<>'' AND password IS NULL AND article_title NOT LIKE ?3 ESCAPE '\'
        THEN substr(article_content,max(1,instr(lower(article_content),lower(?2))-48),160)
        ELSE NULL END AS search_snippet";
const ARTICLE_DETAIL_COLUMNS: &str = "user_id,article_content,video_url,
    (SELECT username FROM user WHERE user.id=article.user_id) AS username,
    comment_status,CASE WHEN password IS NULL THEN 0 ELSE 1 END AS password_required,
    tips,update_time";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArticleQuery {
    page: Option<i64>,
    size: Option<i64>,
    sort_id: Option<i64>,
    label_id: Option<i64>,
    search: Option<String>,
    recommended: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
enum ArticleAudience {
    Public,
    Administrator,
}

async fn list_articles(
    pool: &SqlitePool,
    query: ArticleQuery,
    audience: ArticleAudience,
) -> Result<PageResult<ArticleSummary>, AppError> {
    let pagination = PageQuery {
        page: query.page,
        size: query.size,
    }
    .bounds(12, 100);
    let search = query.search.as_deref().unwrap_or_default().trim();
    if search.encode_utf16().count() > 200 {
        return Err(invalid("搜索词过长"));
    }
    let pattern = format!(
        "%{}%",
        search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let recommended = query.recommended.map(i64::from);
    let phrase = format!("\"{}\"", search.replace('"', "\"\""));
    let (count_sql, list_sql) = article_page_sql(&query, audience, search);
    let mut transaction = pool.begin().await?;
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .bind(query.sort_id)
        .bind(search)
        .bind(&pattern)
        .bind(recommended)
        .bind(&phrase)
        .bind(query.label_id)
        .fetch_one(&mut *transaction)
        .await?;
    let items = sqlx::query_as::<_, ArticleSummary>(sqlx::AssertSqlSafe(list_sql))
        .bind(query.sort_id)
        .bind(search)
        .bind(&pattern)
        .bind(recommended)
        .bind(&phrase)
        .bind(query.label_id)
        .bind(pagination.size)
        .bind(pagination.offset())
        .fetch_all(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(pagination.result(items, total))
}

fn article_page_sql(
    query: &ArticleQuery,
    audience: ArticleAudience,
    search: &str,
) -> (String, String) {
    const PAGE_IDENTITY: &str = "SELECT id AS article_id,create_time AS page_created_at";
    const DATE_ORDER: &str = "page_created_at DESC,article_id DESC";
    // Only column names and SQL fragments come from here; user values stay bound.
    let filters = [
        query.sort_id.map_or("?1 IS NULL", |_| "sort_id=?1"),
        query.label_id.map_or("?6 IS NULL", |_| "label_id=?6"),
        query
            .recommended
            .map_or("?4 IS NULL", |_| "recommend_status=?4"),
    ]
    .join(" AND ");
    let (count, page) = match (audience, search) {
        (ArticleAudience::Public, "") => {
            // Disjoint index ranges keep the public and protected rows in date order.
            let [visible, protected_before, protected_after] = [
                "view_status=1",
                "view_status<1 AND password IS NOT NULL",
                "view_status>1 AND password IS NOT NULL",
            ]
            .map(|visibility| {
                format!("FROM article WHERE deleted=0 AND {visibility} AND {filters}")
            });
            (
                format!(
                    "SELECT (SELECT COUNT(*) {visible}) + (SELECT COUNT(*) {protected_before}) + (SELECT COUNT(*) {protected_after})"
                ),
                format!(
                    "{PAGE_IDENTITY},0 AS page_search_rank {visible} UNION ALL {PAGE_IDENTITY},0 AS page_search_rank {protected_before} UNION ALL {PAGE_IDENTITY},0 AS page_search_rank {protected_after} ORDER BY {DATE_ORDER} LIMIT ?7 OFFSET ?8"
                ),
            )
        }
        (audience, search) => {
            let (visible, body_guard) = match audience {
                ArticleAudience::Public => (
                    " AND (view_status=1 OR password IS NOT NULL)",
                    "password IS NULL AND ",
                ),
                ArticleAudience::Administrator => ("", ""),
            };
            let (table, text, rank, order) = if search.is_empty() {
                ("article", String::new(), "0", DATE_ORDER)
            } else {
                // Prefer a taxonomy index when available; global trigram searches start with FTS.
                // Embedded NUL still uses the existing SQLite length/LIKE predicates.
                let (table, hits) = if query.sort_id.is_none()
                    && query.label_id.is_none()
                    && !search.contains('\0')
                    && search.chars().nth(2).is_some()
                {
                    (
                        "(SELECT rowid AS hit_id FROM article_search WHERE article_search MATCH ?5) AS hits CROSS JOIN article ON article.id=hits.hit_id",
                        "",
                    )
                } else {
                    (
                        "article",
                        " AND (length(?2)<3 OR id IN (SELECT rowid FROM article_search WHERE article_search MATCH ?5))",
                    )
                };
                (
                    table,
                    format!(
                        r" AND (article_title LIKE ?3 ESCAPE '\' OR ({body_guard}article_content LIKE ?3 ESCAPE '\')){hits}"
                    ),
                    r"CASE WHEN article_title LIKE ?3 ESCAPE '\' THEN 0 ELSE 1 END",
                    "page_search_rank,page_created_at DESC,article_id DESC",
                )
            };
            let from = format!("FROM {table} WHERE deleted=0{visible} AND {filters}{text}");
            (
                format!("SELECT COUNT(*) {from}"),
                format!(
                    "{PAGE_IDENTITY},{rank} AS page_search_rank {from} ORDER BY {order} LIMIT ?7 OFFSET ?8"
                ),
            )
        }
    };
    // Materialize only a page of IDs before reading body excerpts, taxonomy and
    // comment counts. CROSS JOIN fixes the page as the outer loop, so OFFSET
    // and sorting never process full article summaries for discarded rows.
    let list = format!(
        "WITH article_page AS MATERIALIZED ({page}) SELECT {ARTICLE_PRESENTATION_COLUMNS},{ARTICLE_SUMMARY_COLUMNS} FROM article_page CROSS JOIN article WHERE article.id=article_page.article_id ORDER BY article_page.page_search_rank,article_page.page_created_at DESC,article_page.article_id DESC"
    );
    (count, list)
}

pub(super) async fn public_articles(
    State(state): State<AppState>,
    Query(query): Query<ArticleQuery>,
) -> ApiResult<PageResult<ArticleSummary>> {
    Ok(Json(
        list_articles(&state.pool, query, ArticleAudience::Public).await?,
    ))
}
pub(super) async fn admin_articles(
    State(state): State<AppState>,
    Query(query): Query<ArticleQuery>,
) -> ApiResult<PageResult<ArticleSummary>> {
    Ok(Json(
        list_articles(&state.pool, query, ArticleAudience::Administrator).await?,
    ))
}

async fn load_article<'executor, E>(
    executor: E,
    id: i64,
    audience: ArticleAudience,
) -> Result<ArticleDetail, AppError>
where
    E: sqlx::Executor<'executor, Database = sqlx::Sqlite>,
{
    let condition = match audience {
        ArticleAudience::Administrator => "id=? AND deleted=0",
        ArticleAudience::Public => "id=? AND deleted=0 AND view_status=1 AND password IS NULL",
    };
    sqlx::query_as::<_, ArticleDetail>(sqlx::AssertSqlSafe(format!(
        "SELECT {ARTICLE_PRESENTATION_COLUMNS},{ARTICLE_DETAIL_COLUMNS} FROM article WHERE {condition}"
    )))
    .bind(id)
    .fetch_optional(executor)
    .await?
    .ok_or_else(absent)
}

pub(super) async fn public_article(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ArticleDetail> {
    let article = load_article(&state.pool, id, ArticleAudience::Public).await?;
    sqlx::query("UPDATE article SET view_count=view_count+1 WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(article))
}
#[derive(Serialize, sqlx::FromRow)]
pub(super) struct ArticleNews {
    id: i64,
    content: String,
    create_time: Option<String>,
}
const ARTICLE_NEWS_SQL: &str = "SELECT w.id,w.content,w.create_time FROM wei_yan w";

pub(super) async fn require_public_article(
    connection: &mut sqlx::SqliteConnection,
    id: i64,
) -> Result<(), AppError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM article WHERE id=? AND deleted=0 AND view_status=1 AND password IS NULL",
    )
    .bind(id)
    .fetch_optional(connection)
    .await?
    .ok_or_else(absent)?;
    Ok(())
}

pub(super) async fn article_news(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Vec<ArticleNews>> {
    let mut transaction = state.pool.begin().await?;
    require_public_article(&mut transaction, id).await?;
    let rows=sqlx::query_as::<_,ArticleNews>(sqlx::AssertSqlSafe(format!("{ARTICLE_NEWS_SQL} WHERE w.type='news' AND w.source=? AND w.is_public=1 ORDER BY w.create_time DESC,w.id DESC LIMIT 100")))
        .bind(id).fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(rows))
}
pub(super) async fn admin_article_news(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> ApiResult<PageResult<ArticleNews>> {
    let pagination = query.admin_bounds();
    let mut transaction = state.pool.begin().await?;
    article_author(&mut transaction, id).await?;
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM wei_yan WHERE type='news' AND source=?")
            .bind(id)
            .fetch_one(&mut *transaction)
            .await?;
    let items = sqlx::query_as::<_, ArticleNews>(sqlx::AssertSqlSafe(format!("{ARTICLE_NEWS_SQL} WHERE w.type='news' AND w.source=? ORDER BY w.create_time DESC,w.id DESC LIMIT ? OFFSET ?")))
        .bind(id).bind(pagination.size).bind(pagination.offset()).fetch_all(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(Json(pagination.result(items, total)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArticleNewsInput {
    content: ShortContent,
    create_time: Option<String>,
}
pub(super) async fn admin_create_article_news(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    crate::ContractJson(input): crate::ContractJson<ArticleNewsInput>,
) -> ApiResult<ArticleNews> {
    let request_id = IdempotencyKey::from_headers(&headers)?;
    Ok(Json(
        insert_article_news(&state, &identity, id, input, request_id).await?,
    ))
}
pub(super) async fn admin_delete_article_news(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path((article_id, news_id)): Path<(i64, i64)>,
) -> ApiResult<serde_json::Value> {
    let transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let deletion = sqlx::query("DELETE FROM wei_yan WHERE id=? AND source=? AND type='news'")
        .bind(news_id)
        .bind(article_id);
    super::finish_delete(transaction, deletion).await
}

pub(super) async fn admin_update_article_news(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path((article_id, news_id)): Path<(i64, i64)>,
    crate::ContractJson(input): crate::ContractJson<ArticleNewsInput>,
) -> ApiResult<ArticleNews> {
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    sqlx::query_scalar::<_, i64>("SELECT id FROM article WHERE id=? AND deleted=0")
        .bind(article_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(absent)?;
    let time = normalize_news_time(input.create_time)?;
    let result = sqlx::query_as(
        "UPDATE wei_yan SET content=?,create_time=COALESCE(?,create_time)
         WHERE id=? AND source=? AND type='news' RETURNING id,content,create_time",
    )
    .bind(input.content.as_str())
    .bind(time)
    .bind(news_id)
    .bind(article_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(absent)?;
    transaction.commit().await?;
    Ok(Json(result))
}

fn normalize_news_time(raw: Option<String>) -> Result<Option<String>, AppError> {
    use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike, Utc};

    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    if raw.len() > 64 {
        return Err(invalid("日期时间无效"));
    }
    let value = raw.trim().replacen(' ', "T", 1);
    // Only absolute calendar input is admitted. SQLite's datetime() also accepts
    // relative values such as "now", which make the same retry hash differently.
    let parsed = DateTime::parse_from_rfc3339(&value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"]
                .into_iter()
                .find_map(|format| NaiveDateTime::parse_from_str(&value, format).ok())
                .map(|value| value.and_utc())
        })
        .or_else(|| {
            NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .ok()
                .and_then(|value| value.and_hms_opt(0, 0, 0))
                .map(|value| value.and_utc())
        })
        .filter(|value| (0..=9999).contains(&value.year()) && value.nanosecond() < 1_000_000_000)
        .ok_or_else(|| invalid("日期时间无效"))?;
    Ok(Some(parsed.format("%Y-%m-%d %H:%M:%S").to_string()))
}

async fn article_author(connection: &mut sqlx::SqliteConnection, id: i64) -> Result<i64, AppError> {
    sqlx::query_scalar("SELECT user_id FROM article WHERE id=? AND deleted=0")
        .bind(id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(absent)
}

async fn insert_article_news(
    state: &AppState,
    identity: &AuthenticatedIdentity,
    id: i64,
    input: ArticleNewsInput,
    request_id: Option<IdempotencyKey<'_>>,
) -> Result<ArticleNews, AppError> {
    let content = input.content.as_str();
    let time = normalize_news_time(input.create_time)?;
    // Bind the key to its article and normalized original draft, including time.
    let request = request_id.map(|key| key.bind_payload(&(id, content, &time)));
    let mut transaction = crate::content::begin_mutation(&state.pool, identity).await?;
    let author = article_author(&mut transaction, id).await?;
    let news_id = if let Some(request) = &request
        && let Some((hash, news_id)) = sqlx::query_as::<_, (Vec<u8>, Option<i64>)>(
            "SELECT request_hash,news_id FROM article_news_submission WHERE request_id=?",
        )
        .bind(request.key.as_str())
        .fetch_optional(&mut *transaction)
        .await?
    {
        if hash != request.hash {
            return Err(AppError::Public(
                HttpStatus::Conflict,
                "进展请求标识已用于不同内容",
            ));
        }
        news_id.ok_or(AppError::Public(
            HttpStatus::Conflict,
            "原进展已删除，不会重复发表",
        ))?
    } else {
        let news_id = sqlx::query("INSERT INTO wei_yan(user_id,content,type,source,is_public,create_time) VALUES(?,?,'news',?,1,COALESCE(?,CURRENT_TIMESTAMP))")
            .bind(author).bind(content).bind(id).bind(time).execute(&mut *transaction).await?.last_insert_rowid();
        if let Some(request) = request {
            sqlx::query("INSERT INTO article_news_submission(request_id,request_hash,news_id) VALUES(?,?,?)")
                .bind(request.key.as_str()).bind(request.hash.as_slice()).bind(news_id).execute(&mut *transaction).await?;
        }
        news_id
    };
    let result = sqlx::query_as::<_, ArticleNews>(sqlx::AssertSqlSafe(format!(
        "{ARTICLE_NEWS_SQL} WHERE w.id=? AND w.source=? AND w.type='news'"
    )))
    .bind(news_id)
    .bind(id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(result)
}
#[derive(Serialize, sqlx::FromRow)]
pub(super) struct ArticleAccess {
    password_required: i64,
    tips: Option<String>,
}
pub(super) async fn article_access(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ArticleAccess> {
    Ok(Json(sqlx::query_as::<_,ArticleAccess>("SELECT CASE WHEN password IS NULL THEN 0 ELSE 1 END AS password_required,tips FROM article WHERE id=? AND deleted=0 AND (view_status=1 OR password IS NOT NULL)")
        .bind(id).fetch_optional(&state.pool).await?.ok_or_else(absent)?))
}
pub(super) async fn admin_article(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<ArticleDetail> {
    Ok(Json(
        load_article(&state.pool, id, ArticleAudience::Administrator).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UnlockRequest {
    password: String,
}
pub(super) async fn unlock_article(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    crate::ContractJson(input): crate::ContractJson<UnlockRequest>,
) -> ApiResult<ArticleDetail> {
    if input.password.len() > 1024 {
        return Err(invalid("访问密码无效"));
    }
    let stored: String = sqlx::query_scalar(
        "SELECT password FROM article WHERE id=? AND deleted=0 AND password IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(absent)?;
    let permit = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        std::sync::Arc::clone(&state.password_slots).acquire_owned(),
    )
    .await
    .map_err(|source| AppError::Busy {
        message: "密码验证暂时繁忙",
        source,
    })?
    .expect("invariant: password semaphore is never closed");
    let password = input.password;
    // The permit remains held by the worker even if the HTTP request is cancelled.
    let (allowed, stored) = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (
            blog_application::admin_auth::verify_password(&password, &stored),
            stored,
        )
    })
    .await
    .map_err(|source| AppError::Task {
        message: "密码验证失败",
        source,
    })?;
    if !allowed {
        return Err(AppError::Public(HttpStatus::Forbidden, "访问密码错误"));
    }
    // Recheck access after waiting/hashing, and read the body under the same
    // writer boundary so a concurrent password change cannot expose new content.
    let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let changed = sqlx::query(
        "UPDATE article SET view_count=view_count+1 WHERE id=? AND deleted=0 AND password=?",
    )
    .bind(id)
    .bind(stored)
    .execute(&mut *transaction)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(AppError::Public(
            HttpStatus::Forbidden,
            "文章访问权限已变更，请重试",
        ));
    }
    let article = load_article(&mut *transaction, id, ArticleAudience::Administrator).await?;
    transaction.commit().await?;
    Ok(Json(article))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArticleDraft {
    article_title: String,
    article_content: String,
    article_cover: Option<String>,
    video_url: Option<String>,
    sort_id: i64,
    label_id: i64,
    view_status: bool,
    recommend_status: bool,
    comment_status: bool,
    password: Option<String>,
    tips: Option<String>,
}

#[derive(Deserialize)]
#[serde(try_from = "ArticleDraft")]
pub(super) struct ArticleInput(ArticleDraft);

#[derive(Debug, thiserror::Error)]
enum ArticleInputError {
    #[error("文章标题无效")]
    Title,
    #[error("文章内容无效")]
    Content,
    #[error("请选择分类和标签")]
    Taxonomy,
    #[error("公开文章不可设置访问密码")]
    PublicPassword,
}

impl TryFrom<ArticleDraft> for ArticleInput {
    type Error = ArticleInputError;

    fn try_from(input: ArticleDraft) -> Result<Self, Self::Error> {
        if input.article_title.trim().is_empty() || input.article_title.chars().count() > 120 {
            return Err(ArticleInputError::Title);
        }
        if input.article_content.trim().is_empty() || input.article_content.len() > 2_000_000 {
            return Err(ArticleInputError::Content);
        }
        if input.sort_id < 1 || input.label_id < 1 {
            return Err(ArticleInputError::Taxonomy);
        }
        if input.view_status && input.password.is_some() {
            return Err(ArticleInputError::PublicPassword);
        }
        Ok(Self(input))
    }
}

// Serialize taxonomy validation with label moves/deletes and category deletion.
// An article must never commit a label from another (or a missing) category.
async fn validate_article_taxonomy(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &ArticleDraft,
) -> Result<(), AppError> {
    let matches: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM label l JOIN sort s ON s.id=l.sort_id WHERE l.id=? AND s.id=?)",
    )
    .bind(input.label_id)
    .bind(input.sort_id)
    .fetch_one(&mut **transaction)
    .await?;
    if !matches {
        return Err(invalid("标签不属于所选分类，请重新选择分类和标签"));
    }
    Ok(())
}

pub(super) async fn create_article(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    headers: HeaderMap,
    crate::ContractJson(ArticleInput(input)): crate::ContractJson<ArticleInput>,
) -> ApiResult<ArticleDetail> {
    if !input.view_status && input.password.is_none() {
        return Err(invalid("加密文章必须设置访问密码"));
    }
    let request = IdempotencyKey::from_headers(&headers)?.map(|key| key.bind_payload(&input));
    let password = input
        .password
        .as_deref()
        .map(Password::new)
        .transpose()
        .map_err(AppError::InvalidPassword)?;
    let password = hash_article_password(&state, password).await?;
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    if let Some(request) = &request
        && let Some((hash, id, deleted)) = sqlx::query_as::<_, (Vec<u8>, i64, i64)>(
            "SELECT s.request_hash,s.article_id,a.deleted FROM article_submission s
             JOIN article a ON a.id=s.article_id WHERE s.request_id=?",
        )
        .bind(request.key.as_str())
        .fetch_optional(&mut *transaction)
        .await?
    {
        if hash != request.hash || deleted != 0 {
            return Err(AppError::Public(
                HttpStatus::Conflict,
                "文章请求已使用或文章已删除",
            ));
        }
        let article = load_article(&mut *transaction, id, ArticleAudience::Administrator).await?;
        transaction.commit().await?;
        return Ok(Json(article));
    }
    let author = crate::site_author_id(&mut *transaction).await?;
    validate_article_taxonomy(&mut transaction, &input).await?;
    let id = sqlx::query("INSERT INTO article(user_id,sort_id,label_id,article_cover,article_title,article_content,video_url,view_status,recommend_status,comment_status,password,tips) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(author)
        .bind(input.sort_id).bind(input.label_id).bind(input.article_cover).bind(input.article_title).bind(input.article_content).bind(input.video_url)
        .bind(i64::from(input.view_status)).bind(i64::from(input.recommend_status)).bind(i64::from(input.comment_status)).bind(password).bind(input.tips)
        .execute(&mut *transaction).await?.last_insert_rowid();
    if let Some(request) = request {
        sqlx::query(
            "INSERT INTO article_submission(request_id,request_hash,article_id) VALUES(?,?,?)",
        )
        .bind(request.key.as_str())
        .bind(request.hash.as_slice())
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    let article = load_article(&mut *transaction, id, ArticleAudience::Administrator).await?;
    transaction.commit().await?;
    Ok(Json(article))
}

async fn hash_article_password(
    state: &AppState,
    password: Option<Password>,
) -> Result<Option<String>, AppError> {
    let Some(password) = password else {
        return Ok(None);
    };
    let permit = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        std::sync::Arc::clone(&state.password_slots).acquire_owned(),
    )
    .await
    .map_err(|source| AppError::Busy {
        message: "密码计算暂时繁忙",
        source,
    })?
    .expect("invariant: password semaphore is never closed");
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        password.hash()
    })
    .await
    .map_err(|source| AppError::Task {
        message: "密码计算失败",
        source,
    })?
    .map(Some)
    .map_err(AppError::Password)
}

pub(super) async fn update_article(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
    crate::ContractJson(ArticleInput(input)): crate::ContractJson<ArticleInput>,
) -> ApiResult<ArticleDetail> {
    let password = input
        .password
        .as_deref()
        .map(Password::new)
        .transpose()
        .map_err(AppError::InvalidPassword)?;
    let password = hash_article_password(&state, password).await?;
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let has_password: bool =
        sqlx::query_scalar("SELECT password IS NOT NULL FROM article WHERE id=? AND deleted=0")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(absent)?;
    if !input.view_status && input.password.is_none() && !has_password {
        return Err(invalid("加密文章必须设置访问密码"));
    }
    validate_article_taxonomy(&mut transaction, &input).await?;

    sqlx::query("UPDATE article SET sort_id=?,label_id=?,article_cover=?,article_title=?,article_content=?,video_url=?,view_status=?,recommend_status=?,comment_status=?,password=CASE WHEN ?=1 THEN NULL ELSE COALESCE(?,password) END,tips=?,update_time=CURRENT_TIMESTAMP WHERE id=? AND deleted=0")
        .bind(input.sort_id).bind(input.label_id).bind(input.article_cover).bind(input.article_title).bind(input.article_content).bind(input.video_url)
        .bind(i64::from(input.view_status)).bind(i64::from(input.recommend_status)).bind(i64::from(input.comment_status)).bind(i64::from(input.view_status)).bind(password).bind(input.tips).bind(id)
        .execute(&mut *transaction).await?;
    let article = load_article(&mut *transaction, id, ArticleAudience::Administrator).await?;
    transaction.commit().await?;
    Ok(Json(article))
}

pub(super) async fn delete_article(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let deletion = sqlx::query(
        "UPDATE article SET deleted=1,update_time=CURRENT_TIMESTAMP WHERE id=? AND deleted=0",
    )
    .bind(id);
    super::finish_delete(transaction, deletion).await
}

#[path = "../../../../test/server/unit/content/articles/pagination_tests.rs"]
#[cfg(test)]
mod pagination_tests;
