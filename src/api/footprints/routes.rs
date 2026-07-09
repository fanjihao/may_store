// API - 足迹路由
// FSD.latest.md compliant - 组内足迹、纪念内容、图片
// FSD v2: 路径为 /api/groups/{group_id}/footprints

use ntex::web::{
    self,
    types::{Json, Path, Query, State}, Responder, ServiceConfig,
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

/// 从 global_configs 读整数配置
///
/// 读不到 (DB 无记录 / 配置值不是整数) 时返回传入的默认值。
/// 与 admin/routes.rs::read_int 行为对齐, 但每请求走一次, 没有缓存。
async fn read_global_int(
    db: &sqlx::PgPool,
    key: &str,
    default: i32,
) -> i32 {
    sqlx::query_as::<_, (Option<serde_json::Value>,)>(
        "SELECT config_value FROM global_configs WHERE config_key = $1",
    )
    .bind(key)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v)
    .and_then(|v| v.as_i64().map(|n| n as i32))
    .unwrap_or(default)
}

/// 配置足迹路由
/// FSD v2: 路径为 /api/groups/{group_id}/footprints
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/footprints")
            .route(web::get().to(list_footprints))
            .route(web::post().to(create_footprint)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/footprints/{footprint_id}")
            .route(web::delete().to(delete_footprint)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/footprints/capacity/expand")
            .route(web::post().to(expand_capacity)),
    );
    // 2026-07-06 新增: 列出"本组可选的足迹分组" (公用分组 + 本组自建)
    cfg.service(
        web::resource("/api/groups/{group_id}/footprint-groups")
            .route(web::get().to(list_available_groups)),
    );
}

// ========== 响应结构 ==========

/// 足迹项 (FSD v2 10.2)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintItem {
    pub footprint_id: i64,
    pub user_id: i64,
    pub user_nickname: Option<String>,
    pub user_avatar: Option<String>,
    pub content: String,
    pub location: Option<String>,
    pub images: Option<serde_json::Value>,
    pub related_order_id: Option<i64>,
    pub related_wish_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 足迹列表响应
#[derive(Debug, Serialize, ToSchema)]
pub struct FootprintsListResponse {
    pub footprints: Vec<FootprintItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total_count: Option<i64>,
    pub capacity: Option<i32>,
    /// 2026-07-08 新增: 扩容单价 (从 global_configs.footprintExpandDiamondCost 读)
    /// 前端用这个显示"扩容 1 格 = X 钻石"; 用户点扩容按钮时会再拉一次拿最新值
    /// 不用 skip_serializing_if: 该字段后端必返回 (Some), 让前端始终拿到字段
    #[serde(rename = "costPerSlot")]
    pub cost_per_slot: Option<i32>,
    /// 2026-07-08 新增: 组钻石余额 (扩容量要用)
    /// 之前 `loadOverview` 写的是 hardcoded 0, 导致扩容按钮永远显示 0 钻石
    #[serde(rename = "diamondBalance")]
    pub diamond_balance: Option<i64>,
}

/// 发布足迹响应 (FSD v2 10.1)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFootprintResponse {
    pub footprint_id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub content: String,
    pub location: Option<String>,
    pub images: Option<serde_json::Value>,
    pub related_order_id: Option<i64>,
    pub related_wish_id: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 扩容容量响应 (FSD v2 10.4)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpandCapacityResponse {
    pub group_id: i64,
    pub old_capacity: i32,
    pub new_capacity: i32,
    pub diamond_cost: i32,
    pub diamond_balance_after: i32,
}

// ========== 请求结构 ==========

/// 发布足迹请求 (FSD v2 10.1)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFootprintRequest {
    pub content: String,
    pub location: Option<String>,
    pub images: Option<Vec<ImageItem>>,
    pub related_order_id: Option<i64>,
    pub related_wish_id: Option<i64>,
    /// 2026-07-06 新增: 所属足迹分组 (公用 = is_global, 或后续组内自建)
    pub record_group_id: Option<i64>,
    pub idempotency_key: String,
}

/// 图片项
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ImageItem {
    pub url: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// 扩容容量请求 (FSD v2 10.4)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpandCapacityRequest {
    pub expand_by: i32,
    /// 幂等键 (防止用户双击/重复点击导致重复扣钻)
    /// 2026-07-09 改: 改成 Option<String>, 允许前端不传
    ///   (后端实际**未实现**幂等检查, 字段仅做兼容保留)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// 足迹列表查询参数 (FSD v2 10.2)
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FootprintsQuery {
    pub cursor: Option<String>,
    pub limit: Option<i32>,
    pub user_id: Option<i64>,
}

// ========== 处理器 ==========

/// 发布足迹
/// POST /api/groups/{group_id}/footprints
/// FSD v2 10.1
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/footprints",
    tag = "足迹",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = CreateFootprintRequest,
    responses(
        (status = 201, description = "发布成功", body = CreateFootprintResponse),
        (status = 400, description = "参数错误或容量已满"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_footprint(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: Path<i64>,
    body: Json<CreateFootprintRequest>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let input = body.into_inner();
    let db = &state.db_pool;
    let user_id = token.user_id;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 检查容量
    // - 当前足迹总数 (用子查询避免 LEFT JOIN + COUNT 的 GROUP BY 报错)
    // - 组容量: 先看 association_groups.footprint_capacity, 没设就用 global_configs.defaultFootprintCapacity
    let (current_count, capacity): (i64, i32) = sqlx::query_as(
        r#"SELECT
              (SELECT COUNT(*) FROM footprints WHERE group_id = $1) AS total,
              COALESCE(
                  g.footprint_capacity,
                  (SELECT (config_value #>> '{}')::int FROM global_configs WHERE config_key='defaultFootprintCapacity'),
                  50
              ) AS capacity
           FROM association_groups g
           WHERE g.group_id = $1"#,
    )
    .bind(gid)
    .fetch_one(db)
    .await?;

    if current_count >= capacity as i64 {
        return Err(CustomError::BadRequest("足迹容量已满".into()));
    }

    // 验证 content 长度
    if input.content.chars().count() > 500 {
        return Err(CustomError::BadRequest("内容最多500字符".into()));
    }

    // 验证图片数量
    if let Some(ref images) = input.images {
        if images.len() > 9 {
            return Err(CustomError::BadRequest("最多9张图片".into()));
        }
    }

    // images 字段转 serde_json::Value 即可, sqlx + PostgreSQL 自动序列化为 jsonb
    // (之前先 to_string 再 bind 会报 "字段类型 jsonb 但表达式为 text", 必须显式 ::jsonb 强转)
    let images_value: Option<serde_json::Value> = input
        .images
        .as_ref()
        .map(|imgs| serde_json::to_value(imgs).unwrap_or_else(|_| serde_json::json!([])));

    // 2026-07-06: 如果传了 record_group_id, 校验它存在 + 状态正常
    //   - is_global=true 时 group_id=NULL, 任何组都能用
    //   - is_global=false 时 group_id 必须等于本组的 gid
    if let Some(rg_id) = input.record_group_id {
        let row: Option<(bool, Option<i64>, i16)> = sqlx::query_as(
            "SELECT is_global, group_id, status FROM record_group WHERE id=$1"
        )
        .bind(rg_id)
        .fetch_optional(db)
        .await?;

        match row {
            None => return Err(CustomError::BadRequest("足迹分组不存在".into())),
            Some((true, _, 0)) => {
                return Err(CustomError::BadRequest("该足迹分组已停用".into()));
            }
            Some((false, Some(rg_gid), 0)) if rg_gid == gid => {
                return Err(CustomError::BadRequest("该足迹分组已停用".into()));
            }
            Some((false, Some(rg_gid), _) ) if rg_gid != gid => {
                return Err(CustomError::BadRequest("该足迹分组不属于本组".into()));
            }
            _ => {} // OK
        }
    }

    // 插入足迹记录
    let footprint_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO footprints (group_id, user_id, content, location, images,
                               related_order_id, related_wish_id, record_group_id, idempotency_key, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
        RETURNING footprint_id
        "#,
    )
    .bind(gid)
    .bind(user_id)
    .bind(&input.content)
    .bind(&input.location)
    .bind(images_value)
    .bind(input.related_order_id)
    .bind(input.related_wish_id)
    .bind(input.record_group_id)
    .bind(&input.idempotency_key)
    .fetch_one(db)
    .await?;

    Ok(ApiResponse::success(CreateFootprintResponse {
        footprint_id,
        group_id: gid,
        user_id,
        content: input.content,
        location: input.location,
        images: input.images.map(|imgs| {
            serde_json::json!(imgs
                .iter()
                .map(|i| serde_json::json!({
                    "url": i.url,
                    "width": i.width,
                    "height": i.height
                }))
                .collect::<Vec<_>>())
        }),
        related_order_id: input.related_order_id,
        related_wish_id: input.related_wish_id,
        created_at: chrono::Utc::now(),
    }))
}

/// 获取足迹列表
/// GET /api/groups/{group_id}/footprints
/// FSD v2 10.2
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/footprints",
    tag = "足迹",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        FootprintsQuery
    ),
    responses(
        (status = 200, description = "获取成功", body = FootprintsListResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_footprints(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: Path<i64>,
    query: Query<FootprintsQuery>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let db = &state.db_pool;
    let user_id = token.user_id;
    let limit = query.limit.unwrap_or(20).min(100);

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 参数化查询:user_id 与 cursor 都走占位符,cursor 必须是数字
    let cursor_id: Option<i64> = match query.cursor.as_deref() {
        Some(c) => Some(
            c.parse::<i64>()
                .map_err(|_| CustomError::BadRequest("cursor 必须是整数足迹ID".into()))?,
        ),
        None => None,
    };

    // 4 种组合 (user_id × cursor) 全部参数化
    let rows = match (query.user_id, cursor_id) {
        (Some(uid), Some(cid)) => sqlx::query(
            r#"
            SELECT f.footprint_id, f.user_id, f.content, f.location, f.images,
                   f.related_order_id, f.related_wish_id, f.created_at,
                   u.nick_name as user_nickname, u.avatar as user_avatar
            FROM footprints f
            JOIN users u ON u.user_id = f.user_id
            WHERE f.group_id = $1 AND f.user_id = $2 AND f.footprint_id < $3
            ORDER BY f.footprint_id DESC
            LIMIT $4
            "#,
        )
        .bind(gid)
        .bind(uid)
        .bind(cid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
        (Some(uid), None) => sqlx::query(
            r#"
            SELECT f.footprint_id, f.user_id, f.content, f.location, f.images,
                   f.related_order_id, f.related_wish_id, f.created_at,
                   u.nick_name as user_nickname, u.avatar as user_avatar
            FROM footprints f
            JOIN users u ON u.user_id = f.user_id
            WHERE f.group_id = $1 AND f.user_id = $2
            ORDER BY f.footprint_id DESC
            LIMIT $3
            "#,
        )
        .bind(gid)
        .bind(uid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
        (None, Some(cid)) => sqlx::query(
            r#"
            SELECT f.footprint_id, f.user_id, f.content, f.location, f.images,
                   f.related_order_id, f.related_wish_id, f.created_at,
                   u.nick_name as user_nickname, u.avatar as user_avatar
            FROM footprints f
            JOIN users u ON u.user_id = f.user_id
            WHERE f.group_id = $1 AND f.footprint_id < $2
            ORDER BY f.footprint_id DESC
            LIMIT $3
            "#,
        )
        .bind(gid)
        .bind(cid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
        (None, None) => sqlx::query(
            r#"
            SELECT f.footprint_id, f.user_id, f.content, f.location, f.images,
                   f.related_order_id, f.related_wish_id, f.created_at,
                   u.nick_name as user_nickname, u.avatar as user_avatar
            FROM footprints f
            JOIN users u ON u.user_id = f.user_id
            WHERE f.group_id = $1
            ORDER BY f.footprint_id DESC
            LIMIT $2
            "#,
        )
        .bind(gid)
        .bind(limit + 1)
        .fetch_all(db)
        .await?,
    };

    let has_more = rows.len() > limit as usize;

    let footprints: Vec<FootprintItem> = rows
        .iter()
        .take(limit as usize)
        .map(|r| {
            let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
            FootprintItem {
                footprint_id: r.get("footprint_id"),
                user_id: r.get("user_id"),
                user_nickname: r.get("user_nickname"),
                user_avatar: r.get("user_avatar"),
                content: r.get("content"),
                location: r.get("location"),
                images: r.get("images"),
                related_order_id: r.get("related_order_id"),
                related_wish_id: r.get("related_wish_id"),
                created_at,
            }
        })
        .collect();

    let next_cursor = if has_more {
        footprints.last().map(|f| f.footprint_id.to_string())
    } else {
        None
    };

    // 获取总数和容量
    // - count 走单表 COUNT (避免 LEFT JOIN + COUNT 的 GROUP BY 报错)
    // - capacity 优先读 association_groups.footprint_capacity, 没设就 fallback 到
    //   global_configs.defaultFootprintCapacity, 最后兜底 50
    //   - count 是 BIGINT,footprint_capacity 是 INT —— ::INT 强转保持 i32 类型
    let (total_count, capacity): (i64, i32) = sqlx::query_as(
        r#"SELECT
              (SELECT COUNT(*)::BIGINT FROM footprints WHERE group_id = $1) AS total_count,
              COALESCE(
                  g.footprint_capacity,
                  (SELECT (config_value #>> '{}')::int FROM global_configs WHERE config_key='defaultFootprintCapacity'),
                  50
              )::INT AS capacity
           FROM association_groups g
           WHERE g.group_id = $1"#,
    )
    .bind(gid)
    .fetch_one(db)
    .await?;

    // 2026-07-08: 扩容单价从 global_configs.footprintExpandDiamondCost 读 (默认 5)
    // 前端在容量条上显示 + 点扩容按钮时也会再拉一次拿最新值
    let cost_per_slot: i32 = read_global_int(db, "footprintExpandDiamondCost", 5).await;

    // 2026-07-08: 拿组钻石余额 (扩容量扣的就是这个)
    // 之前前端 footprint 页的 loadOverview 写的是 hardcoded 0, 导致扩容按钮永远显示 0 钻石
    let diamond_balance: i64 = sqlx::query_scalar(
        "SELECT diamond::BIGINT FROM association_groups WHERE group_id = $1"
    )
    .bind(gid)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .unwrap_or(0);

    Ok(ApiResponse::success(FootprintsListResponse {
        footprints,
        next_cursor,
        has_more,
        total_count: Some(total_count),
        capacity: Some(capacity),
        cost_per_slot: Some(cost_per_slot),
        diamond_balance: Some(diamond_balance),
    }))
}

/// 删除足迹
/// DELETE /api/groups/{group_id}/footprints/{footprint_id}
/// FSD v2 10.3
#[utoipa::path(
    delete,
    path = "/api/groups/{group_id}/footprints/{footprint_id}",
    tag = "足迹",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ("footprint_id" = i64, Path, description = "足迹ID")
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "无权删除"),
        (status = 404, description = "足迹不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_footprint(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: Path<(i64, i64)>,
) -> Result<impl Responder, CustomError> {
    let (gid, footprint_id) = *path;
    let db = &state.db_pool;
    let user_id = token.user_id;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 检查是否是创建者（仅创建者可删除）
    let is_owner: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM footprints WHERE id=$1 AND user_id=$2)",
    )
    .bind(footprint_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_owner {
        return Err(CustomError::Forbidden("无权删除此足迹".into()));
    }

    // 删除足迹
    let result = sqlx::query("DELETE FROM footprints WHERE id = $1")
        .bind(footprint_id)
        .execute(db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(CustomError::NotFound("足迹不存在".into()));
    }

    #[derive(Serialize)]
    struct OkResponse {
        status: String,
    }
    Ok(ApiResponse::success(OkResponse {
        status: "ok".to_string(),
    }))
}

/// 扩容足迹容量
/// POST /api/groups/{group_id}/footprints/capacity/expand
/// FSD v2 10.4
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/footprints/capacity/expand",
    tag = "足迹",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    request_body = ExpandCapacityRequest,
    responses(
        (status = 200, description = "扩容成功", body = ExpandCapacityResponse),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员或钻石不足"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn expand_capacity(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: Path<i64>,
    body: Json<ExpandCapacityRequest>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let input = body.into_inner();
    let db = &state.db_pool;
    let user_id = token.user_id;

    // 检查用户是否是组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 获取组当前钻石和容量 (容量 fallback 同 create/list)
    let (current_diamond, current_capacity): (i32, i32) = sqlx::query_as(
        r#"SELECT diamond,
                  COALESCE(
                      footprint_capacity,
                      (SELECT (config_value #>> '{}')::int FROM global_configs WHERE config_key='defaultFootprintCapacity'),
                      50
                  ) AS capacity
           FROM association_groups WHERE group_id = $1"#
    )
    .bind(gid)
    .fetch_optional(db)
    .await?
    .unwrap_or((0, 50));

    // 钻石单价从 global_configs.footprintExpandDiamondCost 读 (默认 5)
    // 总花费 = expand_by * 单价
    let cost_per_slot: i32 = read_global_int(db, "footprintExpandDiamondCost", 5).await;
    let diamond_cost = input.expand_by * cost_per_slot;
    let new_capacity = current_capacity + input.expand_by;

    if current_diamond < diamond_cost {
        return Err(CustomError::Forbidden("组钻石不足".into()));
    }

    // 扣除钻石并更新容量
    sqlx::query(
        r#"UPDATE association_groups
           SET diamond = diamond - $1,
               footprint_capacity = $2,
               updated_at = NOW()
           WHERE group_id = $3"#,
    )
    .bind(diamond_cost)
    .bind(new_capacity)
    .bind(gid)
    .execute(db)
    .await?;

    // 写钻石流水
    // 2026-07-09: idempotency_key 改成 Optional 了, 没传时用 "ts-{ms}" 兜底
    //   避免重复点击产生相同 key 触发 UNIQUE 冲突 → 接口 500
    //   真正的"幂等防双击"是前端的活 (按钮 disabled), 后端这里只做流水 key
    let idempotency_key = format!(
        "footprint_expand_{}_{}",
        gid,
        input.idempotency_key.unwrap_or_else(|| format!("ts-{}", chrono::Utc::now().timestamp_millis()))
    );
    sqlx::query(
        r#"INSERT INTO diamond_transactions (group_id, type, amount, balance_before, balance_after, biz_type, idempotency_key, created_at)
           VALUES ($1, 'CONSUME'::diamond_tx_type_enum, $2, $3, $3 - $2, 'FOOTPRINT_CAPACITY_EXPANSION', $4, NOW())"#
    )
    .bind(gid)
    .bind(diamond_cost)
    .bind(current_diamond)
    .bind(&idempotency_key)
    .execute(db)
    .await?;

    Ok(ApiResponse::success(ExpandCapacityResponse {
        group_id: gid,
        old_capacity: current_capacity,
        new_capacity,
        diamond_cost,
        diamond_balance_after: current_diamond - diamond_cost,
    }))
}

/// 足迹分组项 (用户端可见版本)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintGroupItem {
    pub id: i64,
    pub group_name: String,
    pub group_type: i16,
    pub is_global: bool,
}

/// 列出本组可选的足迹分组
/// GET /api/groups/{group_id}/footprint-groups
///
/// 用户发布足迹时, 从这个接口拉可选分组下拉框:
/// - 公用分组 (is_global=true, group_id=NULL)
/// - 本组自建分组 (is_global=false, group_id=gid, 后续阶段启用)
/// - 仅返回 status=1 (启用中)
async fn list_available_groups(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    group_id: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let gid = *group_id;
    let user_id = token.user_id;
    let db = &state.db_pool;

    // 检查组成员
    let is_member: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2 AND member_status='ACTIVE'::group_member_status_enum)",
    )
    .bind(gid)
    .bind(user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::Forbidden("非组成员".into()));
    }

    // 公用分组 (is_global=true) + 本组自建分组 (is_global=false AND group_id=gid)
    // 按"公用在前, 自建在后"排序
    let rows = sqlx::query(
        r#"SELECT id, group_name, group_type, is_global
           FROM record_group
           WHERE status = 1
             AND (
                 is_global = TRUE
                 OR (is_global = FALSE AND group_id = $1)
             )
           ORDER BY is_global DESC, id ASC"#,
    )
    .bind(gid)
    .fetch_all(db)
    .await?;

    let items: Vec<FootprintGroupItem> = rows
        .into_iter()
        .map(|r| FootprintGroupItem {
            id: r.get("id"),
            group_name: r.get("group_name"),
            group_type: r.get("group_type"),
            is_global: r.get("is_global"),
        })
        .collect();

    Ok(ApiResponse::success(items))
}
