// API - 双人组管理路由
// FSD.latest.md compliant endpoints

use chrono::Utc;
use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, ServiceConfig,
};
use ntex::web::guard;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::api::ws::get_connection_manager;
use crate::api::ws::messages::{
    WsEnvelope, WsGroupMemberChangeData, WsGroupMemberInfo,
};
use crate::config::AppState;
use crate::domain::group::entities::{
    FulfillmentStats, GroupDetailInfo, GroupRecord, SettlementCheckResult,
};
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::utils::response::ApiResponse;

/// 路由守卫: 检查动态段 `{group_id}` 是不是 i64 数字
/// 作用: `/api/groups/join`、`/api/groups/invite` 等字面量路径不会匹配到
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

/// 配置双人组路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 写法说明:ntex 2.1 中,`web::scope("/prefix").route("/{param}", ...)` 这种
    // 在 scope 内带动态路径参数的写法不会被路由命中(实测 0.4ms 404)。
    // 必须用 `web::resource("/prefix/{param}").route(...)` 写法,或把动态路由
    // 放在外部 resource(不在 scope 内)。本函数采用拆分写法:
    // - 静态路由用独立 resource(避免与 {group_id} 动态段冲突)
    // - 动态参数路由用独立 resource 挂在 cfg 上
    //
    // 重要: `/api/groups/join` 必须用 web::resource 单独挂,不能放进 scope。
    // 否则 ntex 2.1 会把 POST /api/groups/join 路由到 `/api/groups/{group_id}` 的
    // swap_role handler,被 RequireGroup 误判为 403 USER_NOT_IN_GROUP。
    //
    // 双保险: 即使静态路由因 ntex 内部原因没匹配上,动态资源上挂了
    // group_id_is_numeric guard,会拒绝匹配 "/api/groups/join" 这种非数字段
    cfg.service(
        web::resource("/api/groups/join")
            .route(web::post().to(join_group)),
    );
    cfg.service(
        web::resource("/api/groups")
            .route(web::post().to(create_group)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}")
            .guard(group_id_is_numeric())
            .route(web::get().to(get_group))
            .route(web::post().to(swap_role)),
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
    cfg.service(
        web::resource("/api/groups/{group_id}/exit")
            .route(web::post().to(exit_group)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/settlement-check")
            .route(web::get().to(settlement_check)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/members")
            .route(web::get().to(get_group_members)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/fulfillment-stats")
            .route(web::get().to(fulfillment_stats)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/invite")
            .route(web::post().to(create_invite)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/orders")
            .route(web::post().to(create_group_order)),
    );
    // 注:/api/groups/{group_id}/wishes 由 wishes 模块负责(POST + GET 都有)
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
    security(("bearer_auth" = []))
)]
async fn create_group(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;

    // 检查用户是否已在组中
    // 注意:is_primary 是 smallint(0/1),不能用 true/false
    // 必须同时过滤 member_status='ACTIVE',否则 LEFT 状态的旧记录也会被算成"已在组中"
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT group_id FROM association_group_members WHERE user_id = $1 AND is_primary = 1 AND member_status = 'ACTIVE'",
    )
    .bind(token.user_id)
    .fetch_optional(db)
    .await?;

    if existing.is_some() {
        return Err(CustomError::BadRequest("您已在组中".into()));
    }

    let mut tx = db.begin().await?;

    // 创建组
    // group_type / status 是 PG 自定义枚举,RETURNING 必须 ::text 强转,否则 sqlx 解不出
    // seller_user_id 留空:双人组是创建者+受邀者两人,创建者为 buyer,seller 位置等受邀者
    // 通过邀请码加入 (POST /api/groups/join) 时再填上
    let invite_code = format!("{:08x}", rand::random::<u32>());
    let group: GroupRecord = sqlx::query_as::<_, GroupRecord>(
        r#"INSERT INTO association_groups (group_name, group_type, status, invite_code, diamond, footprint_capacity, footprint_count, buyer_user_id, seller_user_id, level, exp, created_at, updated_at)
           VALUES ($1, 'PAIR', 'ACTIVE', $2, 0, 50, 0, $3, NULL, 1, 0, $4, $4)
           RETURNING group_id, group_name, group_type::text AS group_type, status::text AS status, invite_code, diamond, footprint_capacity, footprint_count, created_at, updated_at,
                     buyer_user_id, seller_user_id, level, exp, settings"#
    )
    .bind(format!("{}的组", token.user.as_ref().map(|u| u.username.as_str()).unwrap_or("用户")))
    .bind(&invite_code)
    .bind(token.user_id) // 创建者填入 buyer 位置
    .bind(Utc::now())
    .fetch_one(&mut *tx)
    .await?;

    // 将创建者加为组成员(is_primary 是 smallint,这里写 1 不用 true)
    sqlx::query(
        r#"INSERT INTO association_group_members (user_id, group_id, role_in_group, is_primary, joined_at)
           VALUES ($1, $2, 'ORDERING', 1, $3)"#
    )
    .bind(token.user_id)
    .bind(group.group_id)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 关键: 失效该用户的 UserPublic Redis 缓存,否则下一次请求 RequireGroup
    // 还会读到旧 group_id=None,继续返回 USER_NOT_IN_GROUP。
    // create_group 也是"是否在组里"状态的翻转点,必须清缓存。
    let _ = state.redis_cache
        .delete_user(&token.user_id.to_string())
        .await
        .map_err(|e| log::warn!("[create_group] failed to invalidate user cache: {}", e));

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
        // 关键: 同步更新 users.role,否则前端 userInfo.role 不会变(它来自 users 表)
        sqlx::query("UPDATE users SET role='ORDERING', last_role_switch_at=NOW() WHERE user_id=$1")
            .bind(buyer_id)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(seller_id) = new_seller {
        sqlx::query("UPDATE association_group_members SET role_in_group='RECEIVING' WHERE user_id=$1 AND group_id=$2")
            .bind(seller_id)
            .bind(gid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE users SET role='RECEIVING', last_role_switch_at=NOW() WHERE user_id=$1")
            .bind(seller_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    // 关键: 失效双方 UserPublic Redis 缓存
    // 否则下次 silentLogin 还会读到旧 users.role,前端看起来"没切换成功"
    for uid in [new_buyer, new_seller].into_iter().flatten() {
        let _ = state.redis_cache
            .delete_user(&uid.to_string())
            .await
            .map_err(|e| log::warn!("[swap_role] failed to invalidate user cache for {}: {}", uid, e));
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

    // 检查冻结爱心积分(SUM 返回 NUMERIC,::BIGINT 强转)
    let frozen_points: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COALESCE(SUM(amount), 0)::BIGINT FROM love_point_transactions
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

    Ok(ApiResponse::success(FulfillmentStatsListResponse { stats: stats_map }))
}

/// 履约统计列表响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentStatsListResponse {
    /// key: user_id; value: 该成员的履约统计
    pub stats: std::collections::HashMap<i64, FulfillmentStats>,
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
    security(("bearer_auth" = []))
)]
async fn create_invite(
    token: UserToken,
    _require: RequireGroup,
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
    // role_in_group 是 PG 自定义枚举,SELECT 必须 ::text 强转,否则 sqlx 解不出
    let user_role: Option<String> = sqlx::query_scalar(
        "SELECT role_in_group::text FROM association_group_members WHERE group_id=$1 AND user_id=$2",
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
        r#"INSERT INTO orders (order_id, group_id, type, user_id, assignee_id, creator_role_snapshot, status, title, content, deadline, created_at)
           VALUES ($1, $2, 'NORMAL', $3, $4, 'ORDERING', 'CREATED', $5, $6, $7, NOW())"#
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
    security(("bearer_auth" = []))
)]
async fn join_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    body: Json<JoinGroupInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let input = body.into_inner();
    // DEBUG: 诊断 join 流程到底走没走到 handler
    log::info!(
        "[join_group] ENTER user_id={} invite_code={} invite_link_group_id={:?}",
        token.user_id, input.invite_code, input.group_id
    );

    // 查找邀请码对应的邀请记录 (权威 group_id 来源)
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
        None => {
            log::warn!("[join_group] REJECT: invite_code={} 未找到 ACTIVE 记录", input.invite_code);
            return Err(CustomError::BadRequest("邀请码无效或已过期".into()));
        }
    };

    // 可选防御: 链接里带的 group_id 必须跟 invite_code 反查的 group_id 一致,
    // 不一致说明链接被篡改/拼接错, 直接拒绝。
    if let Some(link_gid) = input.group_id {
        if link_gid != group_id {
            log::warn!(
                "[join_group] REJECT: invite_link.group_id={} != invite_record.group_id={}",
                link_gid, group_id
            );
            return Err(CustomError::BadRequest("邀请码与群信息不匹配".into()));
        }
    }

    // 幂等: 用户已经在本 group 的 ACTIVE 成员里, 直接返回成功 (跳过 INSERT / 推送)
    let same_group_existing: Option<(i64,)> = sqlx::query_as(
        "SELECT group_id FROM association_group_members
         WHERE user_id = $1 AND group_id = $2 AND member_status = 'ACTIVE'"
    )
    .bind(token.user_id)
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    if same_group_existing.is_some() {
        log::info!(
            "[join_group] IDEMPOTENT user_id={} 已在 group_id={}, 直接返回成功",
            token.user_id, group_id
        );
        // 让前端拿到的 groupId 跟正常入群路径一致, 便于它清缓存/刷新
        let _ = state.redis_cache
            .delete_user(&token.user_id.to_string())
            .await
            .map_err(|e| log::warn!("[join_group] failed to invalidate user cache (idempotent): {}", e));
        return Ok(ApiResponse::success(serde_json::json!({
            "groupId": group_id,
            "role": "SELLER",
            "status": "ok"
        })));
    }

    // 用户已经在别的 group 的 ACTIVE 成员里, 拒绝 (业务规则)
    let other_group_existing: Option<(i64,)> = sqlx::query_as(
        "SELECT group_id FROM association_group_members
         WHERE user_id = $1 AND member_status = 'ACTIVE'"
    )
    .bind(token.user_id)
    .fetch_optional(db)
    .await?;

    if other_group_existing.is_some() {
        log::warn!(
            "[join_group] REJECT: user_id={} 已在其他组中 (existing={:?})",
            token.user_id, other_group_existing
        );
        return Err(CustomError::BadRequest("您已在其他组中".into()));
    }

    // 检查是否过期
    if Utc::now() > expires_at {
        log::warn!("[join_group] REJECT: invite_code={} 已过期 (expires_at={})", input.invite_code, expires_at);
        return Err(CustomError::BadRequest("邀请码已过期".into()));
    }

    // 检查使用次数
    if used_count >= max_uses {
        log::warn!("[join_group] REJECT: invite_code={} 已用满 (used={}/max={})", input.invite_code, used_count, max_uses);
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
        log::warn!("[join_group] REJECT: group_id={} 已满 (member_count={})", group_id, member_count);
        return Err(CustomError::BadRequest("组已满2人，无法加入".into()));
    }

    let mut tx = db.begin().await?;

    // 加入组成员(is_primary 是 smallint,这里写 0 不用 false)
    sqlx::query(
        r#"INSERT INTO association_group_members (user_id, group_id, role_in_group, is_primary, member_status, joined_at)
           VALUES ($1, $2, 'RECEIVING', 0, 'ACTIVE', $3)"#
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
    // DEBUG: 诊断日志
    log::info!("[join_group] SUCCESS user_id={} joined group_id={}", token.user_id, group_id);

    // 关键: 失效该用户的 UserPublic Redis 缓存,否则下一次请求 RequireGroup
    // 还会读到旧 group_id=None,继续返回 USER_NOT_IN_GROUP。
    // join 是用户"是否在组里"状态的翻转点,必须清缓存。
    let _ = state.redis_cache
        .delete_user(&token.user_id.to_string())
        .await
        .map_err(|e| log::warn!("[join_group] failed to invalidate user cache: {}", e));

    // 通知 group 里所有 ACTIVE 成员 —— 有人加入了
    // 推函数内部按 group_id 反查所有 ACTIVE 成员 (含 buyer + seller 双方),
    // 这里不需要再手撸 target Vec, 也不需要兜底分支
    // (tx 已 commit, association_group_members 已写入, 反查一定拿得到)
    push_group_member_change_notice(
        db,
        group_id,
        "joined",
        token.user_id, // actor = 加入者
        None,          // buyer 信息由 fetch_user_info_for_notice 按需查; 这里不强制带
        Some(token.user_id), // seller = 加入者(自己)
    )
    .await;

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
    let _ = state.redis_cache
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

    // 获取组成员列表(role_in_group 自定义枚举,::text 强转)
    let members = sqlx::query(
        r#"SELECT agm.user_id, agm.role_in_group::text AS role_in_group, agm.joined_at,
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

    // 检查冻结爱心积分
    // SUM 在 PostgreSQL 返回 NUMERIC,::BIGINT 强转才能解 i64
    let frozen_points: i64 = sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT COALESCE(SUM(amount)::BIGINT, 0) FROM love_point_transactions
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
/// `group_id` 来自邀请链接 (前端拼链接时带上), 后端仍以 invite_code 反查的
/// group_id 为准; group_id 仅用于日志/审计, 以及让前端"已在同群"幂等判断
/// 时的请求语义自洽。
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinGroupInput {
    pub invite_code: String,
    #[serde(default)]
    pub group_id: Option<i64>,
}

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
           WHERE g.group_id = $1 AND gm.member_status = 'ACTIVE'"#,
    )
    .bind(gid)
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| {
        log::error!("[swap_role_check] Q1 failed: gid={} user_id={} err={:?}", gid, user_id, e);
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
        _ => "SELLER".to_string(),  // unreachable in practice
    };
    let would_be_role = if current_role == "BUYER" {
        "SELLER".to_string()
    } else {
        "BUYER".to_string()
    };

    // Q2-Q4 并行
    let (active_orders, pending_wishes, frozen_points): (i64, i64, i64) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM orders
               WHERE group_id = $1
                 AND status NOT IN ('CONFIRMED_COMPLETED','COMPLETED',
                                    'CONFIRMED_INCOMPLETE','CONFIRMED_UNFINISHED',
                                    'REJECTED','CANCELLED','CANCELED',
                                    'TIMEOUT','SYSTEM_CLOSED','BREEDER_CLOSED')"#,
        )
        .bind(gid)
        .fetch_one(db),
        async {
            if ignore_ongoing_wish {
                return Ok::<i64, sqlx::Error>(0);
            }
            let n: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM wishes
                   WHERE group_id = $1 AND status = 'CLAIMED'
                     AND (selected_by = $2 OR fulfiller_id = $2)"#,
            )
            .bind(gid)
            .bind(user_id)
            .fetch_one(db)
            .await?;
            Ok(n)
        },
        sqlx::query_scalar::<_, i64>(
            // SUM 在 PostgreSQL 返回 NUMERIC,不是 BIGINT,直接解码 i64 会炸
            // 加 ::BIGINT 显式强转,COALESCE 在 SUM 为 NULL 时(理论上不会)回退到 0
            r#"SELECT COALESCE(SUM(amount)::BIGINT, 0) FROM love_point_transactions
               WHERE user_id = $1 AND group_id = $2 AND type = 'FREEZE'"#,
        )
        .bind(user_id)
        .bind(gid)
        .fetch_one(db),
    )
    .map_err(|e| {
        log::error!("[swap_role_check] Q2-Q4 failed: gid={} user_id={} err={:?}", gid, user_id, e);
        CustomError::from(e)
    })?;

    // 拼装 reasons
    let mut reasons: Vec<String> = Vec::new();
    if active_orders > 0 {
        reasons.push(format!("存在 {} 个未完结订单", active_orders));
    }
    if pending_wishes > 0 && !ignore_ongoing_wish {
        reasons.push(format!("存在 {} 个在途心愿", pending_wishes));
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
async fn fetch_user_info_for_notice(
    db: &sqlx::PgPool,
    user_id: i64,
) -> Option<WsGroupMemberInfo> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT nick_name, avatar FROM users WHERE user_id = $1",
    )
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
async fn push_group_member_change_notice(
    db: &sqlx::PgPool,
    group_id: i64,
    action: &str,
    actor_id: i64,
    buyer_id: Option<i64>,
    seller_id: Option<i64>,
) {
    let actor = fetch_user_info_for_notice(db, actor_id).await.unwrap_or(WsGroupMemberInfo {
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
         WHERE group_id = $1 AND member_status = 'ACTIVE'"
    )
    .bind(group_id)
    .fetch_all(db)
    .await
    .unwrap_or_else(|e| {
        log::error!(
            "[push_group_member_change] 查 group_id={} 成员失败: {:?}",
            group_id, e
        );
        Vec::new()
    });

    let manager = get_connection_manager();
    for target_id in target_ids {
        let sent = manager.send_to_user(target_id, &json).await;
        if !sent {
            log::debug!(
                "组员变化通知未送达(用户 {} 不在线): group_id={} action={}",
                target_id, group_id, action
            );
        }
    }
}
