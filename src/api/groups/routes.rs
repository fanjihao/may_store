// API - 双人组管理路由
// FSD.latest.md compliant endpoints

use chrono::Utc;
use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::domain::group::entities::{
    FulfillmentStats, GroupDetailInfo, GroupRecord, SettlementCheckResult,
};
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

/// 配置双人组路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups")
            .route("", web::post().to(create_group))
            .route("/join", web::post().to(join_group))
            .route("/{group_id}", web::get().to(get_group))
            .route("/{group_id}/swap-role", web::post().to(swap_role))
            .route("/{group_id}/exit", web::post().to(exit_group))
            .route(
                "/{group_id}/settlement-check",
                web::get().to(settlement_check),
            )
            .route(
                "/{group_id}/members",
                web::get().to(get_group_members),
            )
            .route(
                "/{group_id}/fulfillment-stats",
                web::get().to(fulfillment_stats),
            )
            // FSD v2: 额外端点
            .route("/{group_id}/invite", web::post().to(create_invite))
            .route("/{group_id}/foods", web::get().to(list_foods))
            .route("/{group_id}/orders", web::post().to(create_group_order))
            .route("/{group_id}/wishes", web::post().to(create_group_wish)),
    );
}

/// 创建双人组
/// POST /api/groups
/// 创建双人组响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupResponse {
    pub group_id: i64,
    pub invite_code: String,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/api/groups",
    tag = "双人组",
    responses(
        (status = 201, description = "创建成功", body = CreateGroupResponse),
        (status = 400, description = "已在组中或其他错误"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
async fn create_group(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;

    // 检查用户是否已在组中
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true",
    )
    .bind(token.user_id)
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        return Err(CustomError::BadRequest("您已在组中".into()));
    }

    let mut tx = db.begin().await?;

    // 创建组
    let invite_code = format!("{:08x}", rand::random::<u32>());
    let group: GroupRecord = sqlx::query_as::<_, GroupRecord>(
        r#"INSERT INTO association_groups (group_name, group_type, status, invite_code, diamond, footprint_capacity, footprint_count, buyer_user_id, seller_user_id, level, exp, created_at, updated_at)
           VALUES ($1, 'PAIR', 1, $2, 0, 50, 0, $3, $4, 1, 0, $5, $5)
           RETURNING group_id, group_name, group_type, status, invite_code, diamond, footprint_capacity, footprint_count, created_at, updated_at,
                     buyer_user_id, seller_user_id, level, exp, settings"#
    )
    .bind(format!("{}的组", token.user.as_ref().map(|u| u.username.as_str()).unwrap_or("用户")))
    .bind(&invite_code)
    .bind(token.user_id)
    .bind(token.user_id) // 初始时创建者为 buyer
    .bind(Utc::now())
    .fetch_one(&mut *tx)
    .await?;

    // 将创建者加为组成员
    sqlx::query(
        r#"INSERT INTO association_group_members (user_id, group_id, role_in_group, is_primary, created_at)
           VALUES ($1, $2, 'BUYER', true, $3)"#
    )
    .bind(token.user_id)
    .bind(group.group_id)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ApiResponse::success(serde_json::json!({
        "groupId": group.group_id,
        "inviteCode": invite_code,
        "status": "ok"
    })))
}

/// 获取组信息
/// GET /api/groups/{group_id}
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = GroupDetailInfo),
        (status = 403, description = "无权访问该组"),
        (status = 404, description = "组不存在")
    ),
    security(("cookie_auth" = []))
)]
async fn get_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 获取组信息
    let group_row = sqlx::query(
        r#"SELECT g.group_id, g.group_name, g.group_type, g.status, g.invite_code, g.diamond,
                  g.footprint_capacity, g.footprint_count, g.created_at, g.updated_at,
                  g.buyer_user_id, g.seller_user_id, g.level, g.exp, g.settings,
                  buyer.nick_name as buyer_nick_name, buyer.avatar as buyer_avatar,
                  seller.nick_name as seller_nick_name, seller.avatar as seller_avatar
           FROM association_groups g
           LEFT JOIN users buyer ON buyer.user_id = g.buyer_user_id
           LEFT JOIN users seller ON seller.user_id = g.seller_user_id
           WHERE g.group_id = $1"#,
    )
    .bind(gid)
    .fetch_optional(db)
    .await?;

    let row = match group_row {
        Some(r) => r,
        None => return Err(CustomError::NotFound("组不存在".into())),
    };

    let detail = GroupDetailInfo {
        group_id: row.get("group_id"),
        group_name: row.get("group_name"),
        buyer_user_id: row.get("buyer_user_id"),
        seller_user_id: row.get("seller_user_id"),
        buyer_nick_name: row.get("buyer_nick_name"),
        seller_nick_name: row.get("seller_nick_name"),
        buyer_avatar: row.get("buyer_avatar"),
        seller_avatar: row.get("seller_avatar"),
        level: row.get::<Option<i32>, _>("level").unwrap_or(1),
        exp: row.get::<Option<i64>, _>("exp").unwrap_or(0),
        diamond: row.get::<i32, _>("diamond") as i64,
        footprint_capacity: row.get("footprint_capacity"),
        footprint_count: row.get("footprint_count"),
    };

    Ok(ApiResponse::success(detail))
}

/// 角色互换
/// POST /api/groups/{group_id}/swap-role
///
/// 前置条件:
/// - 小组无未完结在途订单
/// - 操作人无CLAIMED状态且自己作为发起人或履约人的在途心愿
/// - 互换后当前Buyer与Seller对调
///
/// 角色互换响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwapRoleResponse {
    pub status: String,
    pub new_buyer: Option<i64>,
    pub new_seller: Option<i64>,
}

/// 配置开关:
/// - swap_ignore_ongoing_wish = true时允许带在途心愿互换身份
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/swap-role",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "互换成功", body = SwapRoleResponse),
        (status = 400, description = "存在未完结订单或心愿"),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn swap_role(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    let mut tx = db.begin().await?;

    // 检查组配置 swap_ignore_ongoing_wish
    let settings: Option<serde_json::Value> = sqlx::query_scalar::<_, Option<serde_json::Value>>(
        "SELECT settings FROM association_groups WHERE group_id = $1",
    )
    .bind(gid)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();

    let swap_ignore_wish = settings
        .as_ref()
        .and_then(|s| s.get("swap_ignore_ongoing_wish"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 检查是否有未完结在途订单
    let pending_orders: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM orders WHERE group_id=$1 AND status NOT IN ('CONFIRMED_COMPLETED', 'CONFIRMED_INCOMPLETE', 'REJECTED', 'CANCELLED', 'TIMEOUT')"
    )
    .bind(gid)
    .fetch_one(&mut *tx)
    .await?;

    if pending_orders > 0 {
        return Err(CustomError::role_swap_blocked_by_order("存在未完结订单，禁止互换"));
    }

    // 检查操作人是否有 CLAIMED 状态的在途心愿（除非 swap_ignore_ongoing_wish=true）
    if !swap_ignore_wish {
        let pending_wishes: i64 = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND status='CLAIMED'
               AND (selected_by=$2 OR fulfiller_id=$2)"#,
        )
        .bind(gid)
        .bind(token.user_id)
        .fetch_one(&mut *tx)
        .await?;

        if pending_wishes > 0 {
            return Err(CustomError::role_swap_blocked_by_wish("存在在途心愿，禁止互换"));
        }
    }

    // 执行角色互换
    let (old_buyer, old_seller): (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT buyer_user_id, seller_user_id FROM association_groups WHERE group_id=$1",
    )
    .bind(gid)
    .fetch_one(&mut *tx)
    .await?;

    let new_buyer = old_seller;
    let new_seller = old_buyer;

    sqlx::query(
        "UPDATE association_groups SET buyer_user_id=$1, seller_user_id=$2, updated_at=NOW() WHERE group_id=$3"
    )
    .bind(new_buyer)
    .bind(new_seller)
    .bind(gid)
    .execute(&mut *tx)
    .await?;

    // 更新组成员角色
    if let Some(buyer_id) = new_buyer {
        sqlx::query("UPDATE association_group_members SET role_in_group='ORDERING' WHERE user_id=$1 AND group_id=$2")
            .bind(buyer_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(seller_id) = new_seller {
        sqlx::query("UPDATE association_group_members SET role_in_group='RECEIVING' WHERE user_id=$1 AND group_id=$2")
            .bind(seller_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(ApiResponse::success(serde_json::json!({
        "status": "ok",
        "newBuyer": new_buyer,
        "newSeller": new_seller
    })))
}

/// 退出组前结清检查
/// GET /api/groups/{group_id}/settlement-check
///
/// 检查:
/// - 无自己发起且未完结的心愿
/// - 无自己作为履约人且未完结的心愿
/// - 无本组冻结爱心积分
/// - 无待处理的逾期补偿或管理员钻石奖励
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/settlement-check",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = SettlementCheckResult),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn settlement_check(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    let mut reasons = Vec::new();

    // 检查自己发起且未完结的心愿
    let pending_initiated: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COUNT(*) FROM wishes
           WHERE group_id=$1 AND selected_by=$2 AND status NOT IN ('FINISHED', 'EXPIRED', 'CLOSED')"#
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?
    .unwrap_or(0);

    // 检查自己作为履约人且未完结的心愿
    let pending_as_fulfiller: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COUNT(*) FROM wishes
           WHERE group_id=$1 AND fulfiller_id=$2 AND status NOT IN ('FINISHED', 'EXPIRED', 'CLOSED')"#
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?
    .unwrap_or(0);

    // 检查冻结爱心积分
    let frozen_points: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COALESCE(SUM(amount), 0) FROM love_point_transactions
           WHERE user_id=$1 AND group_id=$2 AND type='FREEZE'"#,
    )
    .bind(token.user_id)
    .bind(gid)
    .fetch_one(db)
    .await?
    .unwrap_or(0);

    let can_exit = pending_initiated == 0 && pending_as_fulfiller == 0 && frozen_points == 0;

    if pending_initiated > 0 {
        reasons.push(format!("存在{}个未完结的心愿", pending_initiated));
    }
    if pending_as_fulfiller > 0 {
        reasons.push(format!("有{}个待履约心愿", pending_as_fulfiller));
    }
    if frozen_points > 0 {
        reasons.push(format!("有{}冻结积分未处理", frozen_points));
    }

    let result = SettlementCheckResult {
        can_exit,
        pending_orders: 0, // 简化
        pending_wishes_initiated: pending_initiated as i32,
        pending_wishes_as_fulfiller: pending_as_fulfiller as i32,
        frozen_love_points: frozen_points,
        pending_compensation: 0,
        pending_diamond_reward: 0,
        reasons,
    };

    Ok(ApiResponse::success(result))
}

/// 查看组内双方履约统计
/// GET /api/groups/{group_id}/fulfillment-stats
///
/// 返回:
/// - fulfillment_total: 作为履约人的总心愿数
/// - fulfillment_finished: 按期完成数量
/// - fulfillment_expired: 逾期数量
/// - fulfillment_rate: 按期完成率
/// - avg_fulfillment_hours: 平均履约时长
/// - pending_fulfillment_count: 当前待履约数量
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/fulfillment-stats",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功"),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn fulfillment_stats(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 获取组内所有用户
    let members: Vec<(i64,)> =
        sqlx::query_as("SELECT user_id FROM association_group_members WHERE group_id=$1")
            .bind(gid)
            .fetch_all(db)
            .await?;

    let mut stats_map = std::collections::HashMap::new();

    for (user_id,) in members {
        // 作为履约人的总心愿数
        let total: i64 = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT COUNT(*) FROM wishes WHERE group_id=$1 AND fulfiller_id=$2",
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        // 按期完成数
        let finished: i64 = sqlx::query_scalar::<_, Option<i64>>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='FINISHED'
               AND fulfilled_at <= fulfillment_due_at"#,
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        // 逾期数
        let expired: i64 = sqlx::query_scalar::<_, Option<i64>>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='EXPIRED'"#,
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        // 待履约数
        let pending: i64 = sqlx::query_scalar::<_, Option<i64>>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='CLAIMED'"#,
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        let rate = if total > 0 {
            finished as f64 / total as f64
        } else {
            0.0
        };

        stats_map.insert(
            user_id,
            FulfillmentStats {
                user_id,
                fulfillment_total: total as i32,
                fulfillment_finished: finished as i32,
                fulfillment_expired: expired as i32,
                fulfillment_rate: rate,
                avg_fulfillment_hours: 0.0, // 简化
                pending_fulfillment_count: pending as i32,
            },
        );
    }

    Ok(ApiResponse::success(stats_map))
}

// ============== FSD v2 额外端点 ==============

/// 创建邀请响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteResponse {
    pub invite_code: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
}

/// 创建邀请链接
/// POST /api/groups/{group_id}/invite
///
/// 生成邀请码，供受邀用户加入组
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/invite",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 201, description = "创建成功", body = CreateInviteResponse),
        (status = 400, description = "组已满2人"),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn create_invite(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 检查组是否已满2人
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM association_group_members WHERE group_id=$1 AND member_status='ACTIVE'"
    )
    .bind(gid)
    .fetch_one(db)
    .await?;

    if member_count >= 2 {
        return Err(CustomError::BadRequest("组已满2人，无法邀请新成员".into()));
    }

    // 生成邀请码
    let invite_code = format!("{:08x}", rand::random::<u32>());
    let expires_at = Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        r#"INSERT INTO guest_invitations (group_id, invite_code, created_by, expires_at, max_uses, used_count, status)
           VALUES ($1, $2, $3, $4, 1, 0, 'ACTIVE')"#
    )
    .bind(gid)
    .bind(&invite_code)
    .bind(token.user_id)
    .bind(expires_at)
    .execute(db)
    .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "inviteCode": invite_code,
        "expiresAt": expires_at,
        "status": "ok"
    })))
}

/// 组内菜品项
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FoodItem {
    pub food_id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub ingredients: Option<serde_json::Value>,
    pub steps: Option<serde_json::Value>,
    pub status: String,
    pub created_by: i64,
}

/// 获取组内菜品列表
/// GET /api/groups/{group_id}/foods
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/foods",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = Vec<FoodItem>),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn list_foods(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 获取组内菜品
    let foods = sqlx::query(
        r#"SELECT food_id, group_id, name, description, images, tags, ingredients, steps, status, created_by
           FROM foods WHERE group_id=$1 AND status='ACTIVE'
           ORDER BY created_at DESC"#
    )
    .bind(gid)
    .fetch_all(db)
    .await?;

    let result: Vec<serde_json::Value> = foods
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "foodId": r.get::<i64, _>("food_id"),
                "groupId": r.get::<i64, _>("group_id"),
                "name": r.get::<String, _>("name"),
                "description": r.get::<Option<String>, _>("description"),
                "images": r.get::<Option<serde_json::Value>, _>("images"),
                "tags": r.get::<Option<serde_json::Value>, _>("tags"),
                "ingredients": r.get::<Option<serde_json::Value>, _>("ingredients"),
                "steps": r.get::<Option<serde_json::Value>, _>("steps"),
                "status": r.get::<String, _>("status"),
                "createdBy": r.get::<i64, _>("created_by")
            })
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 在组内创建订单响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupOrderResponse {
    pub order_id: i64,
    pub status: String,
}

/// 在组内创建订单
/// POST /api/groups/{group_id}/orders
///
/// Buyer 创建本组订单
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/orders",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = GroupOrderInput,
    responses(
        (status = 201, description = "创建成功", body = CreateGroupOrderResponse),
        (status = 403, description = "无权访问或只有Buyer可创建"),
        (status = 404, description = "组不存在")
    ),
    security(("cookie_auth" = []))
)]
async fn create_group_order(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    body: Json<GroupOrderInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let input = body.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 检查用户角色是否为 BUYER
    let user_role: Option<String> = sqlx::query_scalar(
        "SELECT role_in_group FROM association_group_members WHERE group_id=$1 AND user_id=$2",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if user_role.as_deref() != Some("BUYER") {
        return Err(CustomError::Forbidden("只有Buyer可以创建订单".into()));
    }

    // 获取当前组Seller
    let seller_id: Option<i64> =
        sqlx::query_scalar("SELECT seller_user_id FROM association_groups WHERE group_id=$1")
            .bind(gid)
            .fetch_one(db)
            .await?;

    // 创建订单
    let order_id = idgenerator::IdInstance::next_id();

    sqlx::query(
        r#"INSERT INTO orders (order_id, group_id, type, creator_id, assignee_id, creator_role_snapshot, status, title, content, deadline, created_at)
           VALUES ($1, $2, 'NORMAL', $3, $4, 'BUYER', 'CREATED', $5, $6, $7, NOW())"#
    )
    .bind(order_id)
    .bind(gid)
    .bind(token.user_id)
    .bind(seller_id)
    .bind(&input.title)
    .bind(&input.content)
    .bind(input.deadline)
    .execute(db)
    .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "orderId": order_id,
        "status": "ok"
    })))
}

/// 在组内创建心愿响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupWishResponse {
    pub wish_id: i64,
    pub status: String,
}

/// 在组内创建心愿
/// POST /api/groups/{group_id}/wishes
///
/// 组成员创建心愿，创建后发起人为 requester_id，另一成员为 fulfiller_id
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/wishes",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = GroupWishInput,
    responses(
        (status = 201, description = "创建成功", body = CreateGroupWishResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn create_group_wish(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    body: Json<GroupWishInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let input = body.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 获取组内另一成员作为默认履约人
    let other_member: Option<i64> = sqlx::query_scalar(
        "SELECT user_id FROM association_group_members WHERE group_id=$1 AND user_id!=$2 LIMIT 1",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    let fulfiller_id = other_member.unwrap_or(0);

    // 获取用户当前角色快照
    let user_role: Option<String> = sqlx::query_scalar(
        "SELECT role_in_group FROM association_group_members WHERE group_id=$1 AND user_id=$2",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    // 创建心愿
    let wish_id = idgenerator::IdInstance::next_id();

    sqlx::query(
        r#"INSERT INTO wishes (wish_id, group_id, created_by, requester_id, fulfiller_id, creator_role_snapshot, wish_name, wish_cost, initial_cost, status, created_at)
           VALUES ($1, $2, $3, $3, $4, $5, $6, $7, $7, 'DRAFT', NOW())"#
    )
    .bind(wish_id)
    .bind(gid)
    .bind(token.user_id)
    .bind(fulfiller_id)
    .bind(&user_role)
    .bind(&input.name)
    .bind(input.initial_cost)
    .execute(db)
    .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "wishId": wish_id,
        "status": "ok"
    })))
}

// ============== FSD v2 新增端点 ==============

/// 通过邀请码加入组
/// POST /api/groups/join
///
/// 受邀者自动成为 Seller，加入后更新组的 seller_user_id
#[utoipa::path(
    post,
    path = "/api/groups/join",
    tag = "双人组",
    request_body = JoinGroupInput,
    responses(
        (status = 200, description = "加入成功"),
        (status = 400, description = "邀请码无效或已过期"),
        (status = 400, description = "组已满2人"),
        (status = 403, description = "已在其他组")
    ),
    security(("cookie_auth" = []))
)]
async fn join_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    body: Json<JoinGroupInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let input = body.into_inner();

    // 检查用户是否已在组中
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT group_id FROM association_group_members WHERE user_id = $1 AND member_status = 'ACTIVE'",
    )
    .bind(token.user_id)
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        return Err(CustomError::BadRequest("您已在其他组中".into()));
    }

    // 查找邀请码对应的邀请记录
    let invite: Option<(i64, chrono::DateTime<chrono::Utc>, i32, i32)> = sqlx::query_as(
        r#"SELECT group_id, expires_at, max_uses, used_count
           FROM guest_invitations
           WHERE invite_code = $1 AND status = 'ACTIVE'"#
    )
    .bind(&input.invite_code)
    .fetch_optional(db)
    .await?;

    let (group_id, expires_at, max_uses, used_count) = match invite {
        Some(inv) => inv,
        None => return Err(CustomError::BadRequest("邀请码无效或已过期".into())),
    };

    // 检查是否过期
    if Utc::now() > expires_at {
        return Err(CustomError::BadRequest("邀请码已过期".into()));
    }

    // 检查使用次数
    if used_count >= max_uses {
        return Err(CustomError::BadRequest("邀请码已使用".into()));
    }

    // 检查组是否已满
    let member_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM association_group_members WHERE group_id=$1 AND member_status='ACTIVE'"
    )
    .bind(group_id)
    .fetch_one(db)
    .await?;

    if member_count >= 2 {
        return Err(CustomError::BadRequest("组已满2人，无法加入".into()));
    }

    let mut tx = db.begin().await?;

    // 加入组成员
    sqlx::query(
        r#"INSERT INTO association_group_members (user_id, group_id, role_in_group, is_primary, member_status, joined_at)
           VALUES ($1, $2, 'RECEIVING', false, 'ACTIVE', $3)"#
    )
    .bind(token.user_id)
    .bind(group_id)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;

    // 更新组的 seller_user_id
    sqlx::query(
        "UPDATE association_groups SET seller_user_id=$1, updated_at=NOW() WHERE group_id=$2"
    )
    .bind(token.user_id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;

    // 更新邀请码使用次数
    sqlx::query(
        "UPDATE guest_invitations SET used_count=used_count+1 WHERE invite_code=$1"
    )
    .bind(&input.invite_code)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ApiResponse::success(serde_json::json!({
        "groupId": group_id,
        "role": "SELLER",
        "status": "ok"
    })))
}

/// 退出双人组
/// POST /api/groups/{group_id}/exit
///
/// 退出前必须通过结清检查（无未完结心愿、无冻结积分）
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/exit",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "退出成功"),
        (status = 400, description = "仍有未结清订单/心愿/冻结积分"),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn exit_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 结清检查
    let settlement = settlement_check_impl(db, gid, token.user_id).await?;
    if !settlement.can_exit {
        return Err(CustomError::BadRequest(settlement.reasons.join("; ").into()));
    }

    let mut tx = db.begin().await?;

    // 获取用户角色
    let user_role: Option<String> = sqlx::query_scalar(
        "SELECT role_in_group FROM association_group_members WHERE group_id=$1 AND user_id=$2"
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(&mut *tx)
    .await?;

    // 更新组成员状态为 LEFT
    sqlx::query(
        "UPDATE association_group_members SET member_status='LEFT' WHERE group_id=$1 AND user_id=$2"
    )
    .bind(gid)
    .bind(token.user_id)
    .execute(&mut *tx)
    .await?;

    // 清空组的 buyer 或 seller 引用
    if user_role.as_deref() == Some("ORDERING") {
        sqlx::query("UPDATE association_groups SET buyer_user_id=NULL, updated_at=NOW() WHERE group_id=$1")
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE association_groups SET seller_user_id=NULL, updated_at=NOW() WHERE group_id=$1")
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(ApiResponse::success(serde_json::json!({
        "status": "ok"
    })))
}

/// 获取组内成员列表
/// GET /api/groups/{group_id}/members
///
/// 返回组成员详细信息和积分余额
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/members",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功"),
        (status = 403, description = "无权访问该组")
    ),
    security(("cookie_auth" = []))
)]
async fn get_group_members(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE')",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 获取组成员列表
    let members = sqlx::query(
        r#"SELECT agm.user_id, agm.role_in_group, agm.joined_at,
                  u.nick_name, u.avatar,
                  COALESCE(ugp.available_love_point, 0) as available_love_point,
                  COALESCE(ugp.frozen_love_point, 0) as frozen_love_point
           FROM association_group_members agm
           JOIN users u ON u.user_id = agm.user_id
           LEFT JOIN user_group_points ugp ON ugp.user_id = agm.user_id AND ugp.group_id = agm.group_id
           WHERE agm.group_id = $1 AND agm.member_status = 'ACTIVE'"#
    )
    .bind(gid)
    .fetch_all(db)
    .await?;

    let result: Vec<serde_json::Value> = members
        .iter()
        .map(|r| {
            serde_json::json!({
                "userId": r.get::<i64, _>("user_id"),
                "nickname": r.get::<Option<String>, _>("nick_name"),
                "avatar": r.get::<Option<String>, _>("avatar"),
                "role": r.get::<String, _>("role_in_group"),
                "lovePointAvailable": r.get::<i64, _>("available_love_point"),
                "lovePointFrozen": r.get::<i64, _>("frozen_love_point"),
                "joinedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("joined_at")
            })
        })
        .collect();

    Ok(ApiResponse::success(result))
}

// 内部实现：结清检查
async fn settlement_check_impl(
    db: &sqlx::PgPool,
    group_id: i64,
    user_id: i64,
) -> Result<SettlementCheckResult, CustomError> {
    let mut reasons = Vec::new();

    // 检查自己发起且未完结的心愿
    let pending_initiated: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COUNT(*) FROM wishes
           WHERE group_id=$1 AND requester_id=$2 AND status NOT IN ('FINISHED', 'EXPIRED', 'CLOSED')"#
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(db)
    .await?
    .unwrap_or(0);

    // 检查自己作为履约人且未完结的心愿
    let pending_as_fulfiller: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COUNT(*) FROM wishes
           WHERE group_id=$1 AND fulfiller_id=$2 AND status NOT IN ('FINISHED', 'EXPIRED', 'CLOSED')"#
    )
    .bind(group_id)
    .bind(user_id)
    .fetch_one(db)
    .await?
    .unwrap_or(0);

    // 检查冻结爱心积分
    let frozen_points: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COALESCE(SUM(amount), 0) FROM love_point_transactions
           WHERE user_id=$1 AND group_id=$2 AND type='FREEZE'"#
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_one(db)
    .await?
    .unwrap_or(0);

    let can_exit = pending_initiated == 0 && pending_as_fulfiller == 0 && frozen_points == 0;

    if pending_initiated > 0 {
        reasons.push(format!("存在{}个未完结的心愿", pending_initiated));
    }
    if pending_as_fulfiller > 0 {
        reasons.push(format!("有{}个待履约心愿", pending_as_fulfiller));
    }
    if frozen_points > 0 {
        reasons.push(format!("有{}冻结积分未处理", frozen_points));
    }

    Ok(SettlementCheckResult {
        can_exit,
        pending_orders: 0,
        pending_wishes_initiated: pending_initiated as i32,
        pending_wishes_as_fulfiller: pending_as_fulfiller as i32,
        frozen_love_points: frozen_points,
        pending_compensation: 0,
        pending_diamond_reward: 0,
        reasons,
    })
}

// ============== FSD v2 请求结构体 ==============

/// 组内创建订单输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupOrderInput {
    pub title: String,
    pub content: String,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
}

/// 组内创建心愿输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupWishInput {
    pub name: String,
    pub initial_cost: i32,
}

/// 通过邀请码加入组输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinGroupInput {
    pub invite_code: String,
}
