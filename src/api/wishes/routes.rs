// API 层 - 心愿路由
// FSD.latest.md compliant - 心愿创建、协商、选择、履约

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::wish_service::WishService;
use crate::domain::wish::{
    WishCreateInput, WishDeadlineInput, WishFeedbackInput, WishOut, WishQuoteInput, WishRejectInput,
    WishStatus,
};
use crate::{
    config::AppState, errors::CustomError, middlewares::auth::UserToken,
    middlewares::require_group::RequireGroup, utils::response::ApiResponse,
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
        web::resource("/api/wishes/pending-fulfillment").route(web::get().to(pending_fulfillment)),
    );
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
        web::resource("/api/wishes/{wish_id}/checkins").route(web::get().to(get_wish_checkins)), // FSD v2 7.12 获取打卡记录
    );
    cfg.service(
        web::resource("/api/wishes/{wish_id}/confirm-completion").route(web::post().to(wish_confirm_completion)), // FSD v2 7.13 接单人确认完成
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

/// 待履约心愿查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PendingFulfillmentQuery {
    pub group_id: Option<i64>,
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
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    data: Json<WishCreateInput>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

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

    let rec = WishService::create_wish(db, user_token.user_id, &data.into_inner()).await?;
    Ok(ApiResponse::success(WishOut::from_record(rec, None)))
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

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

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
            crate::domain::wish::entities::WishOut {
                wish_id: r.get::<i64, _>("wish_id"),
                wish_name: r.get::<String, _>("wish_name"),
                wish_cost,
                status,
                created_by: r.get::<i64, _>("created_by"),
                group_id: gid,
                claimed_by: None,
                claimed_at: None,
                claim_cost: None,
                created_at,
                updated_at: created_at,
                feedback: None,
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
    _user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let (rec, negotiations) = WishService::get_wish(&state.db_pool, *id).await?;
    let wish_out = WishOut::from_record(rec, None);
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
        (status = 200, description = "确认成功，心愿进入心愿池"),
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
    let out: crate::domain::wish::entities::WishOut = WishService::confirm_agreement(&state.db_pool, user_token.user_id, wish_id).await?;
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
    // P3-1: 入口处先校验调用方是协商双方之一,避免 service 写权限前的无效调用
    let caller_id = user_token.user_id;
    let party: Option<(i64, i64)> = sqlx::query_as(
        "SELECT requester_id, fulfiller_id FROM wishes WHERE wish_id = $1"
    )
    .bind(wish_id)
    .fetch_optional(&state.db_pool)
    .await?;
    if let Some((req, ful)) = party {
        if caller_id != req && caller_id != ful {
            return Err(CustomError::Forbidden(
                "只有心愿协商双方可以拒绝".into(),
            ));
        }
    }
    let out = WishService::reject_wish(&state.db_pool, caller_id, wish_id, &input, "REJECT").await?;
    Ok(ApiResponse::success(out))
}

/// 选择心愿并冻结积分
/// POST /api/wishes/{wish_id}/select
/// FSD v2 7.8
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/select",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "选择成功，积分已冻结"),
        (status = 400, description = "积分不足或心愿状态不允许"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn wish_select(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out = WishService::select_wish(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(ApiResponse::success(out))
}

/// 接单人确认履约完成 — 心愿从 CLAIMED 推到 FINISHED
/// POST /api/wishes/{wish_id}/confirm-completion
/// FSD v2 7.12
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/confirm-completion",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "确认完成", body = WishOut),
        (status = 400, description = "状态不允许或履约人未提交打卡"),
        (status = 403, description = "只有接单人可以确认完成"),
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

/// 提交打卡反馈
/// POST /api/wishes/{wish_id}/feedback
/// FSD v2 7.9
#[utoipa::path(
    put,
    path = "/api/wishes/{wish_id}/feedback",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishFeedbackInput,
    responses(
        (status = 200, description = "提交成功", body = WishOut),
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
    let (rec, feedback) =
        WishService::submit_feedback(&state.db_pool, user_token.user_id, *id, &data.into_inner())
            .await?;
    Ok(ApiResponse::success(WishOut::from_record(rec, feedback)))
}

/// 关闭心愿（双方协商一致）
/// POST /api/wishes/{wish_id}/close
/// FSD v2 7.11
#[utoipa::path(
    post,
    path = "/api/wishes/{wish_id}/close",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    request_body = WishCloseInput,
    responses(
        (status = 200, description = "关闭成功"),
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
    let out = WishService::reject_wish(
        &state.db_pool,
        user_token.user_id,
        wish_id,
        &WishRejectInput {
            reason: input.reason,
        },
        "CLOSE",  // P3-4: /close 走 CLOSE 路径,允许任意非终态并自动解冻
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
    let db = &state.db_pool;

    // P2-2: 加载心愿并校验权限 - 只有 requester/fulfiller 可以触发逾期
    let row =
        sqlx::query("SELECT status::text, requester_id, fulfiller_id, group_id FROM wishes WHERE wish_id = $1")
            .bind(wish_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

    let status: String = row.get("status");
    let requester_id: i64 = row.get("requester_id");
    let fulfiller_id: i64 = row.get("fulfiller_id");
    let group_id: i64 = row.get("group_id");

    let caller_id = user_token.user_id;
    if caller_id != requester_id && caller_id != fulfiller_id {
        return Err(CustomError::Forbidden(
            "只有心愿的发起方或履约方可以处理逾期".into(),
        ));
    }

    if status != "CLAIMED" {
        return Err(CustomError::BadRequest("心愿状态不允许逾期处理".into()));
    }

    // P2-3: 用条件 UPDATE 保证幂等,防止 TOCTOU 双重处理
    let expired_rows = sqlx::query(
        "UPDATE wishes SET status='EXPIRED'::wish_status_enum, expired_at=NOW(), updated_at=NOW() \
         WHERE wish_id=$1 AND status='CLAIMED'::wish_status_enum RETURNING wish_id"
    )
    .bind(wish_id)
    .fetch_optional(db)
    .await?;
    if expired_rows.is_none() {
        // 已经被其他并发请求处理过
        return Ok(ApiResponse::success(serde_json::json!({
            "wishId": wish_id,
            "status": "EXPIRED",
            "frozenAmountUnfrozen": 0,
            "alreadyProcessed": true
        })));
    }

    // 获取冻结金额并解冻(用 idempotency_key 二次防护)
    let frozen_amount: i64 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COALESCE(SUM(CASE WHEN type='FREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint - SUM(CASE WHEN type='UNFREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint, 0::bigint) FROM love_point_transactions WHERE user_id=$1 AND group_id=$2 AND biz_id=$3 AND biz_type = 'wish'"#
    )
    .bind(requester_id)
    .bind(group_id)
    .bind(wish_id)
    .fetch_optional(db)
    .await?
    .unwrap_or(0);

    if frozen_amount > 0 {
        let idempotency_key = format!("wish_expire_{}", wish_id);
        let row = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(SUM(CASE WHEN type IN ('EARN'::love_point_tx_type_enum) THEN amount ELSE 0 END)::bigint, 0::bigint), COALESCE(SUM(CASE WHEN type='FREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint - SUM(CASE WHEN type='UNFREEZE'::love_point_tx_type_enum THEN amount ELSE 0 END)::bigint, 0::bigint) FROM love_point_transactions WHERE user_id=$1 AND group_id=$2"
        )
        .bind(requester_id)
        .bind(group_id)
        .fetch_one(db)
        .await?;
        let (available_before, frozen_before) = row;

        sqlx::query(
            r#"INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
               VALUES ($1, $2, 'UNFREEZE'::love_point_tx_type_enum, $3, $4, $4+$3, $5, 0, 'wish', $6, $7, NOW())"#
        )
        .bind(requester_id)
        .bind(group_id)
        .bind(frozen_amount)
        .bind(available_before)
        .bind(frozen_before)
        .bind(wish_id)
        .bind(&idempotency_key)
        .execute(db)
        .await?;
    }

    // P1-3:发布逾期事件,通知双方
    use crate::infrastructure::event::publisher::EventPublisher;
    let _ = EventPublisher::publish(
        db,
        crate::domain::event::EventType::WishExpired,
        crate::domain::event::WishExpiredPayload {
            wish_id,
            requester_id,
            fulfiller_id: sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(fulfiller_id, 0) FROM wishes WHERE wish_id = $1"
            )
            .bind(wish_id)
            .fetch_one(db)
            .await
            .unwrap_or(0),
            group_id,
            unfrozen_amount: frozen_amount as i32,
            trace_id: None,
        },
        None,
        Some(group_id),
        Some("wish"),
        Some(wish_id),
    )
    .await;

    Ok(ApiResponse::success(serde_json::json!({
        "wishId": wish_id,
        "status": "EXPIRED",
        "frozenAmountUnfrozen": frozen_amount
    })))
}

/// 获取心愿打卡记录列表
/// GET /api/wishes/{wish_id}/checkins
/// FSD v2 7.12
#[utoipa::path(
    get,
    path = "/api/wishes/{wish_id}/checkins",
    tag = "心愿",
    params(("wish_id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "获取成功"),
        (status = 404, description = "心愿不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_wish_checkins(
    _user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let checkins = sqlx::query(
        r#"SELECT wc.id, wc.wish_id, wc.user_id, wc.content, wc.location, wc.images, wc.created_at, u.nick_name
           FROM wish_checkins wc JOIN users u ON u.user_id = wc.user_id WHERE wc.wish_id = $1 ORDER BY wc.created_at DESC"#
    )
    .bind(wish_id)
    .fetch_all(&state.db_pool)
    .await?;
    let items: Vec<serde_json::Value> = checkins
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "wishId": r.get::<i64, _>("wish_id"),
                "userId": r.get::<i64, _>("user_id"),
                "nickname": r.get::<Option<String>, _>("nick_name"),
                "content": r.get::<Option<String>, _>("content"),
                "location": r.get::<Option<String>, _>("location"),
                "images": r.get::<Option<serde_json::Value>, _>("images"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();
    Ok(ApiResponse::success(
        serde_json::json!({ "checkins": items }),
    ))
}

/// 获取我作为履约人的待履约心愿
/// GET /api/wishes/pending-fulfillment
#[utoipa::path(
    get,
    path = "/api/wishes/pending-fulfillment",
    tag = "心愿",
    params(
        ("group_id" = Option<i64>, Query, description = "组ID筛选")
    ),
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
pub async fn pending_fulfillment(
    user_token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    query: Query<PendingFulfillmentQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let rows = match query.group_id {
        Some(gid) => {
            sqlx::query(
                r#"
            SELECT w.wish_id, w.wish_name, w.final_cost, w.fulfillment_due_at, w.status, w.group_id,
                   u.nick_name as requester_nickname,
                   CASE WHEN w.fulfillment_due_at < NOW() THEN true ELSE false END as is_overdue
            FROM wishes w
            JOIN users u ON u.user_id = w.requester_id
            WHERE w.fulfiller_id = $1 AND w.status = 'CLAIMED'::wish_status_enum AND w.group_id = $2
            ORDER BY w.fulfillment_due_at ASC
            "#,
            )
            .bind(user_token.user_id)
            .bind(gid)
            .fetch_all(db)
            .await?
        }
        None => {
            sqlx::query(
                r#"
            SELECT w.wish_id, w.wish_name, w.final_cost, w.fulfillment_due_at, w.status, w.group_id,
                   u.nick_name as requester_nickname,
                   CASE WHEN w.fulfillment_due_at < NOW() THEN true ELSE false END as is_overdue
            FROM wishes w
            JOIN users u ON u.user_id = w.requester_id
            WHERE w.fulfiller_id = $1 AND w.status = 'CLAIMED'::wish_status_enum
            ORDER BY w.fulfillment_due_at ASC
            "#,
            )
            .bind(user_token.user_id)
            .fetch_all(db)
            .await?
        }
    };
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "wishId": r.get::<i64, _>("wish_id"),
                "name": r.get::<String, _>("wish_name"),
                "requesterNickname": r.get::<Option<String>, _>("requester_nickname"),
                "finalCost": r.get::<Option<i32>, _>("final_cost"),
                "fulfillmentDueAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("fulfillment_due_at"),
                "status": r.get::<String, _>("status"),
                "isOverdue": r.get::<bool, _>("is_overdue")
            })
        })
        .collect();
    Ok(ApiResponse::success(serde_json::json!({ "items": items })))
}
