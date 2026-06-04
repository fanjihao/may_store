// API 层 - 心愿路由
// FSD.latest.md compliant - 心愿创建、协商、选择、履约

use ntex::web::{
    self,
    types::{Json, Path, Query, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::wish_service::WishService;
use crate::domain::wish::{
    WishCreateInput, WishDeadlineInput, WishFeedbackInput, WishOut,
    WishQuoteInput, WishRejectInput,
};
use crate::{
    config::AppState,
    errors::CustomError,
    middlewares::auth::UserToken,
    utils::response::ApiResponse,
};

/// 配置心愿路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}/wishes")
            .route("", web::post().to(create_group_wish))  // FSD v2 7.1 创建心愿
            .route("", web::get().to(list_group_wishes))   // FSD v2 7.2 获取心愿列表
    )
    .service(
        web::scope("/api/wishes")
            .route("/pending-fulfillment", web::get().to(pending_fulfillment))
            .route("/{id}", web::get().to(get_wish))  // FSD v2 7.3 获取心愿详情
            .route("/{id}/quote", web::post().to(wish_quote))           // FSD v2 7.4 协商报价
            .route("/{id}/deadline", web::post().to(wish_deadline))     // FSD v2 7.5 协商履约期限
            .route("/{id}/confirm-agreement", web::post().to(wish_confirm_agreement))  // FSD v2 7.6 双方确认
            .route("/{id}/reject", web::post().to(wish_reject))         // FSD v2 7.7 拒绝/关闭
            .route("/{id}/select", web::post().to(wish_select))        // FSD v2 7.8 选择心愿
            .route("/{id}/feedback", web::post().to(submit_feedback))   // FSD v2 7.9 提交打卡反馈
            .route("/{id}/expire", web::post().to(wish_expire))         // FSD v2 7.10 逾期处理
            .route("/{id}/close", web::post().to(wish_close))           // FSD v2 7.11 关闭心愿
            .route("/{id}/checkins", web::get().to(get_wish_checkins)) // FSD v2 7.12 获取打卡记录
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
    security(("cookie_auth" = []))
)]
pub async fn create_group_wish(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    data: Json<WishCreateInput>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
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
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn list_group_wishes(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    query: Query<WishListQuery>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;
    let limit = query.limit.unwrap_or(20).min(100);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(user_token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 构建筛选条件
    let status_filter = if let Some(ref status) = query.status {
        format!("AND w.status = '{}'", status)
    } else {
        String::new()
    };

    let role_filter = if let Some(ref role) = query.role {
        match role.as_str() {
            "REQUESTER" => format!("AND w.requester_id = {}", user_token.user_id),
            "FULFILLER" => format!("AND w.fulfiller_id = {}", user_token.user_id),
            _ => String::new()
        }
    } else {
        String::new()
    };

    let cursor_filter = if let Some(ref cursor) = query.cursor {
        format!("AND w.created_at < '{}'", cursor)
    } else {
        String::new()
    };

    let sql = format!(
        r#"
        SELECT w.wish_id, w.wish_name, w.description, w.final_cost, w.fulfillment_deadline_hours,
               w.status, w.requester_id, w.fulfiller_id, w.selected_by, w.selected_at,
               w.fulfillment_due_at, w.created_at,
               u1.nick_name as requester_nickname,
               u2.nick_name as fulfiller_nickname
        FROM wishes w
        JOIN users u1 ON u1.user_id = w.requester_id
        JOIN users u2 ON u2.user_id = w.fulfiller_id
        WHERE w.group_id = $1 {} {} {}
        ORDER BY w.created_at DESC
        LIMIT $2
        "#,
        status_filter, role_filter, cursor_filter
    );

    let rows = sqlx::query(&sql).bind(gid).bind(limit + 1).fetch_all(db).await?;

    let has_more = rows.len() > limit as usize;

    let wishes: Vec<serde_json::Value> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            serde_json::json!({
                "wish_id": r.get::<i64, _>("wish_id"),
                "name": r.get::<String, _>("wish_name"),
                "description": r.get::<Option<String>, _>("description"),
                "final_cost": r.get::<Option<i32>, _>("final_cost"),
                "fulfillment_deadline_hours": r.get::<Option<i32>, _>("fulfillment_deadline_hours"),
                "status": r.get::<String, _>("status"),
                "requester_id": r.get::<i64, _>("requester_id"),
                "requester_nickname": r.get::<Option<String>, _>("requester_nickname"),
                "fulfiller_id": r.get::<i64, _>("fulfiller_id"),
                "fulfiller_nickname": r.get::<Option<String>, _>("fulfiller_nickname"),
                "selected_by": r.get::<Option<i64>, _>("selected_by"),
                "selected_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("selected_at"),
                "fulfillment_due_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("fulfillment_due_at"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
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

    Ok(ApiResponse::success(serde_json::json!({
        "wishes": wishes,
        "next_cursor": next_cursor,
        "has_more": has_more
    })))
}

/// 获取心愿详情
/// GET /api/wishes/{id}
/// FSD v2 7.3
#[utoipa::path(
    get,
    path = "/api/wishes/{id}",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "获取成功", body = WishOut),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_wish(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let (rec, feedback) = WishService::get_wish(&state.db_pool, *id).await?;
    Ok(ApiResponse::success(WishOut::from_record(rec, feedback)))
}

/// 协商报价
/// POST /api/wishes/{id}/quote
/// FSD v2 7.4
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/quote",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishQuoteInput,
    responses(
        (status = 200, description = "报价成功"),
        (status = 400, description = "心愿状态不允许报价"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_quote(
    user_token: UserToken,
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
/// POST /api/wishes/{id}/deadline
/// FSD v2 7.5
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/deadline",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishDeadlineInput,
    responses(
        (status = 200, description = "设置成功"),
        (status = 400, description = "心愿状态不允许设置期限"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_deadline(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishDeadlineInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out = WishService::set_deadline(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 双方线上确认积分和期限
/// POST /api/wishes/{id}/confirm-agreement
/// FSD v2 7.6
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/confirm-agreement",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "确认成功，心愿进入心愿池"),
        (status = 400, description = "心愿状态不允许确认"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_confirm_agreement(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out = WishService::confirm_agreement(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(ApiResponse::success(out))
}

/// 拒绝或关闭协商
/// POST /api/wishes/{id}/reject
/// FSD v2 7.7
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/reject",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishRejectInput,
    responses(
        (status = 200, description = "操作成功"),
        (status = 400, description = "心愿状态不允许此操作"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_reject(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
    body: Json<WishRejectInput>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let input = body.into_inner();
    let out = WishService::reject_wish(&state.db_pool, user_token.user_id, wish_id, &input).await?;
    Ok(ApiResponse::success(out))
}

/// 选择心愿并冻结积分
/// POST /api/wishes/{id}/select
/// FSD v2 7.8
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/select",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "选择成功，积分已冻结"),
        (status = 400, description = "积分不足或心愿状态不允许"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_select(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let out = WishService::select_wish(&state.db_pool, user_token.user_id, wish_id).await?;
    Ok(ApiResponse::success(out))
}

/// 提交打卡反馈
/// POST /api/wishes/{id}/feedback
/// FSD v2 7.9
#[utoipa::path(
    put,
    path = "/api/wishes/{id}/feedback",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishFeedbackInput,
    responses(
        (status = 200, description = "提交成功", body = WishOut),
        (status = 400, description = "心愿状态不允许"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn submit_feedback(
    user_token: UserToken,
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
/// POST /api/wishes/{id}/close
/// FSD v2 7.11
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/close",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    request_body = WishCloseInput,
    responses(
        (status = 200, description = "关闭成功"),
        (status = 400, description = "心愿状态不允许关闭"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_close(
    user_token: UserToken,
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
        &WishRejectInput { reason: input.reason },
    )
    .await?;
    Ok(ApiResponse::success(out))
}

/// 心愿履约逾期处理
/// POST /api/wishes/{id}/expire
/// FSD v2 7.10
#[utoipa::path(
    post,
    path = "/api/wishes/{id}/expire",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "处理成功"),
        (status = 400, description = "心愿状态不允许逾期处理"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn wish_expire(
    _user_token: UserToken,
    state: State<Arc<AppState>>,
    id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let wish_id = *id;
    let db = &state.db_pool;

    // 检查心愿状态
    let row = sqlx::query(
        "SELECT status::text, requester_id, group_id FROM wishes WHERE wish_id = $1"
    )
    .bind(wish_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::NotFound("心愿不存在".into()))?;

    let status: String = row.get("status");
    let requester_id: i64 = row.get("requester_id");
    let group_id: i64 = row.get("group_id");

    if status != "CLAIMED" {
        return Err(CustomError::BadRequest("心愿状态不允许逾期处理".into()));
    }

    // 获取冻结金额并解冻
    let frozen_amount: i64 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COALESCE(SUM(CASE WHEN type='FREEZE' THEN amount ELSE 0 END)::bigint, 0::bigint) FROM love_point_transactions WHERE user_id=$1 AND group_id=$2 AND biz_id=$3 AND biz_type='WISH'"#
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
            "SELECT COALESCE(SUM(CASE WHEN type IN ('EARN') THEN amount ELSE 0 END)::bigint, 0::bigint), COALESCE(SUM(CASE WHEN type='FREEZE' THEN amount ELSE 0 END)::bigint, 0::bigint) FROM love_point_transactions WHERE user_id=$1 AND group_id=$2"
        )
        .bind(requester_id)
        .bind(group_id)
        .fetch_one(db)
        .await?;
        let (available_before, frozen_before) = row;

        sqlx::query(
            r#"INSERT INTO love_point_transactions (user_id, group_id, type, amount, available_before, available_after, frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
               VALUES ($1, $2, 'UNFREEZE', $3, $4, $4+$3, $5, 0, 'WISH', $6, $7, NOW())"#
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

    // 更新心愿状态为 EXPIRED
    sqlx::query("UPDATE wishes SET status='EXPIRED', expired_at=NOW(), updated_at=NOW() WHERE wish_id=$1")
        .bind(wish_id)
        .execute(db)
        .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "wishId": wish_id,
        "status": "EXPIRED",
        "frozenAmountUnfrozen": frozen_amount
    })))
}

/// 获取心愿打卡记录列表
/// GET /api/wishes/{id}/checkins
/// FSD v2 7.12
#[utoipa::path(
    get,
    path = "/api/wishes/{id}/checkins",
    tag = "心愿",
    params(("id" = i64, Path, description = "心愿ID")),
    responses(
        (status = 200, description = "获取成功"),
        (status = 404, description = "心愿不存在")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_wish_checkins(
    _user_token: UserToken,
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
    Ok(ApiResponse::success(serde_json::json!({ "checkins": items })))
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
    security(("cookie_auth" = []))
)]
pub async fn pending_fulfillment(
    user_token: UserToken,
    state: State<Arc<AppState>>,
    query: Query<PendingFulfillmentQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let group_filter = if let Some(gid) = query.group_id {
        format!(" AND w.group_id = {}", gid)
    } else {
        String::new()
    };
    let sql = format!(
        r#"
        SELECT w.wish_id, w.wish_name, w.final_cost, w.fulfillment_due_at, w.status, w.group_id,
               u.nick_name as requester_nickname,
               CASE WHEN w.fulfillment_due_at < NOW() THEN true ELSE false END as is_overdue
        FROM wishes w
        JOIN users u ON u.user_id = w.requester_id
        WHERE w.fulfiller_id = $1 AND w.status = 'CLAIMED'{}
        ORDER BY w.fulfillment_due_at ASC
        "#,
        group_filter
    );
    let rows = sqlx::query(&sql).bind(user_token.user_id).fetch_all(db).await?;
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
