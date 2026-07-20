// API - 双人组管理路由
// FSD.latest.md compliant endpoints

use ntex::web::guard;
use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::api::ws::get_connection_manager;
use crate::api::ws::messages::{WsEnvelope, WsGroupMemberChangeData, WsGroupMemberInfo};
use crate::config::AppState;
use crate::domain::group::entities::{FulfillmentStats, GroupDetailInfo, SettlementCheckResult};
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::middlewares::target_group::require_active_target_group_member;
use crate::utils::response::ApiResponse;

/// 路由守卫: 检查动态段 `{group_id}` 是不是 i64 数字
/// 作用: `/api/groups/partner-invitations` 等字面量路径不会匹配到
///       `/api/groups/{group_id}` 这条动态资源,避免被误路由到 swap_role 等
///       需要 RequireGroup 的 handler 而返回 403 USER_NOT_IN_GROUP
fn group_id_is_numeric() -> impl ntex::web::guard::Guard {
    guard::fn_guard(|head| {
        head.uri
            .path()
            .split('/')
            .nth(3) // ["", "api", "groups", "{group_id}", ...]
            .and_then(|s| s.parse::<i64>().ok())
            .is_some()
    })
}

fn resolve_frozen_points(
    stored_balance: Option<i64>,
    freeze_total: i64,
    unfreeze_total: i64,
) -> Option<i64> {
    match stored_balance {
        Some(balance) => (balance >= 0).then_some(balance),
        None => freeze_total
            .checked_sub(unfreeze_total)
            .filter(|balance| *balance >= 0),
    }
}

async fn read_frozen_points(
    db: &sqlx::PgPool,
    user_id: i64,
    group_id: i64,
) -> Result<i64, CustomError> {
    let stored_balance: Option<i64> = sqlx::query_scalar(
        r#"SELECT frozen_love_point
           FROM user_group_points
           WHERE user_id = $1 AND group_id = $2"#,
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    if stored_balance.is_some() {
        return resolve_frozen_points(stored_balance, 0, 0)
            .ok_or_else(|| CustomError::internal_error("冻结积分余额异常"));
    }

    // 兼容尚未建立 user_group_points 行的历史数据；解冻流水必须从冻结流水中扣除。
    let (freeze_total, unfreeze_total): (i64, i64) = sqlx::query_as(
        r#"SELECT
               COALESCE(
                   SUM(amount) FILTER (
                       WHERE type = 'FREEZE'::love_point_tx_type_enum
                   ),
                   0
               )::BIGINT,
               COALESCE(
                   SUM(amount) FILTER (
                       WHERE type = 'UNFREEZE'::love_point_tx_type_enum
                   ),
                   0
               )::BIGINT
           FROM love_point_transactions
           WHERE user_id = $1 AND group_id = $2"#,
    )
    .bind(user_id)
    .bind(group_id)
    .fetch_one(db)
    .await?;

    resolve_frozen_points(None, freeze_total, unfreeze_total)
        .ok_or_else(|| CustomError::internal_error("冻结积分流水聚合异常"))
}

/// 配置双人组路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 写法说明:ntex 2.1 中,`web::scope("/prefix").route("/{param}", ...)` 这种
    // 在 scope 内带动态路径参数的写法不会被路由命中(实测 0.4ms 404)。
    // 必须用 `web::resource("/prefix/{param}").route(...)` 写法,或把动态路由
    // 放在外部 resource(不在 scope 内)。本函数采用拆分写法:
    // - 静态路由用独立 resource(避免与 {group_id} 动态段冲突)
    // - 动态参数路由用独立 resource 挂在 cfg 上
    //
    // 伙伴绑定统一由 partner_invitations 模块处理；旧的裸 user_id join 路由不再注册。
    cfg.service(
        web::resource("/api/groups/{group_id}")
            .guard(group_id_is_numeric())
            .route(web::get().to(get_group))
            .route(web::post().to(swap_role))
            .route(web::patch().to(update_group)),
    );
    // 重要: utoipa::path 标注是 /api/groups/{group_id}/swap-role
    // (前端 openapi 自动生成的代码按这个调),但历史上 swap_role 实际挂在
    // POST /api/groups/{group_id} 上(无后缀),导致前端调过来 404。
    // 这里补一个带后缀的路由,与 openapi 标注对齐;旧的保留以防其他客户端在用。
    cfg.service(
        web::resource("/api/groups/{group_id}/swap-role")
            .guard(group_id_is_numeric())
            .route(web::post().to(swap_role)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/swap-role/check")
            .route(web::get().to(swap_role_check)),
    );
    cfg.service(web::resource("/api/groups/{group_id}/exit").route(web::post().to(exit_group)));
    cfg.service(
        web::resource("/api/groups/{group_id}/settlement-check")
            .route(web::get().to(settlement_check)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/members").route(web::get().to(get_group_members)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/fulfillment-stats")
            .route(web::get().to(fulfillment_stats)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/orders").route(web::post().to(create_group_order)),
    );
    // 注: 之前有 GET/PATCH /api/groups/{group_id}/point-config,允许组员调整积分配置。
    // MVP 阶段默认配置已合理,该入口用户基本不会用,且增加前后端维护成本。
    // 决定删除 (P3 业务清理 - 简化过度设计)。
    // 表 group_point_configs 保留 (数据兜底),SQL DEFAULT 仍生效。
    cfg.service(
        web::resource("/api/groups/{group_id}/name")
            .guard(group_id_is_numeric())
            .route(web::patch().to(update_group_name)),
    );
    // 注:/api/groups/{group_id}/wishes 由 wishes 模块负责(POST + GET 都有)
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
    security(("bearer_auth" = []))
)]
async fn get_group(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 获取组信息
    let group_row = sqlx::query(
        r#"SELECT g.group_id, g.group_name, g.group_type, g.status, g.invite_code, g.diamond,
                  g.footprint_capacity, g.footprint_count, g.created_at, g.updated_at,
                  g.buyer_user_id, g.seller_user_id, g.level, g.exp, g.settings, g.group_avatar,
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
        group_avatar: row.get("group_avatar"),
        level: row.get::<Option<i32>, _>("level").unwrap_or(1),
        exp: row.get::<Option<i64>, _>("exp").unwrap_or(0),
        diamond: row.get::<i32, _>("diamond") as i64,
        footprint_capacity: row.get("footprint_capacity"),
        footprint_count: row.get("footprint_count"),
        // 等级进度实时算 (不依赖 association_groups.level 字段)
        level_progress: Some(
            crate::application::group_level_service::GroupLevelService::compute_progress(
                db,
                row.get::<Option<i64>, _>("exp").unwrap_or(0),
            )
            .await?,
        ),
        // 好友做客邀请码:前端用这个拼分享链接 ?inviteCode=xxx&groupId=yyy
        invite_code: row.get("invite_code"),
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
    security(("bearer_auth" = []))
)]
async fn swap_role(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

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
    // 终态集合与 swap_role_check 对齐:把同义拼写(COMPLETED/CONFIRMED_COMPLETED、CANCELED/CANCELLED、CONFIRMED_UNFINISHED/CONFIRMED_INCOMPLETE)
    // 和 SYSTEM_CLOSED / BREEDER_CLOSED 都视为已关闭,避免预检和实操两个接口判定不一致
    let pending_orders: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM orders WHERE group_id=$1 AND status NOT IN (\
            'CONFIRMED_COMPLETED','COMPLETED',\
            'CONFIRMED_INCOMPLETE','CONFIRMED_UNFINISHED',\
            'REJECTED','CANCELLED','CANCELED',\
            'SYSTEM_CLOSED','BREEDER_CLOSED',\
            'TIMEOUT'\
        )",
    )
    .bind(gid)
    .fetch_one(&mut *tx)
    .await?;

    if pending_orders > 0 {
        return Err(CustomError::role_swap_blocked_by_order(
            "存在未完结订单，禁止互换",
        ));
    }

    // 协商中的心愿(DRAFT/NEGOTIATING)不允许切换 role：
    //   协商中的双方关系绑定在 (requester_id, fulfiller_id) 上,
    //   切换 role 会让 buyer/seller 颠倒,导致已有的心愿双方关系错乱。
    //   强制让用户先把当前协商的心愿处理完(同意/拒绝)再切。
    let negotiating_wishes: i64 = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM wishes
           WHERE group_id = $1 AND status IN ('DRAFT'::wish_status_enum, 'NEGOTIATING'::wish_status_enum)"#,
    )
    .bind(gid)
    .fetch_one(&mut *tx)
    .await?;
    if negotiating_wishes > 0 {
        return Err(CustomError::role_swap_blocked_by_wish(
            "存在协商中的心愿，请先处理（同意或拒绝）后再切换角色",
        ));
    }

    // 检查操作人是否有 CLAIMED 状态的在途心愿（除非 swap_ignore_ongoing_wish=true）
    if !swap_ignore_wish {
        let pending_wishes: i64 = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND status='CLAIMED'::wish_status_enum
               AND (selected_by=$2 OR fulfiller_id=$2)"#,
        )
        .bind(gid)
        .bind(token.user_id)
        .fetch_one(&mut *tx)
        .await?;

        if pending_wishes > 0 {
            return Err(CustomError::role_swap_blocked_by_wish(
                "存在在途心愿，禁止互换",
            ));
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
        sqlx::query("UPDATE association_group_members SET role_in_group = 'ORDERING'::group_member_role_enum WHERE user_id=$1 AND group_id=$2")
            .bind(buyer_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
        // 关键: 同步更新 users.role,否则前端 userInfo.role 不会变(它来自 users 表)
        sqlx::query("UPDATE users SET role='ORDERING'::user_role_enum, last_role_switch_at=NOW() WHERE user_id=$1")
            .bind(buyer_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(seller_id) = new_seller {
        sqlx::query("UPDATE association_group_members SET role_in_group = 'RECEIVING'::group_member_role_enum WHERE user_id=$1 AND group_id=$2")
            .bind(seller_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE users SET role='RECEIVING'::user_role_enum, last_role_switch_at=NOW() WHERE user_id=$1")
            .bind(seller_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // 关键: 失效双方 UserPublic Redis 缓存
    // 否则下次 silentLogin 还会读到旧 users.role,前端看起来"没切换成功"
    for uid in [new_buyer, new_seller].into_iter().flatten() {
        let _ = state
            .redis_cache
            .delete_user(&uid.to_string())
            .await
            .map_err(|e| {
                log::warn!(
                    "[swap_role] failed to invalidate user cache for {}: {}",
                    uid,
                    e
                )
            });
    }

    // 通知组里所有人 —— 角色互换了
    // 推函数内部按 group_id 反查所有 ACTIVE 成员, 这里不再手撸 target Vec
    push_group_member_change_notice(
        db,
        gid,
        "swapped",
        token.user_id, // actor = 发起互换的人
        new_buyer,
        new_seller,
    )
    .await;

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
    security(("bearer_auth" = []))
)]
async fn settlement_check(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

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

    // 优先使用当前冻结余额；无余额行时才按 FREEZE - UNFREEZE 回放历史流水。
    let frozen_points = read_frozen_points(db, token.user_id, gid).await?;

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
        (status = 200, description = "获取成功", body = FulfillmentStatsListResponse),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn fulfillment_stats(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

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
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='FINISHED'::wish_status_enum
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
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='EXPIRED'::wish_status_enum"#,
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await?
        .unwrap_or(0);

        // 待履约数
        let pending: i64 = sqlx::query_scalar::<_, Option<i64>>(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id=$1 AND fulfiller_id=$2 AND status='CLAIMED'::wish_status_enum"#,
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

    Ok(ApiResponse::success(FulfillmentStatsListResponse {
        stats: stats_map,
    }))
}

/// 履约统计列表响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentStatsListResponse {
    /// key: user_id; value: 该成员的履约统计
    pub stats: std::collections::HashMap<i64, FulfillmentStats>,
}

// ============== FSD v2 额外端点 ==============

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
    security(("bearer_auth" = []))
)]
async fn create_group_order(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    body: Json<GroupOrderInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let input = body.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 数据库存储的真实组内下单方角色是 ORDERING（BUYER 仅为前端业务称谓）。
    // role_in_group 是 PG 自定义枚举,SELECT 必须 ::text 强转,否则 sqlx 解不出
    let user_role: Option<String> = sqlx::query_scalar(
        r#"SELECT role_in_group::text
           FROM association_group_members
           WHERE group_id = $1
             AND user_id = $2
             AND member_status = 'ACTIVE'::group_member_status_enum"#,
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if user_role.as_deref() != Some("ORDERING") {
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
        r#"INSERT INTO orders (order_id, group_id, type, user_id, assignee_id, creator_role_snapshot, status, title, content, deadline, created_at)
           VALUES ($1, $2, 'NORMAL'::order_type_enum, $3, $4, 'ORDERING', 'CREATED'::order_status_enum, $5, $6, $7, NOW())"#
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
    security(("bearer_auth" = []))
)]
async fn create_group_wish(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
    body: Json<GroupWishInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let input = body.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 获取组内另一成员作为默认履约人
    let other_member: Option<i64> = sqlx::query_scalar(
        "SELECT user_id FROM association_group_members WHERE group_id=$1 AND user_id!=$2 LIMIT 1",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    let fulfiller_id = other_member.unwrap_or(0);

    // 获取用户当前角色快照(role_in_group 是自定义枚举,::text 强转)
    let user_role: Option<String> = sqlx::query_scalar(
        "SELECT role_in_group::text FROM association_group_members WHERE group_id=$1 AND user_id=$2",
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    // 创建心愿
    let wish_id = idgenerator::IdInstance::next_id();

    sqlx::query(
        r#"INSERT INTO wishes (wish_id, group_id, created_by, requester_id, fulfiller_id, creator_role_snapshot, wish_name, wish_cost, initial_cost, status, created_at)
           VALUES ($1, $2, $3, $3, $4, $5, $6, $7, $7, 'DRAFT'::wish_status_enum, NOW())"#
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
    security(("bearer_auth" = []))
)]
async fn exit_group(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 结清检查
    let settlement = settlement_check_impl(db, gid, token.user_id).await?;
    if !settlement.can_exit {
        return Err(CustomError::BadRequest(
            settlement.reasons.join("; ").into(),
        ));
    }

    let mut tx = db.begin().await?;

    // 获取用户角色(role_in_group 是自定义枚举,::text 强转)
    let user_role: Option<String> = sqlx::query_scalar(
        "SELECT role_in_group::text FROM association_group_members WHERE group_id=$1 AND user_id=$2"
    )
    .bind(gid)
    .bind(token.user_id)
    .fetch_one(&mut *tx)
    .await?;

    // 更新组成员状态为 LEFT
    sqlx::query(
        "UPDATE association_group_members SET member_status = 'LEFT'::group_member_status_enum WHERE group_id=$1 AND user_id=$2"
    )
    .bind(gid)
    .bind(token.user_id)
    .execute(&mut *tx)
    .await?;

    // 清空组的 buyer 或 seller 引用
    if user_role.as_deref() == Some("ORDERING") {
        sqlx::query(
            "UPDATE association_groups SET buyer_user_id=NULL, updated_at=NOW() WHERE group_id=$1",
        )
        .bind(gid)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE association_groups SET seller_user_id=NULL, updated_at=NOW() WHERE group_id=$1",
        )
        .bind(gid)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // 通知 group 里所有 ACTIVE 成员 —— 有人退出了
    // 推函数内部按 group_id 反查所有 ACTIVE 成员, 不需要手撸 target Vec
    // 退出后 group 里只剩另一个人(也可能没人了), 反查会自动跳过 actor
    let (remaining_buyer, remaining_seller): (Option<Option<i64>>, Option<Option<i64>>) = (
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT buyer_user_id FROM association_groups WHERE group_id = $1",
        )
        .bind(gid)
        .fetch_optional(db)
        .await
        .ok()
        .flatten(),
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT seller_user_id FROM association_groups WHERE group_id = $1",
        )
        .bind(gid)
        .fetch_optional(db)
        .await
        .ok()
        .flatten(),
    );
    let buyer_id_after = remaining_buyer.flatten();
    let seller_id_after = remaining_seller.flatten();

    push_group_member_change_notice(
        db,
        gid,
        "exited",
        token.user_id, // actor = 退出者
        buyer_id_after,
        seller_id_after,
    )
    .await;

    // 关键: 失效该用户的 UserPublic Redis 缓存,否则下一次请求 RequireGroup
    // 还会读到旧 group_id=Some(gid),继续允许访问旧组,或在某些边界下报错。
    // exit_group 也是"是否在组里"状态的翻转点,必须清缓存。
    let _ = state
        .redis_cache
        .delete_user(&token.user_id.to_string())
        .await
        .map_err(|e| log::warn!("[exit_group] failed to invalidate user cache: {}", e));

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
        (status = 200, description = "获取成功", body = Vec<GroupMemberOut>),
        (status = 403, description = "无权访问该组")
    ),
    security(("bearer_auth" = []))
)]
async fn get_group_members(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();

    require_active_target_group_member(db, token.user_id, gid).await?;

    // 获取组成员列表(role_in_group 自定义枚举,::text 强转)
    let members = sqlx::query(
        r#"SELECT agm.user_id, agm.role_in_group::text AS role_in_group, agm.joined_at,
                  u.nick_name, u.avatar,
                  COALESCE(ugp.available_love_point, 0) as available_love_point,
                  COALESCE(ugp.frozen_love_point, 0) as frozen_love_point
           FROM association_group_members agm
           JOIN users u ON u.user_id = agm.user_id
           LEFT JOIN user_group_points ugp ON ugp.user_id = agm.user_id AND ugp.group_id = agm.group_id
           WHERE agm.group_id = $1 AND agm.member_status = 'ACTIVE'::group_member_status_enum"#
    )
    .bind(gid)
    .fetch_all(db)
    .await?;

    let result: Vec<GroupMemberOut> = members
        .iter()
        .map(|r| GroupMemberOut {
            user_id: r.get("user_id"),
            nickname: r.get("nick_name"),
            avatar: r.get("avatar"),
            role: r.get("role_in_group"),
            love_point_available: r.get("available_love_point"),
            love_point_frozen: r.get("frozen_love_point"),
            joined_at: r.get("joined_at"),
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

    // 优先使用当前冻结余额；无余额行时才按 FREEZE - UNFREEZE 回放历史流水。
    let frozen_points = read_frozen_points(db, user_id, group_id).await?;

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

// 注: GroupPointConfigResponse / GroupPointConfigUpdateRequest 已被删除
// 配置入口 MVP 阶段不需要, group_point_configs 表保留, SQL DEFAULT 兜底
// (积分奖惩直接走表默认值, 不暴露配置 API)

// ============== Swap Role Check (FSD 2026-06-15 设计稿 §4) ==============

/// 组成员信息
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupMemberOut {
    pub user_id: i64,
    pub nickname: Option<String>,
    pub avatar: Option<String>,
    pub role: String,
    pub love_point_available: i64,
    pub love_point_frozen: i64,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

/// 角色互换前置检查响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwapRoleCheckResponse {
    pub can_swap: bool,
    pub reasons: Vec<String>,
    pub active_orders_count: i32,
    pub pending_wishes_count: i32,
    pub frozen_love_points: i64,
    pub pending_compensation: i32,
    pub pending_diamond_reward: i32,
    pub current_role: String,
    pub would_be_role: String,
    pub ignore_ongoing_wish_enabled: bool,
}

/// 角色互换前置检查
/// GET /api/groups/{group_id}/swap-role/check
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/swap-role/check",
    tag = "双人组",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "检查成功", body = SwapRoleCheckResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 404, description = "组不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
async fn swap_role_check(
    token: UserToken,
    _require: RequireGroup,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let gid = group_id.into_inner();
    let user_id = token.user_id;
    require_active_target_group_member(db, user_id, gid).await?;

    // Q1：取组信息 + 用户角色 + ignore 配置
    // role_in_group 自定义枚举,::text 强转
    // settings JSONB 用 COALESCE((...->>'key')::bool, false) 可能在 settings IS NULL 时
    // 因 null::bool 失败。改用 CASE WHEN 显式判断,避免 ::bool 在 NULL 上炸
    let row = sqlx::query(
        r#"SELECT
             g.buyer_user_id, g.seller_user_id,
             gm.role_in_group::text AS current_role,
             COALESCE(
               CASE g.settings->>'swap_ignore_ongoing_wish'
                 WHEN 'true' THEN true
                 WHEN 'false' THEN false
                 ELSE false
               END,
               false
             ) AS ignore_ongoing_wish
           FROM association_groups g
           JOIN association_group_members gm
             ON gm.group_id = g.group_id AND gm.user_id = $2
           WHERE g.group_id = $1 AND gm.member_status = 'ACTIVE'::group_member_status_enum"#,
    )
    .bind(gid)
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| {
        log::error!(
            "[swap_role_check] Q1 failed: gid={} user_id={} err={:?}",
            gid,
            user_id,
            e
        );
        CustomError::from(e)
    })?;

    let row = match row {
        Some(r) => r,
        None => {
            let group_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM association_groups WHERE group_id = $1)",
            )
            .bind(gid)
            .fetch_one(db)
            .await?;

            if !group_exists {
                return Err(CustomError::NotFound("组不存在".into()));
            }
            return Err(CustomError::Forbidden("非组成员".into()));
        }
    };

    let buyer_user_id: i64 = row.get("buyer_user_id");
    let seller_user_id: i64 = row.get("seller_user_id");

    // 仅 buyer_user_id / seller_user_id 两位成员能互换角色。
    // group_member_role_enum 还含 ADMIN,但 ADMIN 不可触发 swap,POST 端会 403。
    // 这里短路掉,避免给出 can_swap=true 但实际 POST 失败的误导。
    if user_id != buyer_user_id && user_id != seller_user_id {
        return Err(CustomError::Forbidden("仅 buyer/seller 可互换角色".into()));
    }

    let ignore_ongoing_wish: bool = row.get("ignore_ongoing_wish");
    // Translate DB enum (ORDERING / RECEIVING) into the business-friendly
    // BUYER / SELLER names that the frontend expects. Anything else
    // (e.g. ADMIN) should not reach this point because the 403 above
    // already filtered out non-buyer/seller members.
    let current_role = match row.get::<String, _>("current_role").as_str() {
        "ORDERING" => "BUYER".to_string(),
        "RECEIVING" => "SELLER".to_string(),
        _ => "SELLER".to_string(), // unreachable in practice
    };
    let would_be_role = if current_role == "BUYER" {
        "SELLER".to_string()
    } else {
        "BUYER".to_string()
    };

    // Q2-Q3 并行
    // 终态集合与 swap_role 实际接口对齐 —— 避免预检通过但实际被拦的体验割裂
    let (active_orders, pending_wishes): (i64, i64) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status NOT IN ('CONFIRMED_COMPLETED','COMPLETED',
                                    'CONFIRMED_INCOMPLETE','CONFIRMED_UNFINISHED',
                                    'REJECTED','CANCELLED','CANCELED',
                                    'SYSTEM_CLOSED','BREEDER_CLOSED','TIMEOUT')"#,
        )
        .bind(gid)
        .fetch_one(db),
        async {
            if ignore_ongoing_wish {
                return Ok::<i64, sqlx::Error>(0);
            }
            // 与 swap_role 对齐:既要查「协商中」(全组 DRAFT/NEGOTIATING),
            // 也要查「在途心愿」(自己作为 selected_by/fulfiller_id 的 CLAIMED)
            let n_negotiating: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM wishes
                   WHERE group_id = $1 AND status IN ('DRAFT'::wish_status_enum, 'NEGOTIATING'::wish_status_enum)"#,
            )
            .bind(gid)
            .fetch_one(db)
            .await?;
            let n_claimed: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM wishes
                   WHERE group_id = $1 AND status = 'CLAIMED'::wish_status_enum
                     AND (selected_by = $2 OR fulfiller_id = $2)"#,
            )
            .bind(gid)
            .bind(user_id)
            .fetch_one(db)
            .await?;
            Ok(n_negotiating + n_claimed)
        },
    )
    .map_err(|e| {
        log::error!("[swap_role_check] Q2-Q3 failed: gid={} user_id={} err={:?}", gid, user_id, e);
        CustomError::from(e)
    })?;
    let frozen_points = read_frozen_points(db, user_id, gid).await.map_err(|e| {
        log::error!(
            "[swap_role_check] frozen points failed: gid={} user_id={} err={:?}",
            gid,
            user_id,
            e
        );
        e
    })?;

    // 拼装 reasons
    let mut reasons: Vec<String> = Vec::new();
    if active_orders > 0 {
        reasons.push(format!("存在 {} 个未完结清单", active_orders));
    }
    if pending_wishes > 0 && !ignore_ongoing_wish {
        // pending_wishes 已经合并了「协商中」+「在途」两类,文案分情况说明
        let negotiating_n: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id = $1 AND status IN ('DRAFT'::wish_status_enum, 'NEGOTIATING'::wish_status_enum)"#,
        )
        .bind(gid)
        .fetch_one(db)
        .await
        .unwrap_or(0);
        let claimed_n: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM wishes
               WHERE group_id = $1 AND status = 'CLAIMED'::wish_status_enum
                 AND (selected_by = $2 OR fulfiller_id = $2)"#,
        )
        .bind(gid)
        .bind(user_id)
        .fetch_one(db)
        .await
        .unwrap_or(0);
        if negotiating_n > 0 {
            reasons.push(format!(
                "存在 {} 个协商中的心愿(请先同意或拒绝)",
                negotiating_n
            ));
        }
        if claimed_n > 0 {
            reasons.push(format!("存在 {} 个在途心愿(已兑换未确认完成)", claimed_n));
        }
    }
    if frozen_points > 0 {
        reasons.push(format!("有 {} 冻结积分未处理", frozen_points));
    }

    let can_swap = reasons.is_empty();

    Ok(ApiResponse::success(SwapRoleCheckResponse {
        can_swap,
        reasons,
        active_orders_count: active_orders as i32,
        pending_wishes_count: pending_wishes as i32,
        frozen_love_points: frozen_points,
        pending_compensation: 0,
        pending_diamond_reward: 0,
        current_role,
        would_be_role,
        ignore_ongoing_wish_enabled: ignore_ongoing_wish,
    }))
}

// ============== 组员变化通知(服务端 -> 客户端 WebSocket) ==============

/// 查用户的昵称和头像(用于通知里展示)。
/// 查不到时返回 None,调用方决定要不要兜底。
async fn fetch_user_info_for_notice(db: &sqlx::PgPool, user_id: i64) -> Option<WsGroupMemberInfo> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT nick_name, avatar FROM users WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();

    row.map(|(nick_name, avatar)| WsGroupMemberInfo {
        user_id,
        nick_name,
        avatar,
    })
}

/// 推送"组员变化"通知给 group 里**所有 ACTIVE 成员**。
///
/// - `group_id`: 从 association_group_members 反查所有 ACTIVE 成员后广播
/// - `action` / `actor_id` / `buyer_id` / `seller_id`:
///   详见 `WsGroupMemberChangeData`
///
/// 行为:target 不在线就静默丢弃(业务侧说"暂时不管离线")
///
/// 设计:不再让调用方手撸"目标 user_id Vec", 而是按 group_id 反查。
/// PAIR 组目前 2 人, FUTURE 多人组 (FAMILY/TEAM) 也能直接复用。
pub(super) async fn push_group_member_change_notice(
    db: &sqlx::PgPool,
    group_id: i64,
    action: &str,
    actor_id: i64,
    buyer_id: Option<i64>,
    seller_id: Option<i64>,
) {
    let actor = fetch_user_info_for_notice(db, actor_id)
        .await
        .unwrap_or(WsGroupMemberInfo {
            user_id: actor_id,
            nick_name: None,
            avatar: None,
        });
    let buyer = match buyer_id {
        Some(id) => fetch_user_info_for_notice(db, id).await,
        None => None,
    };
    let seller = match seller_id {
        Some(id) => fetch_user_info_for_notice(db, id).await,
        None => None,
    };

    let payload = WsGroupMemberChangeData {
        group_id,
        action: action.to_string(),
        actor,
        buyer,
        seller,
    };

    let envelope = WsEnvelope::group_member_change(&payload);
    let Ok(json) = serde_json::to_string(&envelope) else {
        log::error!("组员变化通知序列化失败: {:?}", payload);
        return;
    };

    // 按 group_id 反查所有 ACTIVE 成员 -> 全体推送
    // 之前手撸 buyer/seller 两人 Vec, 会漏掉多人组成员/以及新加进来但还没
    // 落到 association_groups.buyer/seller 字段的瞬间状态。
    let target_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT user_id FROM association_group_members
         WHERE group_id = $1 AND member_status = 'ACTIVE'::group_member_status_enum",
    )
    .bind(group_id)
    .fetch_all(db)
    .await
    .unwrap_or_else(|e| {
        log::error!(
            "[push_group_member_change] 查 group_id={} 成员失败: {:?}",
            group_id,
            e
        );
        Vec::new()
    });

    let manager = get_connection_manager();
    for target_id in target_ids {
        let sent = manager.send_to_user(target_id, &json).await;
        if !sent {
            log::debug!(
                "组员变化通知未送达(用户 {} 不在线): group_id={} action={}",
                target_id,
                group_id,
                action
            );
        }
    }
}

// ============== Group Name (改组名) HTTP 路由 ==============

/// 改组名请求体
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupNameRequest {
    pub name: String,
}

/// 校验组名: 1-10 位中英文 / 数字 / 下划线
///
/// 跟前端 group-name 组件的正则保持一致:
/// `/^[一-龥a-zA-Z0-9_]{1,10}$/`
pub fn validate_group_name(name: &str) -> Result<(), CustomError> {
    if name.is_empty() {
        return Err(CustomError::BadRequest("组名不能为空".into()));
    }
    if name.chars().count() > 10 {
        return Err(CustomError::BadRequest("组名不能超过 10 个字符".into()));
    }
    let mut ok = true;
    for c in name.chars() {
        let is_chinese = '\u{4e00}' <= c && c <= '\u{9fa5}';
        let is_ascii_alnum = c.is_ascii_alphanumeric();
        let is_underscore = c == '_';
        if !(is_chinese || is_ascii_alnum || is_underscore) {
            ok = false;
            break;
        }
    }
    if !ok {
        return Err(CustomError::BadRequest(
            "组名只能包含中英文、数字、下划线".into(),
        ));
    }
    Ok(())
}

/// 校验组头像 URL：必须是 https:// 开头、非空、最长 512 字符
///
/// 与前端 upload-custom 上传组件返回的 Qiniu key 拼 BASE_URL 后的格式保持一致：
/// `${BASE_URL}${key}`，BASE_URL 是 https:// 开头。
pub fn validate_group_avatar_url(url: &str) -> Result<(), CustomError> {
    if url.is_empty() {
        return Err(CustomError::BadRequest("头像 URL 不能为空".into()));
    }
    if url.len() > 512 {
        return Err(CustomError::BadRequest("头像 URL 过长（>512 字符）".into()));
    }
    if !url.starts_with("https://") {
        return Err(CustomError::BadRequest(
            "头像 URL 必须以 https:// 开头".into(),
        ));
    }
    Ok(())
}

/// 更新组名响应体
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupNameResponse {
    pub group_id: i64,
    pub group_name: String,
}

/// 更新组名
/// PATCH /api/groups/{group_id}/name
///
/// 鉴权: 当前用户必须是该 group 的 ACTIVE 成员(任意一方都能改)
/// 业务上不广播 ws(改组名是低频动作, 对方下次进厨房页下拉刷新即可)
/// 返回: 仅 { group_id, group_name },前端拿到后再单独调 get_group 拉完整信息
#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}/name",
    tag = "双人组",
    params(("group_id" = i64, Path, description = "组ID")),
    request_body = UpdateGroupNameRequest,
    responses(
        (status = 200, description = "更新成功", body = UpdateGroupNameResponse),
        (status = 400, description = "组名格式错误"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_group_name(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    body: Json<UpdateGroupNameRequest>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    // 校验组名
    validate_group_name(&input.name)?;

    require_active_target_group_member(db, token.user_id, group_id).await?;

    // 更新 group_name
    let affected: u64 = sqlx::query(
        "UPDATE association_groups SET group_name = $1, updated_at = NOW()
         WHERE group_id = $2",
    )
    .bind(&input.name)
    .bind(group_id)
    .execute(db)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(CustomError::NotFound("组不存在".into()));
    }

    Ok(ApiResponse::success(UpdateGroupNameResponse {
        group_id,
        group_name: input.name,
    }))
}

// ============== Group Generic Update (name + avatar_url) ==============

/// 通用更新组信息请求：所有字段可选，None 跳过（局部更新）
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupRequest {
    /// 新组名（可选，1-10 位中英文/数字/下划线）
    pub name: Option<String>,
    /// 新头像 URL（可选，https:// 开头、最长 512 字符）
    pub avatar_url: Option<String>,
}

/// 通用更新组信息响应：返回最新 group_name + group_avatar
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupResponse {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub group_avatar: Option<String>,
}

/// 通用更新组信息
/// PATCH /api/groups/{group_id}
///
/// 鉴权：当前用户必须是该 group 的 ACTIVE 成员
/// 行为：所有字段都是 Option；至少传一个；None 跳过；只 UPDATE 提供的字段
#[utoipa::path(
    patch,
    path = "/api/groups/{group_id}",
    tag = "双人组",
    params(("group_id" = i64, Path, description = "组ID")),
    request_body = UpdateGroupRequest,
    responses(
        (status = 200, description = "更新成功", body = UpdateGroupResponse),
        (status = 400, description = "参数错误或未提供任何字段"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 404, description = "组不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_group(
    state: State<Arc<AppState>>,
    token: UserToken,
    path: Path<i64>,
    body: Json<UpdateGroupRequest>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let input = body.into_inner();
    let db = &state.db_pool;

    // 至少提供一个字段
    if input.name.is_none() && input.avatar_url.is_none() {
        return Err(CustomError::BadRequest("至少需要修改一个字段".into()));
    }

    // 校验（如果提供了）
    if let Some(ref n) = input.name {
        validate_group_name(n)?;
    }
    if let Some(ref url) = input.avatar_url {
        validate_group_avatar_url(url)?;
    }

    require_active_target_group_member(db, token.user_id, group_id).await?;

    // 局部更新：用 COALESCE 让 None 跳过
    let updated: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"UPDATE association_groups
           SET group_name   = COALESCE($2, group_name),
               group_avatar = COALESCE($3, group_avatar),
               updated_at   = NOW()
           WHERE group_id = $1
           RETURNING group_name, group_avatar"#,
    )
    .bind(group_id)
    .bind(&input.name)
    .bind(&input.avatar_url)
    .fetch_optional(db)
    .await?;

    let (new_name, new_avatar) = match updated {
        Some(row) => row,
        None => return Err(CustomError::NotFound("组不存在".into())),
    };

    Ok(ApiResponse::success(UpdateGroupResponse {
        group_id,
        group_name: new_name,
        group_avatar: new_avatar,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_aggregation_prefers_current_balance_over_historical_freezes() {
        assert_eq!(resolve_frozen_points(Some(0), 500, 0), Some(0));
        assert_eq!(resolve_frozen_points(Some(25), 500, 500), Some(25));
    }

    #[test]
    fn frozen_aggregation_subtracts_unfreeze_history() {
        assert_eq!(resolve_frozen_points(None, 500, 500), Some(0));
        assert_eq!(resolve_frozen_points(None, 500, 125), Some(375));
        assert_eq!(resolve_frozen_points(None, 100, 125), None);
    }

    #[test]
    fn validate_group_name_ok_chinese() {
        assert!(validate_group_name("开心厨房").is_ok());
    }

    #[test]
    fn validate_group_name_ok_english_underscore() {
        assert!(validate_group_name("My_Kit_1").is_ok());
    }

    #[test]
    fn validate_group_name_ok_max_len() {
        assert!(validate_group_name("一二三四五六七八九十").is_ok());
    }

    #[test]
    fn validate_group_name_empty() {
        assert!(validate_group_name("").is_err());
    }

    #[test]
    fn validate_group_name_too_long() {
        assert!(validate_group_name("一二三四五六七八九十X").is_err());
    }

    #[test]
    fn validate_group_name_special_char() {
        assert!(validate_group_name("hi@you").is_err());
    }

    #[test]
    fn validate_group_name_space() {
        assert!(validate_group_name("hi you").is_err());
    }

    #[test]
    fn validate_group_avatar_url_ok() {
        assert!(validate_group_avatar_url("https://cdn.example.com/group/abc.jpg").is_ok());
    }

    #[test]
    fn validate_group_avatar_url_empty() {
        assert!(validate_group_avatar_url("").is_err());
    }

    #[test]
    fn validate_group_avatar_url_http() {
        assert!(validate_group_avatar_url("http://insecure.example.com/x.jpg").is_err());
    }

    #[test]
    fn validate_group_avatar_url_too_long() {
        let long = format!("https://x.example.com/{}", "a".repeat(600));
        assert!(validate_group_avatar_url(&long).is_err());
    }

    #[test]
    fn validate_group_avatar_url_relative() {
        assert!(validate_group_avatar_url("/local/path/img.jpg").is_err());
    }
}
