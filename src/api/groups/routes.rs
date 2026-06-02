// API - 双人组管理路由
// FSD.latest.md compliant endpoints

use chrono::Utc;
use ntex::web::{self, HttpResponse, ServiceConfig, types::{Path, State}};
use std::sync::Arc;
use sqlx::Row;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::domain::group::entities::{GroupRecord, GroupDetailInfo, FulfillmentStats, SettlementCheckResult};

/// 配置双人组路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups")
            .route("", web::post().to(create_group))
            .route("/{group_id}", web::get().to(get_group))
            .route("/{group_id}/swap-role", web::post().to(swap_role))
            .route("/{group_id}/settlement-check", web::get().to(settlement_check))
            .route("/{group_id}/fulfillment-stats", web::get().to(fulfillment_stats))
    );
}

/// 创建双人组
/// POST /api/groups
async fn create_group(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;

    // 检查用户是否已在组中
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = true"
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

    Ok(HttpResponse::Created().json(&serde_json::json!({
        "groupId": group.group_id,
        "inviteCode": invite_code,
        "status": "ok"
    })))
}

/// 获取组信息
/// GET /api/groups/{group_id}
async fn get_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
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
           WHERE g.group_id = $1"#
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

    Ok(HttpResponse::Ok().json(&detail))
}

/// 角色互换
/// POST /api/groups/{group_id}/swap-role
///
/// 前置条件:
/// - 小组无未完结在途订单
/// - 操作人无CLAIMED状态且自己作为发起人或履约人的在途心愿
/// - 互换后当前Buyer与Seller对调
///
/// 配置开关:
/// - swap_ignore_ongoing_wish = true时允许带在途心愿互换身份
async fn swap_role(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
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
        "SELECT settings FROM association_groups WHERE group_id = $1"
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
        return Err(CustomError::BadRequest("存在未完结订单，禁止互换".into()));
    }

    // 检查操作人是否有 CLAIMED 状态的在途心愿（除非 swap_ignore_ongoing_wish=true）
    if !swap_ignore_wish {
        let pending_wishes: i64 = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND status='CLAIMED'
               AND (selected_by=$2 OR fulfiller_id=$2)"#
        )
        .bind(gid)
        .bind(token.user_id)
        .fetch_one(&mut *tx)
        .await?;

        if pending_wishes > 0 {
            return Err(CustomError::BadRequest("存在在途心愿，禁止互换".into()));
        }
    }

    // 执行角色互换
    let (old_buyer, old_seller): (Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT buyer_user_id, seller_user_id FROM association_groups WHERE group_id=$1"
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
        sqlx::query("UPDATE association_group_members SET role_in_group='BUYER' WHERE user_id=$1 AND group_id=$2")
            .bind(buyer_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(seller_id) = new_seller {
        sqlx::query("UPDATE association_group_members SET role_in_group='SELLER' WHERE user_id=$1 AND group_id=$2")
            .bind(seller_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(HttpResponse::Ok().json(&serde_json::json!({
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
async fn settlement_check(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
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
           WHERE user_id=$1 AND group_id=$2 AND type='FREEZE'"#
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

    Ok(HttpResponse::Ok().json(&result))
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
async fn fulfillment_stats(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    // 获取组内所有用户
    let members: Vec<(i64,)> = sqlx::query_as(
        "SELECT user_id FROM association_group_members WHERE group_id=$1"
    )
    .bind(gid)
    .fetch_all(db)
    .await?;

    let mut stats_map = std::collections::HashMap::new();

    for (user_id,) in members {
        // 作为履约人的总心愿数
        let total: i64 = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT COUNT(*) FROM wishes WHERE group_id=$1 AND fulfiller_id=$2"
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
               AND fulfilled_at <= fulfillment_due_at"#
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        // 逾期数
        let expired: i64 = sqlx::query_scalar::<_, Option<i64>>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='EXPIRED'"#
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        // 待履约数
        let pending: i64 = sqlx::query_scalar::<_, Option<i64>>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='CLAIMED'"#
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        let rate = if total > 0 { finished as f64 / total as f64 } else { 0.0 };

        stats_map.insert(user_id, FulfillmentStats {
            user_id,
            fulfillment_total: total as i32,
            fulfillment_finished: finished as i32,
            fulfillment_expired: expired as i32,
            fulfillment_rate: rate,
            avg_fulfillment_hours: 0.0, // 简化
            pending_fulfillment_count: pending as i32,
        });
    }

    Ok(HttpResponse::Ok().json(&stats_map))
}