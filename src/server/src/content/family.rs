use crate::{ApiResult, AppError, AppState, absent, invalid};
use axum::{Json, extract::State};
use blog_application::admin_core::AuthenticatedIdentity;
use blog_application::error::HttpStatus;
use blog_application::server_cli::ContractPath as Path;
use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct PublicFamily {
    id: i64,
    bg_cover: Option<String>,
    man_cover: Option<String>,
    woman_cover: Option<String>,
    man_name: Option<String>,
    woman_name: Option<String>,
    timing: Option<String>,
    countdown_title: Option<String>,
    countdown_time: Option<String>,
    family_info: Option<String>,
}
pub(super) async fn family(State(state): State<AppState>) -> ApiResult<Vec<PublicFamily>> {
    let author = crate::site_author_id(&state.pool).await?;
    Ok(Json(sqlx::query_as::<_,PublicFamily>("SELECT id,bg_cover,man_cover,woman_cover,man_name,woman_name,timing,countdown_title,countdown_time,family_info FROM family WHERE user_id=? AND status=1")
        .bind(author).fetch_all(&state.pool).await?))
}

#[derive(Serialize, sqlx::FromRow)]
pub(super) struct AdminFamily {
    #[sqlx(flatten)]
    #[serde(flatten)]
    profile: PublicFamily,
    status: Option<i64>,
}
pub(super) async fn content_family(State(state): State<AppState>) -> ApiResult<Vec<AdminFamily>> {
    let author = crate::site_author_id(&state.pool).await?;
    Ok(Json(
        sqlx::query_as("SELECT * FROM family WHERE user_id=?")
            .bind(author)
            .fetch_all(&state.pool)
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FamilyInput {
    bg_cover: Option<String>,
    man_cover: Option<String>,
    woman_cover: Option<String>,
    man_name: String,
    woman_name: String,
    timing: String,
    countdown_title: Option<String>,
    countdown_time: Option<String>,
    family_info: Option<String>,
    status: bool,
}
type FamilyWrite =
    sqlx::query::QueryAs<'static, sqlx::Sqlite, AdminFamily, sqlx::sqlite::SqliteArguments>;

impl FamilyInput {
    fn write_query(self, sql: &'static str, author: i64) -> FamilyWrite {
        sqlx::query_as(sql)
            .bind(author)
            .bind(self.bg_cover)
            .bind(self.man_cover)
            .bind(self.woman_cover)
            .bind(self.man_name)
            .bind(self.woman_name)
            .bind(self.timing)
            .bind(self.countdown_title)
            .bind(self.countdown_time)
            .bind(self.family_info)
            .bind(i64::from(self.status))
    }
}
fn valid_image(value: Option<&str>) -> bool {
    value.is_none_or(|v| {
        v.len() <= 2048
            && (v.starts_with("https://") || v.starts_with("http://") || v.starts_with("/media/"))
    })
}
fn validate_family(input: &FamilyInput) -> Result<(), AppError> {
    let invalid_name = |name: &str| name.trim().is_empty() || name.chars().count() > 32;
    if invalid_name(&input.man_name)
        || invalid_name(&input.woman_name)
        || input.timing.chars().count() > 32
        || [&input.countdown_title, &input.countdown_time]
            .into_iter()
            .flatten()
            .any(|value| value.chars().count() > 32)
        || input
            .family_info
            .as_deref()
            .is_some_and(|v| v.chars().count() > 1024)
        || !valid_image(input.bg_cover.as_deref())
        || !valid_image(input.man_cover.as_deref())
        || !valid_image(input.woman_cover.as_deref())
    {
        return Err(invalid("恋爱笔记内容无效"));
    }
    Ok(())
}
pub(super) async fn create_family(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    crate::ContractJson(input): crate::ContractJson<FamilyInput>,
) -> ApiResult<AdminFamily> {
    validate_family(&input)?;
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let author = crate::site_author_id(&mut *transaction).await?;
    let family = input.write_query(
        "INSERT INTO family(user_id,bg_cover,man_cover,woman_cover,man_name,woman_name,timing,countdown_title,countdown_time,family_info,status)
         SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11 WHERE NOT EXISTS (SELECT 1 FROM family WHERE user_id=?1)
         RETURNING *",
        author,
    )
        .fetch_optional(&mut *transaction)
    .await?
    .ok_or(AppError::Public(HttpStatus::Conflict, "家已创建，请编辑现有资料"))?;
    transaction.commit().await?;
    Ok(Json(family))
}
pub(super) async fn update_family(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
    crate::ContractJson(input): crate::ContractJson<FamilyInput>,
) -> ApiResult<AdminFamily> {
    validate_family(&input)?;
    let mut transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let author = crate::site_author_id(&mut *transaction).await?;
    let family = input.write_query(
        "UPDATE family SET bg_cover=?2,man_cover=?3,woman_cover=?4,man_name=?5,woman_name=?6,timing=?7,
         countdown_title=?8,countdown_time=?9,family_info=?10,status=?11,update_time=CURRENT_TIMESTAMP
         WHERE id=?12 AND user_id=?1 RETURNING *",
        author,
    )
    .bind(id)
        .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(absent)?;
    transaction.commit().await?;
    Ok(Json(family))
}
pub(super) async fn delete_family(
    State(state): State<AppState>,
    axum::Extension(identity): axum::Extension<AuthenticatedIdentity>,
    Path(id): Path<i64>,
) -> ApiResult<serde_json::Value> {
    let author = crate::site_author_id(&state.pool).await?;
    let transaction = crate::content::begin_mutation(&state.pool, &identity).await?;
    let deletion = sqlx::query("DELETE FROM family WHERE id=? AND user_id=?")
        .bind(id)
        .bind(author);
    super::finish_delete(transaction, deletion).await
}
