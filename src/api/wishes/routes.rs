// API 层 - 心愿路由
// FSD.latest.md compliant - 心愿创建、协商、选择、履约

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::{types::Json as SqlxJson, Row};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::wish_service::WishService;
use crate::domain::wish::{
    WishCreateInput, WishDeadlineInput, WishFeedbackInput, WishOut, WishQuoteInput,
    WishRejectInput, WishStatus,
};
use crate::{
    config::AppState,
    errors::CustomError,
    middlewares::auth::UserToken,
    middlewares::idempotency::{self, IdempotencyKey, ReservationOutcome},
    middlewares::require_group::RequireGroup,
    middlewares::target_group::require_active_target_group_member,
    utils::response::ApiResponse,
};

/// 配置心愿路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 不用 web::scope —— 避免圈住路径
    // 组内心愿
    cfg.service(
        web::resource("/api/groups/{group_id}/wishes")
            .route(web::post().to(create_group_wish)) // FSD v2 7.1 创建心愿
            .route(web::get().to(list_group_wishes)), // FSD v2 7.2 获取心愿列表
    );
    // 全局心愿详情/操作
    cfg.service(
        web::resource("/api/wishes/{wish_id}").route(web::get().to(get_wish)), // FSD v2 7.3 获取心愿详情
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/quote").route(web::post().to(wish_quote)), // FSD v2 7.4 协商报价
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/deadline").route(web::post().to(wish_deadline)), // FSD v2 7.5 协商履约期限
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/confirm-agreement")
            .route(web::post().to(wish_confirm_agreement)), // FSD v2 7.6 双方确认
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/reject").route(web::post().to(wish_reject)), // FSD v2 7.7 拒绝/关闭
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/select").route(web::post().to(wish_select)), // FSD v2 7.8 选择心愿
    );
    cfg.service(web::resource("/api/wishes/{wish_id}/release").route(web::post().to(wish_release)));
    cfg.service(
        web::resource("/api/wishes/{wish_id}/feedback").route(web::post().to(submit_feedback)), // FSD v2 7.9 提交打卡反馈
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/expire").route(web::post().to(wish_expire)), // FSD v2 7.10 逾期处理
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/close").route(web::post().to(wish_close)), // FSD v2 7.11 关闭心愿
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/confirm-completion")
            .route(web::post().to(wish_confirm_completion)), // FSD v2 7.13 接单人确认完成
    );
}

// ========== 请求/响应结构 ==========

/// 心愿列表查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct WishListQuery {
    pub cursor: Option<String>,
    pub limit: Option<i32>,
    pub status: Option<String>,
    pub role: Option<String>,
    /// "mine" = 只返回当前用户创建或被指定为履约人的心愿；其他值或缺失 = 全组
    #[serde(default)]
    pub scope: Option<String>,
}

/// 心愿关闭输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WishCloseInput {
    pub reason: Option<String>,
}

// ========== 处理器 ==========

/// 创建心愿
/// POST /api/groups/{group_id}/wishes
/// FSD v2 7.1
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/wishes",
    tag = "心愿",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = WishCreateInput,
    responses(
        (status = 201, description = "创建成功", body = WishOut),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_group_wish(
    user_token: UserToken,
    _require: RequireGroup,
    idempotency_key: IdempotencyKey,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    data: Json<WishCreateInput>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    require_active_target_group_member(db, user_token.user_id, gid).await?;

    // 校验: 组内至少 2 人才能创建心愿
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM association_group_members \
         WHERE group_id = $1 AND member_status = 'ACTIVE'::group_member_status_enum",
    )
    .bind(gid)
    .fetch_one(db)
    .await?;

    if member_count < 2 {
        return Err(CustomError::BadRequest(
            "组内需要至少 2 人才能创建心愿".into(),
        ));
    }

    let route = format!("/api/groups/{gid}/wishes");
    let reservation = match idempotency::reserve(
        &state.redis_cache,
        user_token.user_id,
        "POST",
        &route,
        idempotency_key.0.as_deref(),
    )
    .await?
    {
        ReservationOutcome::Bypass => None,
        ReservationOutcome::Acquired(reservation) => Some(reservation),
        ReservationOutcome::Completed(cached) => {
            return Ok(ApiResponse::success(cached.body));
        }
    };

    let input = data.into_inner();
    let rec = match WishService::create_wish(db, user_token.user_id, gid, &input).await {
        Ok(rec) => rec,
        Err(error) => {
            if let Some(reservation) = reservation.as_ref() {
                reservation.release().await;
            }
            return Err(error);
        }
    };
    let payload = serde_json::to_value(WishOut::from_record(rec, None))
        .map_err(|e| CustomError::internal(format!("心愿响应序列化失败: {e}")))?;
    if let Some(reservation) = reservation.as_ref() {
        reservation.complete(200, &payload).await?;
    }
    Ok(ApiResponse::success(payload))
}

/// 获取组内心愿列表
/// GET /api/groups/{group_id}/wishes
/// FSD v2 7.2
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/wishes",
    tag = "心愿",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        WishListQuery
    ),
    responses(
        (status = 200, description = "获取成功", body = crate::domain::wish::entities::CursorPageWishList),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_group_wishes(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<WishListQuery>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;
    let limit = query.limit.unwrap_or(20).min(100);

    require_active_target_group_member(db, user_token.user_id, gid).await?;
    WishService::auto_process_overdue_group_wishes(db, gid).await?;

    // 参数化查询:状态/角色/游标全部使用占位符 + 枚举白名单
    let status_filter: Option<&str> = match query.status.as_deref() {
        Some(s)
            if matches!(
                s,
                "NEGOTIATING" | "CREATED" | "CLAIMED" | "FINISHED" | "EXPIRED" | "CLOSED"
            ) =>
        {
            Some(s)
        }
        Some(_) => return Err(CustomError::BadRequest("status 非法".into())),
        None => None,
    };

    // role 转换为对当前 user_id 的过滤条件(只用枚举白名单)
    let role_filter: Option<&str> = match query.role.as_deref() {
        Some("REQUESTER") | Some("FULFILLER") => query.role.as_deref(),
        Some(_) => return Err(CustomError::BadRequest("role 非法".into())),
        None => None,
    };

    // scope 校验:只接受 "mine"(只看当前用户相关);其他值或缺失 = 全组
    let scope_filter: Option<&str> = match query.scope.as_deref() {
        Some("mine") => Some("mine"),
        Some(_) => return Err(CustomError::BadRequest("scope 非法".into())),
        None => None,
    };

    // cursor 校验:必须是 RFC3339 时间戳格式,否则拒绝
    let cursor_ts: Option<chrono::DateTime<chrono::Utc>> = match query.cursor.as_deref() {
        Some(c) => Some(
            chrono::DateTime::parse_from_rfc3339(c)
                .map_err(|_| CustomError::BadRequest("cursor 必须是 RFC3339 时间戳".into()))?
                .with_timezone(&chrono::Utc),
        ),
        None => None,
    };

    // 组合 16 种查询分支(2 scope × 2 状态 × 2 角色 × 2 游标),全部用参数化
    // scope=mine 时附加 AND (created_by=$? OR fulfiller_id=$?) 过滤
    let rows = match (scope_filter, status_filter, role_filter, cursor_ts) {
        (Some(_sc), Some(s), Some(r), Some(c)) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.status = $2::wish_status_enum AND w.{} = $3
                     AND (w.created_by = $4 OR w.fulfiller_id = $4)
                     AND w.created_at < $5
                   ORDER BY w.created_at DESC
                   LIMIT $6"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(s)
                .bind(user_token.user_id)
                .bind(user_token.user_id)
                .bind(c)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (Some(_sc), Some(s), Some(r), None) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.status = $2::wish_status_enum AND w.{} = $3
                     AND (w.created_by = $4 OR w.fulfiller_id = $4)
                   ORDER BY w.created_at DESC
                   LIMIT $5"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(s)
                .bind(user_token.user_id)
                .bind(user_token.user_id)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (Some(_sc), Some(s), None, Some(c)) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1 AND w.status = $2::wish_status_enum
                 AND (w.created_by = $3 OR w.fulfiller_id = $3)
                 AND w.created_at < $4
               ORDER BY w.created_at DESC
               LIMIT $5"#,
            )
            .bind(gid)
            .bind(s)
            .bind(user_token.user_id)
            .bind(c)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        (Some(_sc), Some(s), None, None) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1 AND w.status = $2::wish_status_enum
                 AND (w.created_by = $3 OR w.fulfiller_id = $3)
               ORDER BY w.created_at DESC
               LIMIT $4"#,
            )
            .bind(gid)
            .bind(s)
            .bind(user_token.user_id)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        (Some(_sc), None, Some(r), Some(c)) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.{} = $2
                     AND (w.created_by = $3 OR w.fulfiller_id = $3)
                     AND w.created_at < $4
                   ORDER BY w.created_at DESC
                   LIMIT $5"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(user_token.user_id)
                .bind(user_token.user_id)
                .bind(c)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (Some(_sc), None, Some(r), None) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.{} = $2
                     AND (w.created_by = $3 OR w.fulfiller_id = $3)
                   ORDER BY w.created_at DESC
                   LIMIT $4"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(user_token.user_id)
                .bind(user_token.user_id)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (Some(_sc), None, None, Some(c)) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1
                 AND (w.created_by = $2 OR w.fulfiller_id = $2)
                 AND w.created_at < $3
               ORDER BY w.created_at DESC
               LIMIT $4"#,
            )
            .bind(gid)
            .bind(user_token.user_id)
            .bind(c)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        (Some(_sc), None, None, None) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1
                 AND (w.created_by = $2 OR w.fulfiller_id = $2)
               ORDER BY w.created_at DESC
               LIMIT $3"#,
            )
            .bind(gid)
            .bind(user_token.user_id)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        // ========== scope 为空(全组)的原 8 个分支 ==========
        (None, Some(s), Some(r), Some(c)) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.status = $2::wish_status_enum AND w.{} = $3 AND w.created_at < $4
                   ORDER BY w.created_at DESC
                   LIMIT $5"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(s)
                .bind(user_token.user_id)
                .bind(c)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (None, Some(s), Some(r), None) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.status = $2::wish_status_enum AND w.{} = $3
                   ORDER BY w.created_at DESC
                   LIMIT $4"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(s)
                .bind(user_token.user_id)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (None, Some(s), None, Some(c)) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1 AND w.status = $2::wish_status_enum AND w.created_at < $3
               ORDER BY w.created_at DESC
               LIMIT $4"#,
            )
            .bind(gid)
            .bind(s)
            .bind(c)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        (None, Some(s), None, None) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1 AND w.status = $2::wish_status_enum
               ORDER BY w.created_at DESC
               LIMIT $3"#,
            )
            .bind(gid)
            .bind(s)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        (None, None, Some(r), Some(c)) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
                   LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.{} = $2 AND w.created_at < $3
                   ORDER BY w.created_at DESC
                   LIMIT $4"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(user_token.user_id)
                .bind(c)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (None, None, Some(r), None) => {
            let col = if r == "REQUESTER" {
                "requester_id"
            } else {
                "fulfiller_id"
            };
            let sql = format!(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                          w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                          w.fulfillment_due_at, w.created_at,
                          u1.nick_name as requester_nickname,
                          u2.nick_name as fulfiller_nickname
                   FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
                   LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
                   WHERE w.group_id = $1 AND w.{} = $2
                   ORDER BY w.created_at DESC
                   LIMIT $3"#,
                col
            );
            sqlx::query(&sql)
                .bind(gid)
                .bind(user_token.user_id)
                .bind(limit + 1)
                .fetch_all(db)
                .await?
        }
        (None, None, None, Some(c)) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1 AND w.created_at < $2
               ORDER BY w.created_at DESC
               LIMIT $3"#,
            )
            .bind(gid)
            .bind(c)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
        (None, None, None, None) => {
            sqlx::query(
                r#"SELECT w.wish_id, w.wish_name, w.wish_cost, w.fulfillment_deadline_hours,
                      w.status::text AS status, w.requester_id, w.fulfiller_id, w.created_by, w.selected_by, w.selected_at,
                      w.fulfillment_due_at, w.created_at,
                      u1.nick_name as requester_nickname,
                      u2.nick_name as fulfiller_nickname
               FROM wishes w
               LEFT JOIN users u1 ON u1.user_id = w.requester_id
               LEFT JOIN users u2 ON u2.user_id = w.fulfiller_id
               WHERE w.group_id = $1
               ORDER BY w.created_at DESC
               LIMIT $2"#,
            )
            .bind(gid)
            .bind(limit + 1)
            .fetch_all(db)
            .await?
        }
    };

    let has_more = rows.len() > limit as usize;
    let visible_ids: Vec<i64> = rows
        .iter()
        .take(limit as usize)
        .map(|row| row.get::<i64, _>("wish_id"))
        .collect();
    let mut feedback_map: std::collections::HashMap<
        i64,
        Vec<crate::domain::wish::entities::WishFeedbackOut>,
    > = std::collections::HashMap::new();
    if !visible_ids.is_empty() {
        let feedback_rows = sqlx::query(
            "SELECT feedback_id, wish_id, user_id, role_snapshot, content, images, created_at, updated_at \
             FROM wish_feedbacks WHERE wish_id = ANY($1) ORDER BY created_at ASC",
        )
        .bind(&visible_ids)
        .fetch_all(db)
        .await?;
        for feedback_row in feedback_rows {
            let wish_id: i64 = feedback_row.get("wish_id");
            feedback_map.entry(wish_id).or_default().push(
                crate::domain::wish::entities::WishFeedbackOut {
                    feedback_id: feedback_row.get("feedback_id"),
                    user_id: feedback_row.get("user_id"),
                    role: feedback_row.try_get("role_snapshot").ok(),
                    content: feedback_row.try_get("content").ok(),
                    images: feedback_row
                        .try_get::<Option<SqlxJson<Vec<String>>>, _>("images")
                        .ok()
                        .flatten()
                        .map(|images| images.0),
                    created_at: feedback_row.get("created_at"),
                    updated_at: feedback_row.get("updated_at"),
                },
            );
        }
    }

    let wishes_list: Vec<crate::domain::wish::entities::WishOut> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            // wish_cost 始终非 NULL（创建时必填,CREATED 后被 confirm_agreement 同步成 final_cost）
            let wish_cost: i32 = r.get::<i32, _>("wish_cost");
            let status_str: String = r.get::<String, _>("status");
            let status = match status_str.as_str() {
                "NEGOTIATING" => WishStatus::Negotiating,
                "CREATED" => WishStatus::Created,
                "CLAIMED" => WishStatus::Claimed,
                "FINISHED" => WishStatus::Finished,
                "EXPIRED" => WishStatus::Expired,
                "CLOSED" => WishStatus::Closed,
                _ => WishStatus::Created,
            };
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            let wish_id = r.get::<i64, _>("wish_id");
            let feedbacks = feedback_map.remove(&wish_id).unwrap_or_default();
            let fulfilled_at = feedbacks
                .iter()
                .find(|item| item.role.as_deref() == Some("FULFILLER"))
                .map(|item| item.created_at);
            crate::domain::wish::entities::WishOut {
                wish_id,
                wish_name: r.get::<String, _>("wish_name"),
                wish_cost,
                status,
                created_by: r.get::<i64, _>("created_by"),
                group_id: gid,
                claimed_by: r.try_get::<Option<i64>, _>("selected_by").ok().flatten(),
                claimed_at: r
                    .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("selected_at")
                    .ok()
                    .flatten(),
                claim_cost: if status == WishStatus::Claimed {
                    Some(r.get::<i32, _>("wish_cost"))
                } else {
                    None
                },
                created_at,
                updated_at: created_at,
                feedback: feedbacks.first().cloned(),
                feedbacks,
                fulfilled_at,
                creator_checkin_due_at: None,
                auto_completed_at: None,
                requester_id: r.get::<Option<i64>, _>("requester_id"),
                fulfiller_id: r.get::<Option<i64>, _>("fulfiller_id"),
                negotiation_status: None,
            }
        })
        .collect();

    let next_cursor = if has_more {
        rows.last().map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            created_at.to_rfc3339()
        })
    } else {
        None
    };

    // ========== 聚合统计 ==========
    // counts 是顶部 chip 用的总览数字,**不带 status 过滤** —— 用户切 tab 时
    // 列表会变,但 chip 上的统计数字始终反映"全组我相关的所有心愿"分布,
    // 否则每切一次 tab 所有数字都跟着跳,看着很乱
    //
    // 仍然受 scope (mine) 和 role (REQUESTER/FULFILLER) 影响:这些是用户身份维度
    let mut counts_sql = String::from(
        "SELECT \
             COUNT(*)::BIGINT AS total, \
             COUNT(*) FILTER (WHERE w.status = 'NEGOTIATING'::wish_status_enum)::BIGINT AS negotiating, \
             COUNT(*) FILTER (WHERE w.status = 'CREATED'::wish_status_enum)::BIGINT AS unlocked, \
             COUNT(*) FILTER (WHERE w.status = 'CLAIMED'::wish_status_enum)::BIGINT AS claimed, \
             COUNT(*) FILTER (WHERE w.status = 'FINISHED'::wish_status_enum)::BIGINT AS finished, \
             COUNT(*) FILTER (WHERE w.status IN ('EXPIRED'::wish_status_enum,'CLOSED'::wish_status_enum))::BIGINT AS closed \
         FROM wishes w \
         WHERE w.group_id = $1",
    );
    let mut counts_query = sqlx::query(&counts_sql).bind(gid);
    if scope_filter.is_some() {
        // (created_by = $X OR fulfiller_id = $X) —— 用同一个占位符两次
        counts_sql.push_str(&format!(" AND (w.created_by = $2 OR w.fulfiller_id = $2)"));
        counts_query = sqlx::query(&counts_sql).bind(gid).bind(user_token.user_id);
    }
    if let Some(r) = role_filter {
        let col = if r == "REQUESTER" {
            "requester_id"
        } else {
            "fulfiller_id"
        };
        // 继续累加占位符编号,scope=mine 时是 $2,这里再 +1 = $3
        let next_idx = if scope_filter.is_some() { 3 } else { 2 };
        counts_sql.push_str(&format!(" AND w.{} = ${}", col, next_idx));
        counts_query = if scope_filter.is_some() {
            sqlx::query(&counts_sql)
                .bind(gid)
                .bind(user_token.user_id)
                .bind(user_token.user_id)
        } else {
            sqlx::query(&counts_sql).bind(gid).bind(user_token.user_id)
        };
    }
    let counts_row = counts_query.fetch_one(db).await?;

    let counts = crate::domain::wish::entities::WishStatusCounts {
        negotiating: counts_row.get::<i64, _>("negotiating"),
        unlocked: counts_row.get::<i64, _>("unlocked"),
        claimed: counts_row.get::<i64, _>("claimed"),
        finished: counts_row.get::<i64, _>("finished"),
        closed: counts_row.get::<i64, _>("closed"),
    };
    let total: i64 = counts_row.get::<i64, _>("total");

    Ok(ApiResponse::success(
        crate::domain::wish::entities::CursorPageWishList {
            wishes: wishes_list,
            next_cursor,
            has_more,
            total,
            counts,
        },
    ))
}

/// 获取心愿详情
/// GET /api/wishes/{wish_id}
/// FSD v2 7.3
#[utoipa::path(
    get,
    path = "/api/wishes/{wish_id}",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "获取成功", body = crate::domain::wish::entities::WishOutWithNegotiations),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let (rec, negotiations, feedbacks) =
        WishService::get_wish(&state.db_pool, user_token.user_id, *id).await?;
    let wish_out = WishOut::from_record_with_feedbacks(
        rec,
        feedbacks
            .into_iter()
            .map(crate::domain::wish::entities::WishFeedbackOut::from)
            .collect(),
    );
    Ok(ApiResponse::success(
        crate::domain::wish::entities::WishOutWithNegotiations {
            wish: wish_out,
            negotiations,
        },
    ))
}

/// 协商报价
/// POST /api/wishes/{wish_id}/quote
/// FSD v2 7.4
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/quote",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishQuoteInput,
    responses(
        (status = 200, description = "报价成功"),
        (status = 400, description = "心愿状态不允许报价"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_quote(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishQuoteInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out = WishService::quote_wish(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 协商履约期限
/// POST /api/wishes/{wish_id}/deadline
/// FSD v2 7.5
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/deadline",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishDeadlineInput,
    responses(
        (status = 200, description = "设置成功"),
        (status = 400, description = "心愿状态不允许设置期限"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_deadline(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishDeadlineInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out =
        WishService::set_deadline(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 双方线上确认积分和期限
/// POST /api/wishes/{wish_id}/confirm-agreement
/// FSD v2 7.6
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/confirm-agreement",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "确认成功；双方同意时冻结积分并进入心愿池", body = WishOut),
        (status = 400, description = "心愿状态不允许确认"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_confirm_agreement(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out: crate::domain::wish::entities::WishOut =
        WishService::confirm_agreement(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(ApiResponse::success(out))
}

/// 拒绝或关闭协商
/// POST /api/wishes/{wish_id}/reject
/// FSD v2 7.7
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/reject",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishRejectInput,
    responses(
        (status = 200, description = "操作成功"),
        (status = 400, description = "心愿状态不允许此操作"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_reject(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishRejectInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let caller_id = user_token.user_id;
    let out = WishService::reject_wish(&state.db_pool, caller_id, wish_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 履约人领取心愿（积分已在进入心愿池时冻结）
/// POST /api/wishes/{wish_id}/select
/// FSD v2 7.8
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/select",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "领取成功", body = WishOut),
        (status = 400, description = "已有履约中的心愿或状态不允许"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_select(
    user_token: UserToken,
    _require: RequireGroup,
    idempotency_key: IdempotencyKey,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let route = format!("/api/wishes/{wish_id}/select");
    let reservation = match idempotency::reserve(
        &state.redis_cache,
        user_token.user_id,
        "POST",
        &route,
        idempotency_key.0.as_deref(),
    )
    .await?
    {
        ReservationOutcome::Bypass => None,
        ReservationOutcome::Acquired(reservation) => Some(reservation),
        ReservationOutcome::Completed(cached) => {
            return Ok(ApiResponse::success(cached.body));
        }
    };

    let out = match WishService::select_wish(&state.db_pool, user_token.user_id, wish_id).await {
        Ok(out) => out,
        Err(error) => {
            if let Some(reservation) = reservation.as_ref() {
                reservation.release().await;
            }
            return Err(error);
        }
    };
    let payload = serde_json::to_value(out)
        .map_err(|e| CustomError::internal(format!("心愿响应序列化失败: {e}")))?;
    if let Some(reservation) = reservation.as_ref() {
        reservation.complete(200, &payload).await?;
    }
    Ok(ApiResponse::success(payload))
}

/// 履约人在未打卡前放弃领取，心愿重新回到心愿池
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/release",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "已放弃领取，心愿回到心愿池", body = WishOut),
        (status = 400, description = "已有打卡或状态不允许"),
        (status = 403, description = "只有当前履约人可以操作"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_release(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let rec = WishService::release_claim(&state.db_pool, user_token.user_id, *id).await?;
    Ok(ApiResponse::success(WishOut::from_record(rec, None)))
}

/// 兼容旧客户端：新流程由双方打卡自动完成
/// POST /api/wishes/{wish_id}/confirm-completion
/// FSD v2 7.12
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/confirm-completion",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "心愿已完成时幂等返回", body = WishOut),
        (status = 400, description = "请通过双方打卡完成心愿"),
        (status = 403, description = "非心愿参与方"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_confirm_completion(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out =
        WishService::confirm_wish_completion(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(ApiResponse::success(out))
}

/// 提交或编辑自己的打卡反馈；双方都打卡后自动完成
/// POST /api/wishes/{wish_id}/feedback
/// FSD v2 7.9
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/feedback",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishFeedbackInput,
    responses(
        (status = 200, description = "提交成功；双方完成时状态自动变为 FINISHED", body = WishOut),
        (status = 400, description = "心愿状态不允许"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn submit_feedback(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    data: Json<WishFeedbackInput>,
) -> Result<impl Responder, CustomError> {
    let (rec, feedbacks) =
        WishService::submit_feedback(&state.db_pool, user_token.user_id, *id, &data.into_inner())
            .await?;
    Ok(ApiResponse::success(WishOut::from_record_with_feedbacks(
        rec,
        feedbacks
            .into_iter()
            .map(crate::domain::wish::entities::WishFeedbackOut::from)
            .collect(),
    )))
}

/// 创建人关闭尚未领取的心愿并解冻积分
/// POST /api/wishes/{wish_id}/close
/// FSD v2 7.11
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/close",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishCloseInput,
    responses(
        (status = 200, description = "关闭成功，冻结积分已退回"),
        (status = 400, description = "心愿状态不允许关闭"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_close(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishCloseInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out = WishService::close_wish(
        &state.db_pool,
        user_token.user_id,
        wish_id,
        &WishRejectInput {
            reason: input.reason,
        },
    )
    .await?;
    Ok(ApiResponse::success(out))
}

/// 心愿履约逾期处理
/// POST /api/wishes/{wish_id}/expire
/// FSD v2 7.10
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/expire",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "处理成功"),
        (status = 400, description = "心愿状态不允许逾期处理"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_expire(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let result = WishService::expire_wish(&state.db_pool, user_token.user_id, wish_id).await?;

    if result.already_processed {
        return Ok(ApiResponse::success(serde_json::json!({
            "wishId": wish_id,
            "status": "EXPIRED",
            "frozenAmountUnfrozen": 0,
            "alreadyProcessed": true
        })));
    }

    Ok(ApiResponse::success(serde_json::json!({
        "wishId": wish_id,
        "status": "EXPIRED",
        "frozenAmountUnfrozen": result.unfrozen_amount
    })))
}
