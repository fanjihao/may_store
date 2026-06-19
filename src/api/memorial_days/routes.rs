// API - 纪念日路由
// FSD §24.9 compliant

use chrono::{Datelike, NaiveDate};
use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::utils::response::ApiResponse;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/memorial-days")
            .route(web::get().to(list_memorial_days))
            .route(web::post().to(create_memorial_day)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/memorial-days/upcoming")
            .route(web::get().to(upcoming_memorial_days)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/memorial-days/{id}")
            .route(web::get().to(get_memorial_day))
            .route(web::patch().to(update_memorial_day))
            .route(web::delete().to(delete_memorial_day)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/memorial-days/{id}/pin")
            .route(web::post().to(pin_memorial_day))
            .route(web::delete().to(unpin_memorial_day)),
    );
}

// ========== 实体 / DTO ==========

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemorialDayOut {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub memorial_date: NaiveDate,
    pub calendar_type: String,        // SOLAR / LUNAR
    pub lunar_month: Option<i16>,
    pub lunar_day: Option<i16>,
    pub is_leap_month: bool,
    pub is_default: i16,
    pub days_until: i64,             // 距离今天还有几天（负数=已过）
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 置顶响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PinResponse {
    /// 当前置顶的纪念日 ID（None = 没置顶）
    pub pinned_id: Option<i64>,
    /// 置顶时间（ISO8601 字符串，None = 没置顶）
    pub pinned_at: Option<String>,
}

/// 取消置顶响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnpinResponse {
    /// 置顶后该字段为 None
    pub pinned_id: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateMemorialDayInput {
    pub name: String,                  // 1-50 字符
    pub description: Option<String>,   // <= 500 字符
    pub memorial_date: NaiveDate,     // 阳历日期
    pub calendar_type: String,        // SOLAR / LUNAR
    pub lunar_month: Option<i16>,     // 阴历月（仅 LUNAR）
    pub lunar_day: Option<i16>,       // 阴历日（仅 LUNAR）
    pub is_leap_month: Option<bool>,  // 是否闰月（仅 LUNAR）
}

/// 更新纪念日输入
///
/// **设计约束**:不接收 `isDefault` 字段。pin 走专门的 `POST /pin` 端点,
/// PATCH 只动 name/description/date/calendar 等普通字段,防止 PATCH 绕过 pin 流程。
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMemorialDayInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub memorial_date: Option<NaiveDate>,
    pub calendar_type: Option<String>,
    pub lunar_month: Option<i16>,
    pub lunar_day: Option<i16>,
    pub is_leap_month: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ListMemorialDaysQuery {
    pub limit: Option<i64>,            // 默认 50, 最大 100
    pub upcoming_days: Option<i64>,    // 只返回 N 天内即将到来的
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct UpcomingQuery {
    pub days: Option<i64>,            // 默认 30
}

// ========== 工具函数 ==========

/// 计算到下一个纪念日的天数
fn days_until_next_occurrence(
    memorial_date: NaiveDate,
    calendar_type: &str,
    lunar_month: Option<i16>,
    lunar_day: Option<i16>,
    _is_leap_month: bool,
    today: NaiveDate,
) -> i64 {
    if calendar_type == "SOLAR" {
        let next = next_solar_occurrence(memorial_date, today);
        (next - today).num_days()
    } else {
        // 阴历：按月+日匹配明年（简化：明年同月同日）
        if let (Some(m), Some(d)) = (lunar_month, lunar_day) {
            // 简化处理：阴历纪念日转换为阳历近似日期
            // MVP: 假设每年阳历同日（生产可接入 chinese-lunisolar-calendar 库）
            let next_year = today.year() + 1;
            let approx = NaiveDate::from_ymd_opt(next_year, m as u32, d as u32)
                .unwrap_or(memorial_date);
            (approx - today).num_days()
        } else {
            0
        }
    }
}

fn next_solar_occurrence(memorial_date: NaiveDate, today: NaiveDate) -> NaiveDate {
    let this_year_date = memorial_date
        .with_year(today.year())
        .unwrap_or(memorial_date);
    if this_year_date >= today {
        this_year_date
    } else {
        memorial_date.with_year(today.year() + 1).unwrap_or(memorial_date)
    }
}

// ========== 处理器 ==========

/// 获取纪念日列表
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/memorial-days",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("limit" = Option<i64>, Query, description = "限制条数"),
        ("upcoming_days" = Option<i64>, Query, description = "只返回 N 天内即将到来")
    ),
    responses(
        (status = 200, description = "成功"),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_memorial_days(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    query: Query<ListMemorialDaysQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let limit = query.limit.unwrap_or(50).min(100);
    let upcoming_days = query.upcoming_days;

    // 校验成员身份
    verify_group_member(&state, token.user_id, group_id).await?;

    let today = chrono::Local::now().date_naive();
    let rows = sqlx::query(
        r#"SELECT id, group_id, name, description, memorial_date, calendar_type,
                  lunar_month, lunar_day, is_leap_month, is_default, created_at
           FROM memorial_day
           WHERE group_id = $1
           ORDER BY is_default DESC, memorial_date ASC
           LIMIT $2"#,
    )
    .bind(group_id)
    .bind(limit)
    .fetch_all(&state.db_pool)
    .await?;

    let result: Vec<MemorialDayOut> = rows
        .into_iter()
        .map(|r| {
            let memorial_date: NaiveDate = r.get("memorial_date");
            let calendar_type: String = r.get("calendar_type");
            let lunar_month: Option<i16> = r.get("lunar_month");
            let lunar_day: Option<i16> = r.get("lunar_day");
            let is_leap_month: bool = r.get("is_leap_month");
            let days_until = days_until_next_occurrence(
                memorial_date,
                &calendar_type,
                lunar_month,
                lunar_day,
                is_leap_month,
                today,
            );
            MemorialDayOut {
                id: r.get("id"),
                group_id: r.get("group_id"),
                name: r.get("name"),
                description: r.get("description"),
                memorial_date,
                calendar_type,
                lunar_month,
                lunar_day,
                is_leap_month,
                is_default: r.get("is_default"),
                days_until,
                created_at: r.get("created_at"),
            }
        })
        .filter(|m| {
            // 如指定了 upcoming_days，仅返回即将到来的
            match upcoming_days {
                Some(d) => m.days_until >= 0 && m.days_until <= d,
                None => true,
            }
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 创建纪念日
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/memorial-days",
    tag = "纪念日 (§24.9)",
    params(("group_id" = i64, Path, description = "组 ID")),
    request_body = CreateMemorialDayInput,
    responses(
        (status = 200, description = "创建成功"),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    body: Json<CreateMemorialDayInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();

    if input.name.is_empty() || input.name.len() > 50 {
        return Err(CustomError::invalid_parameter("纪念日名称 1-50 字符"));
    }
    if input.calendar_type != "SOLAR" && input.calendar_type != "LUNAR" {
        return Err(CustomError::invalid_parameter("calendar_type 必须为 SOLAR 或 LUNAR"));
    }
    if input.calendar_type == "LUNAR" {
        if input.lunar_month.is_none() || input.lunar_day.is_none() {
            return Err(CustomError::invalid_parameter("阴历纪念日必须提供 lunar_month 和 lunar_day"));
        }
    }

    verify_group_member(&state, token.user_id, group_id).await?;

    let row = sqlx::query(
        r#"INSERT INTO memorial_day
            (group_id, name, description, memorial_date, calendar_type, lunar_month, lunar_day, is_leap_month, is_default)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0)
           RETURNING id, group_id, name, description, memorial_date, calendar_type,
                     lunar_month, lunar_day, is_leap_month, is_default, created_at"#,
    )
    .bind(group_id)
    .bind(&input.name)
    .bind(&input.description)
    .bind(input.memorial_date)
    .bind(&input.calendar_type)
    .bind(input.lunar_month)
    .bind(input.lunar_day)
    .bind(input.is_leap_month.unwrap_or(false))
    .fetch_one(&state.db_pool)
    .await?;

    let today = chrono::Local::now().date_naive();
    let result = row_to_memorial_out(row, today);
    Ok(ApiResponse::success(result))
}

/// 获取纪念日详情
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/memorial-days/{id}",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    responses(
        (status = 200, description = "成功"),
        (status = 404, description = "纪念日不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    let row = sqlx::query(
        r#"SELECT id, group_id, name, description, memorial_date, calendar_type,
                  lunar_month, lunar_day, is_leap_month, is_default, created_at
           FROM memorial_day WHERE id = $1 AND group_id = $2"#,
    )
    .bind(id)
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| CustomError::resource_not_found("纪念日不存在"))?;

    let today = chrono::Local::now().date_naive();
    Ok(ApiResponse::success(row_to_memorial_out(row, today)))
}

/// 更新纪念日
#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/memorial-days/{id}",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    request_body = UpdateMemorialDayInput,
    responses(
        (status = 200, description = "更新成功"),
        (status = 404, description = "纪念日不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
    body: Json<UpdateMemorialDayInput>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    let input = body.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    // 动态更新（仅更新有值的字段）
    let mut updates = Vec::new();
    if input.name.is_some() { updates.push("name = $3"); }
    if input.description.is_some() { updates.push("description = $4"); }
    if input.memorial_date.is_some() { updates.push("memorial_date = $5"); }
    if input.calendar_type.is_some() { updates.push("calendar_type = $6"); }
    if input.lunar_month.is_some() { updates.push("lunar_month = $7"); }
    if input.lunar_day.is_some() { updates.push("lunar_day = $8"); }
    if input.is_leap_month.is_some() { updates.push("is_leap_month = $9"); }

    if updates.is_empty() {
        return Err(CustomError::invalid_parameter("至少更新一个字段"));
    }

    let sql = format!(
        "UPDATE memorial_day SET {}, updated_at = NOW() WHERE id = $1 AND group_id = $2 RETURNING id, group_id, name, description, memorial_date, calendar_type, lunar_month, lunar_day, is_leap_month, is_default, created_at",
        updates.join(", ")
    );

    let mut q = sqlx::query(&sql)
        .bind(id)
        .bind(group_id);
    if let Some(v) = &input.name { q = q.bind(v); } else { q = q.bind(Option::<String>::None); }
    if let Some(v) = &input.description { q = q.bind(v); } else { q = q.bind(Option::<String>::None); }
    if let Some(v) = input.memorial_date { q = q.bind(v); } else { q = q.bind(Option::<NaiveDate>::None); }
    if let Some(v) = &input.calendar_type { q = q.bind(v); } else { q = q.bind(Option::<String>::None); }
    if let Some(v) = input.lunar_month { q = q.bind(v); } else { q = q.bind(Option::<i16>::None); }
    if let Some(v) = input.lunar_day { q = q.bind(v); } else { q = q.bind(Option::<i16>::None); }
    if let Some(v) = input.is_leap_month { q = q.bind(v); } else { q = q.bind(bool::default()); }

    let row = q.fetch_optional(&state.db_pool).await?
        .ok_or_else(|| CustomError::resource_not_found("纪念日不存在"))?;
    let today = chrono::Local::now().date_naive();
    Ok(ApiResponse::success(row_to_memorial_out(row, today)))
}

/// 删除纪念日
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/memorial-days/{id}",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 404, description = "纪念日不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    let rows_affected = sqlx::query("DELETE FROM memorial_day WHERE id = $1 AND group_id = $2")
        .bind(id)
        .bind(group_id)
        .execute(&state.db_pool)
        .await?
        .rows_affected();

    if rows_affected == 0 {
        return Err(CustomError::resource_not_found("纪念日不存在"));
    }
    Ok(ApiResponse::success(serde_json::json!({ "deleted": true })))
}

/// 置顶纪念日
/// POST /api/groups/{group_id}/memorial-days/{id}/pin
///
/// 行为：
/// - 事务里先清掉同组之前的置顶,再设新置顶（保证"一组同时只能 1 条"）
/// - 不存在 / 跨组 → 404
/// - 非组成员 → 403
/// - 成功 → 200 + { pinnedId, pinnedAt }
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/memorial-days/{id}/pin",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    responses(
        (status = 200, description = "置顶成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 404, description = "纪念日不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn pin_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    let mut tx = state.db_pool.begin().await?;

    // 1) 清掉同组之前的置顶
    sqlx::query("UPDATE memorial_day SET is_default = 0 WHERE group_id = $1 AND is_default = 1")
        .bind(group_id)
        .execute(&mut *tx)
        .await?;

    // 2) 设新置顶 (同时刷新 updated_at,作为 pinnedAt 返回)
    let updated = sqlx::query(
        "UPDATE memorial_day SET is_default = 1, updated_at = NOW() WHERE id = $1 AND group_id = $2",
    )
    .bind(id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        // 回滚 + 404
        tx.rollback().await?;
        return Err(CustomError::resource_not_found("纪念日不存在"));
    }

    // 3) 取回置顶时间（updated_at）作为 pinnedAt 返回
    let row: (chrono::DateTime<chrono::Utc>,) = sqlx::query_as(
        "SELECT updated_at FROM memorial_day WHERE id = $1 AND group_id = $2",
    )
    .bind(id)
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ApiResponse::success(PinResponse {
        pinned_id: Some(id),
        pinned_at: Some(row.0.to_rfc3339()),
    }))
}

/// 取消置顶纪念日
/// DELETE /api/groups/{group_id}/memorial-days/{id}/pin
///
/// 行为：
/// - 把指定纪念日的 is_default 设为 0
/// - 不存在 / 跨组 / 本来就未置顶 → 一律返回 200（幂等，不泄露旁路信息）
/// - 非组成员 → 403
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/memorial-days/{id}/pin",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("id" = i64, Path, description = "纪念日 ID")
    ),
    responses(
        (status = 200, description = "取消置顶成功（幂等）"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员")
    ),
    security(("bearer_auth" = []))
)]
pub async fn unpin_memorial_day(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (group_id, id) = path.into_inner();
    verify_group_member(&state, token.user_id, group_id).await?;

    // 幂等：不管该条纪念日存不存在、是 0 还是 1，都直接 UPDATE
    let _ = sqlx::query(
        "UPDATE memorial_day SET is_default = 0 WHERE id = $1 AND group_id = $2 AND is_default = 1",
    )
    .bind(id)
    .bind(group_id)
    .execute(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(UnpinResponse { pinned_id: None }))
}

/// 即将到来的纪念日列表
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/memorial-days/upcoming",
    tag = "纪念日 (§24.9)",
    params(
        ("group_id" = i64, Path, description = "组 ID"),
        ("days" = Option<i64>, Query, description = "未来 N 天内，默认 30")
    ),
    responses((status = 200, description = "成功")),
    security(("bearer_auth" = []))
)]
pub async fn upcoming_memorial_days(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<i64>,
    query: Query<UpcomingQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let days = query.days.unwrap_or(30);
    verify_group_member(&state, token.user_id, group_id).await?;

    let today = chrono::Local::now().date_naive();
    let rows = sqlx::query(
        r#"SELECT id, group_id, name, description, memorial_date, calendar_type,
                  lunar_month, lunar_day, is_leap_month, is_default, created_at
           FROM memorial_day
           WHERE group_id = $1
           ORDER BY is_default DESC, memorial_date ASC"#,
    )
    .bind(group_id)
    .fetch_all(&state.db_pool)
    .await?;

    let upcoming: Vec<MemorialDayOut> = rows
        .into_iter()
        .map(|r| row_to_memorial_out(r, today))
        .filter(|m| m.days_until >= 0 && m.days_until <= days)
        .collect();

    Ok(ApiResponse::success(upcoming))
}

// ========== 辅助函数 ==========

fn row_to_memorial_out(row: sqlx::postgres::PgRow, today: NaiveDate) -> MemorialDayOut {
    let memorial_date: NaiveDate = row.get("memorial_date");
    let calendar_type: String = row.get("calendar_type");
    let lunar_month: Option<i16> = row.get("lunar_month");
    let lunar_day: Option<i16> = row.get("lunar_day");
    let is_leap_month: bool = row.get("is_leap_month");
    let days_until = days_until_next_occurrence(
        memorial_date,
        &calendar_type,
        lunar_month,
        lunar_day,
        is_leap_month,
        today,
    );
    MemorialDayOut {
        id: row.get("id"),
        group_id: row.get("group_id"),
        name: row.get("name"),
        description: row.get("description"),
        memorial_date,
        calendar_type,
        lunar_month,
        lunar_day,
        is_leap_month,
        is_default: row.get("is_default"),
        days_until,
        created_at: row.get("created_at"),
    }
}

async fn verify_group_member(state: &Arc<AppState>, user_id: i64, group_id: i64) -> Result<(), CustomError> {
    let member: Option<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM association_group_members WHERE user_id = $1 AND group_id = $2 AND member_status = 'ACTIVE'"
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await?;
    if member.is_none() {
        return Err(CustomError::permission_denied("不是该组成员"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_response_serializes_to_camel_case() {
        let resp = PinResponse {
            pinned_id: Some(100),
            pinned_at: Some("2026-06-19T10:30:00Z".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            json,
            r#"{"pinnedId":100,"pinnedAt":"2026-06-19T10:30:00Z"}"#
        );
    }

    #[test]
    fn unpin_response_serializes_to_null_pinned_id() {
        let resp = UnpinResponse { pinned_id: None };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"pinnedId":null}"#);
    }

    #[test]
    fn pin_response_optional_pinned_at() {
        let resp = PinResponse { pinned_id: None, pinned_at: None };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"pinnedId":null,"pinnedAt":null}"#);
    }
}
