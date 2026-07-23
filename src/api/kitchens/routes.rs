// API - 主人家厨房路由
// FSD.latest.md compliant - 做客系统

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, ServiceConfig,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::{collections::HashSet, sync::Arc};
use utoipa::ToSchema;

use crate::api::foods::routes::{FoodImage, FoodIngredient, FoodStep, TagRef};
use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

const GUEST_INVITATION_TTL_HOURS: i64 = 24;
const MAX_GUEST_INVITATION_USES: i32 = 20;
const MAX_GUEST_ORDER_ITEMS: usize = 50;
const MAX_GUEST_ITEM_QUANTITY: i32 = 99;

/// 配置做客厨房路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/{group_id}/guest-invitations")
            .route(web::post().to(create_guest_invitation)),
    );
    cfg.service(
        web::scope("/api/kitchens/invitations")
            .route("/{invite_code}", web::get().to(access_kitchen))
            .route("/{invite_code}/foods", web::get().to(get_kitchen_foods))
            .route("/{invite_code}/orders", web::post().to(create_guest_order)),
    );
}

/// 创建做客邀请响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuestInvitationCreated {
    pub invite_code: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InvitationValidityError {
    Inactive,
    MissingExpiry,
    Expired,
    Exhausted,
}

fn generate_guest_invite_code() -> String {
    // 24 bytes = 192 bits；base64url 无 padding 后刚好 32 字符，适配 v3.sql VARCHAR(32)。
    let mut entropy = [0_u8; 24];
    let mut rng = OsRng;
    rng.fill_bytes(&mut entropy);
    URL_SAFE_NO_PAD.encode(entropy)
}

fn validate_invitation_snapshot(
    status: &str,
    expires_at: Option<DateTime<Utc>>,
    used_count: i32,
    max_uses: i32,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, InvitationValidityError> {
    if status != "ACTIVE" {
        return Err(InvitationValidityError::Inactive);
    }
    let expires_at = expires_at.ok_or(InvitationValidityError::MissingExpiry)?;
    if expires_at <= now {
        return Err(InvitationValidityError::Expired);
    }
    if max_uses <= 0 || used_count < 0 || used_count >= max_uses {
        return Err(InvitationValidityError::Exhausted);
    }
    Ok(expires_at)
}

fn validate_guest_order_input(input: &GuestOrderInput) -> Result<(), CustomError> {
    let title_len = input.title.trim().chars().count();
    if title_len == 0 || title_len > 128 {
        return Err(CustomError::BadRequest("title 必须为 1-128 个字符".into()));
    }
    if input.content.trim().is_empty() || input.content.chars().count() > 5000 {
        return Err(CustomError::BadRequest(
            "content 必须为 1-5000 个字符".into(),
        ));
    }
    if input
        .guest_remark
        .as_deref()
        .is_some_and(|remark| remark.chars().count() > 2000)
    {
        return Err(CustomError::BadRequest(
            "guestRemark 最多 2000 个字符".into(),
        ));
    }
    if input.items.is_empty() || input.items.len() > MAX_GUEST_ORDER_ITEMS {
        return Err(CustomError::BadRequest(format!(
            "items 必须包含 1-{} 个菜品",
            MAX_GUEST_ORDER_ITEMS
        )));
    }

    let mut food_ids = HashSet::with_capacity(input.items.len());
    for item in &input.items {
        let quantity = item.quantity.unwrap_or(1);
        if item.food_id <= 0 || !(1..=MAX_GUEST_ITEM_QUANTITY).contains(&quantity) {
            return Err(CustomError::BadRequest("菜品或数量不合法".into()));
        }
        if !food_ids.insert(item.food_id) {
            return Err(CustomError::BadRequest("items 不能包含重复菜品".into()));
        }
    }
    Ok(())
}

fn validate_guest_order_actor(is_target_group_member: bool) -> Result<(), CustomError> {
    if is_target_group_member {
        return Err(CustomError::user_already_in_group(
            "你已是该厨房成员，请使用普通清单",
        ));
    }
    Ok(())
}

/// 创建做客邀请
/// POST /api/groups/{group_id}/guest-invitations
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/guest-invitations",
    tag = "做客厨房",
    params(("group_id" = i64, Path, description = "目标组 ID")),
    responses(
        (status = 200, description = "邀请创建成功", body = GuestInvitationCreated),
        (status = 401, description = "未登录"),
        (status = 403, description = "不是目标组 ACTIVE 成员")
    ),
    security(("bearer_auth" = []))
)]
async fn create_guest_invitation(
    token: UserToken,
    state: State<Arc<AppState>>,
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let group_id = group_id.into_inner();
    let mut tx = state.db_pool.begin().await?;

    // 以 URL 中的目标组为准，在数据库内校验并锁住成员关系；不信任 RequireGroup。
    let membership_id: Option<i64> = sqlx::query_scalar(
        r#"SELECT m.id
           FROM association_group_members m
           JOIN association_groups g ON g.group_id = m.group_id
           WHERE m.user_id = $1
             AND m.group_id = $2
             AND m.member_status = 'ACTIVE'::group_member_status_enum
             AND g.status = 'ACTIVE'::user_status_enum
           FOR UPDATE OF m, g"#,
    )
    .bind(token.user_id)
    .bind(group_id)
    .fetch_optional(&mut *tx)
    .await?;
    if membership_id.is_none() {
        return Err(CustomError::Forbidden("无权访问该组".into()));
    }

    let now = Utc::now();
    let active_invitation: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT invite_code, expires_at
           FROM guest_invitations
           WHERE group_id = $1
             AND created_by = $2
             AND status = 'ACTIVE'::guest_invite_status_enum
             AND expires_at IS NOT NULL
             AND expires_at > $3
             AND used_count < max_uses
           ORDER BY created_at DESC, id DESC
           LIMIT 1"#,
    )
    .bind(group_id)
    .bind(token.user_id)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;

    // 获取邀请是服务端幂等操作：已有可用码时直接复用，不撤销也不延长有效期。
    if let Some((invite_code, expires_at)) = active_invitation {
        tx.commit().await?;
        return Ok(ApiResponse::success(GuestInvitationCreated {
            invite_code,
            expires_at,
        }));
    }

    // 没有可用码时才撤销同一创建者的失效旧码，保留历史记录供审计。
    sqlx::query(
        r#"UPDATE guest_invitations
           SET status = 'REVOKED'::guest_invite_status_enum
           WHERE group_id = $1
             AND created_by = $2
             AND status = 'ACTIVE'::guest_invite_status_enum
             AND (
                 expires_at IS NULL
                 OR expires_at <= $3
                 OR max_uses <= 0
                 OR used_count < 0
                 OR used_count >= max_uses
             )"#,
    )
    .bind(group_id)
    .bind(token.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let invite_code = generate_guest_invite_code();
    let expires_at = now + Duration::hours(GUEST_INVITATION_TTL_HOURS);
    sqlx::query(
        r#"INSERT INTO guest_invitations
               (group_id, invite_code, created_by, max_uses, used_count, expires_at, status)
           VALUES ($1, $2, $3, $4, 0, $5, 'ACTIVE'::guest_invite_status_enum)"#,
    )
    .bind(group_id)
    .bind(&invite_code)
    .bind(token.user_id)
    .bind(MAX_GUEST_INVITATION_USES)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(ApiResponse::success(GuestInvitationCreated {
        invite_code,
        expires_at,
    }))
}

/// 访问主人家厨房响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccessKitchenResponse {
    pub group_id: i64,
    pub group_name: String,
    pub group_avatar: Option<String>,
    pub buyer_nick_name: Option<String>,
    pub seller_nick_name: Option<String>,
    pub buyer_avatar: Option<String>,
    pub seller_avatar: Option<String>,
    pub tags: Vec<TagRef>,
    pub invite_code: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
}

/// 访问主人家厨房
/// GET /api/kitchens/invitations/{invite_code}
///
/// 做客用户必须使用长期账号，通过邀请链接访问主人家厨房
/// 返回厨房信息和邀请有效期
#[utoipa::path(
    get,
    path = "/api/kitchens/invitations/{invite_code}",
    tag = "做客厨房",
    params(
        ("invite_code" = String, Path, description = "邀请码")
    ),
    responses(
        (status = 200, description = "获取成功", body = AccessKitchenResponse),
        (status = 400, description = "邀请码已过期或已用完"),
        (status = 404, description = "邀请码不存在或组不存在")
    )
)]
async fn access_kitchen(
    state: State<Arc<AppState>>,
    invite_code: Path<String>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let code = invite_code.into_inner();

    let row = sqlx::query(
        r#"SELECT gi.group_id, gi.expires_at, gi.max_uses, gi.used_count,
                  gi.status::text AS invite_status,
                  COALESCE(g.group_name, '主人家厨房') AS group_name, g.group_avatar,
                  buyer.nick_name AS buyer_nick, buyer.avatar AS buyer_avatar,
                  seller.nick_name AS seller_nick, seller.avatar AS seller_avatar
           FROM guest_invitations gi
           JOIN association_groups g ON g.group_id = gi.group_id
           LEFT JOIN users buyer ON buyer.user_id = g.buyer_user_id
           LEFT JOIN users seller ON seller.user_id = g.seller_user_id
           WHERE gi.invite_code = $1
             AND g.status = 'ACTIVE'::user_status_enum"#,
    )
    .bind(&code)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::NotFound("邀请码不存在或已失效".into()))?;

    let expires_at = validate_invitation_snapshot(
        row.get::<String, _>("invite_status").as_str(),
        row.try_get::<Option<DateTime<Utc>>, _>("expires_at")?,
        row.get::<i32, _>("used_count"),
        row.get::<i32, _>("max_uses"),
        Utc::now(),
    )
    .map_err(|reason| match reason {
        InvitationValidityError::Inactive => CustomError::NotFound("邀请码不存在或已失效".into()),
        InvitationValidityError::MissingExpiry | InvitationValidityError::Expired => {
            CustomError::BadRequest("邀请码已过期".into())
        }
        InvitationValidityError::Exhausted => CustomError::BadRequest("邀请码已用完".into()),
    })?;

    let group_id = row.get::<i64, _>("group_id");
    let tag_rows = sqlx::query(
        r#"SELECT tag_id, tag_name, icon
           FROM tags
           WHERE group_id = $1 OR group_id IS NULL
           ORDER BY sort ASC, tag_id ASC"#,
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;
    let tags = tag_rows
        .into_iter()
        .map(|tag| TagRef {
            tag_id: tag.get("tag_id"),
            name: tag.get("tag_name"),
            icon: tag.try_get("icon").ok().flatten(),
        })
        .collect();

    Ok(ApiResponse::success(AccessKitchenResponse {
        group_id,
        group_name: row.get("group_name"),
        group_avatar: row.try_get("group_avatar").ok().flatten(),
        buyer_nick_name: row.try_get("buyer_nick").ok().flatten(),
        seller_nick_name: row.try_get("seller_nick").ok().flatten(),
        buyer_avatar: row.try_get("buyer_avatar").ok().flatten(),
        seller_avatar: row.try_get("seller_avatar").ok().flatten(),
        tags,
        invite_code: code,
        expires_at,
        status: "ok".to_string(),
    }))
}

/// 厨房菜品项
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KitchenFoodItem {
    pub food_id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Vec<FoodImage>,
    pub tag: TagRef,
    pub ingredients: Vec<FoodIngredient>,
    pub steps: Vec<FoodStep>,
    pub status: String,
}

/// 查看主人家厨房菜单
/// GET /api/kitchens/invitations/{invite_code}/foods
///
/// 做客用户可查看主人家厨房菜单
/// 仅返回授权范围内的菜单
#[utoipa::path(
    get,
    path = "/api/kitchens/invitations/{invite_code}/foods",
    tag = "做客厨房",
    params(
        ("invite_code" = String, Path, description = "邀请码")
    ),
    responses(
        (status = 200, description = "获取成功", body = Vec<KitchenFoodItem>),
        (status = 404, description = "邀请码无效或已过期")
    )
)]
async fn get_kitchen_foods(
    state: State<Arc<AppState>>,
    invite_code: Path<String>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let code = invite_code.into_inner();

    // 仅邀请码决定目标组；groupId 等客户端参数不参与授权。
    let group_id: i64 = sqlx::query_scalar(
        r#"SELECT gi.group_id
           FROM guest_invitations gi
           JOIN association_groups g ON g.group_id = gi.group_id
           WHERE gi.invite_code = $1
             AND gi.status = 'ACTIVE'::guest_invite_status_enum
             AND gi.expires_at IS NOT NULL
             AND gi.expires_at > NOW()
             AND gi.used_count < gi.max_uses
             AND g.status = 'ACTIVE'::user_status_enum"#,
    )
    .bind(&code)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::NotFound("邀请码无效或已过期".into()))?;

    // v3.sql 中真实列为 food_name / food_status / tag_id；标签来自 tags 表。
    let foods = sqlx::query(
        r#"SELECT f.food_id, f.group_id, f.food_name, f.description, f.images,
                  f.tag_id, t.tag_name, t.icon AS tag_icon,
                  f.ingredients, f.steps, f.food_status::text AS food_status
           FROM foods f
           JOIN tags t ON t.tag_id = f.tag_id
           WHERE f.group_id = $1
             AND f.food_status = 'NORMAL'::food_status_enum
             AND f.is_del = 0
           ORDER BY f.created_at DESC"#,
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;

    let result: Vec<KitchenFoodItem> = foods
        .into_iter()
        .map(|r| {
            let images_json: serde_json::Value = r
                .try_get("images")
                .unwrap_or_else(|_| serde_json::json!([]));
            let ingredients_text: Option<String> = r.try_get("ingredients").ok().flatten();
            let steps_text: Option<String> = r.try_get("steps").ok().flatten();

            KitchenFoodItem {
                food_id: r.get("food_id"),
                group_id: r.get("group_id"),
                name: r.get("food_name"),
                description: r.try_get("description").ok().flatten(),
                images: serde_json::from_value(images_json).unwrap_or_default(),
                tag: TagRef {
                    tag_id: r.get("tag_id"),
                    name: r.get("tag_name"),
                    icon: r.try_get("tag_icon").ok().flatten(),
                },
                ingredients: ingredients_text
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or_default(),
                steps: steps_text
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or_default(),
                status: match r.get::<String, _>("food_status").as_str() {
                    "NORMAL" => "ACTIVE".to_string(),
                    other => other.to_string(),
                },
            }
        })
        .collect();

    Ok(ApiResponse::success(result))
}

/// 创建做客订单响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateGuestOrderResponse {
    pub order_id: i64,
    pub status: String,
}

/// 创建做客订单
/// POST /api/kitchens/invitations/{invite_code}/orders
///
/// 做客用户下单，订单归属主人家小组
/// 由主人家Seller完成
/// 做客用户不获得主人组爱心积分
#[utoipa::path(
    post,
    path = "/api/kitchens/invitations/{invite_code}/orders",
    tag = "做客厨房",
    params(
        ("invite_code" = String, Path, description = "邀请码")
    ),
    request_body = GuestOrderInput,
    responses(
        (status = 200, description = "创建成功", body = CreateGuestOrderResponse),
        (status = 400, description = "订单内容或菜品无效"),
        (status = 401, description = "未登录"),
        (status = 404, description = "邀请码无效或已过期"),
        (status = 409, description = "当前用户已是目标厨房成员")
    ),
    security(("bearer_auth" = []))
)]
async fn create_guest_order(
    token: UserToken,
    state: State<Arc<AppState>>,
    invite_code: Path<String>,
    body: Json<GuestOrderInput>,
) -> Result<HttpResponse, CustomError> {
    let db = &state.db_pool;
    let code = invite_code.into_inner();
    let input = body.into_inner();
    validate_guest_order_input(&input)?;

    let mut tx = db.begin().await?;

    // 锁住邀请行，确保并发请求不能重复消费最后一次使用额度。
    let invite_row = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT gi.group_id, gi.id
           FROM guest_invitations gi
           JOIN association_groups g ON g.group_id = gi.group_id
           WHERE gi.invite_code = $1
             AND gi.status = 'ACTIVE'::guest_invite_status_enum
             AND gi.expires_at IS NOT NULL
             AND gi.expires_at > NOW()
             AND gi.used_count < gi.max_uses
             AND g.status = 'ACTIVE'::user_status_enum
           FOR UPDATE OF gi, g"#,
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::NotFound("邀请码无效或已过期".into()))?;

    let (group_id, invite_id) = invite_row;

    // 目标厨房自己的成员不能通过做客链路创建 GUEST 清单；
    // 用户属于其他厨房不受影响，仍可作为外部访客做客。
    let is_target_group_member: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1
               FROM association_group_members
               WHERE group_id = $1
                 AND user_id = $2
                 AND member_status = 'ACTIVE'::group_member_status_enum
           )"#,
    )
    .bind(group_id)
    .bind(token.user_id)
    .fetch_one(&mut *tx)
    .await?;
    validate_guest_order_actor(is_target_group_member)?;

    // assignee 必须是目标组当前 ACTIVE 的 RECEIVING 成员，并与 group.seller_user_id 一致。
    let assignee_id: i64 = sqlx::query_scalar(
        r#"SELECT m.user_id
           FROM association_groups g
           JOIN association_group_members m
             ON m.group_id = g.group_id
            AND m.user_id = g.seller_user_id
           WHERE g.group_id = $1
             AND g.status = 'ACTIVE'::user_status_enum
             AND m.member_status = 'ACTIVE'::group_member_status_enum
             AND m.role_in_group = 'RECEIVING'::group_member_role_enum"#,
    )
    .bind(group_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::BadRequest("主人家暂无可接单成员".into()))?;

    // 一次查询验证全部 food_id：必须属于邀请目标组，且是未删除的 NORMAL 菜品。
    let food_ids: Vec<i64> = input.items.iter().map(|item| item.food_id).collect();
    let food_rows = sqlx::query(
        r#"SELECT f.food_id, f.food_name, f.images
           FROM foods f
           WHERE f.food_id = ANY($1::bigint[])
             AND f.group_id = $2
             AND f.food_status = 'NORMAL'::food_status_enum
             AND f.is_del = 0
           FOR SHARE OF f"#,
    )
    .bind(&food_ids)
    .bind(group_id)
    .fetch_all(&mut *tx)
    .await?;
    let active_food_ids: HashSet<i64> = food_rows
        .iter()
        .map(|row| row.get::<i64, _>("food_id"))
        .collect();
    if active_food_ids.len() != input.items.len() {
        return Err(CustomError::BadRequest(
            "所选菜品不存在、已下架或不属于该厨房".into(),
        ));
    }

    // 在同一事务内做带有效性条件的原子消费；后续任一步失败都会整体回滚。
    let consumed = sqlx::query(
        r#"UPDATE guest_invitations
           SET used_count = used_count + 1
           WHERE id = $1
             AND status = 'ACTIVE'::guest_invite_status_enum
             AND expires_at IS NOT NULL
             AND expires_at > NOW()
             AND used_count < max_uses"#,
    )
    .bind(invite_id)
    .execute(&mut *tx)
    .await?;
    if consumed.rows_affected() != 1 {
        return Err(CustomError::NotFound("邀请码无效或已过期".into()));
    }

    let order_id = idgenerator::IdInstance::next_id();
    let guest_remark = input
        .guest_remark
        .as_deref()
        .map(str::trim)
        .filter(|remark| !remark.is_empty())
        .map(str::to_string);

    sqlx::query(
        r#"INSERT INTO orders
               (order_id, user_id, group_id, type, status, goal_time, remark,
                creator_role_snapshot, assignee_id, assignee_role_snapshot,
                title, content, guest_user_id, guest_invite_id, guest_remark,
                is_guest, last_status_change_at, created_at, updated_at)
           VALUES
               ($1, $2, $3, 'GUEST'::order_type_enum, 'CREATED'::order_status_enum,
                $4, $5, 'ORDERING', $6, 'RECEIVING', $7, $8, $9, $10, $11,
                true, NOW(), NOW(), NOW())"#,
    )
    .bind(order_id)
    .bind(token.user_id)
    .bind(group_id)
    .bind(input.goal_time)
    .bind(&guest_remark)
    .bind(assignee_id)
    .bind(input.title.trim())
    .bind(input.content.trim())
    .bind(token.user_id)
    .bind(invite_id)
    .bind(&guest_remark)
    .execute(&mut *tx)
    .await?;

    for item in &input.items {
        let food_row = food_rows
            .iter()
            .find(|row| row.get::<i64, _>("food_id") == item.food_id)
            .expect("active_food_ids 已完整校验");
        let snapshot = serde_json::json!({
            "foodId": item.food_id,
            "name": food_row.get::<String, _>("food_name"),
            "images": food_row.get::<serde_json::Value, _>("images")
        });
        let quantity = item.quantity.unwrap_or(1);

        sqlx::query(
            r#"INSERT INTO order_items (order_id, food_id, quantity, snapshot_json)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(order_id)
        .bind(item.food_id)
        .bind(quantity)
        .bind(snapshot)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO food_stats
                   (food_id, total_order_count, last_order_time, updated_at)
               VALUES ($1, $2, NOW(), NOW())
               ON CONFLICT (food_id) DO UPDATE
               SET total_order_count = food_stats.total_order_count + EXCLUDED.total_order_count,
                   last_order_time = NOW(),
                   updated_at = NOW()"#,
        )
        .bind(item.food_id)
        .bind(quantity)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"INSERT INTO order_status_history
               (order_id, from_status, to_status, changed_by)
           VALUES ($1, NULL, 'CREATED'::order_status_enum, $2)"#,
    )
    .bind(order_id)
    .bind(token.user_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(ApiResponse::success(CreateGuestOrderResponse {
        order_id,
        status: "ok".to_string(),
    }))
}

/// 做客订单菜品
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuestOrderItemInput {
    pub food_id: i64,
    pub quantity: Option<i32>,
}

/// 做客订单输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuestOrderInput {
    pub title: String,
    pub content: String,
    pub guest_remark: Option<String>,
    pub goal_time: Option<DateTime<Utc>>,
    pub items: Vec<GuestOrderItemInput>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_order_input() -> GuestOrderInput {
        GuestOrderInput {
            title: "好友做客：红烧肉".to_string(),
            content: "红烧肉".to_string(),
            guest_remark: Some("少盐".to_string()),
            goal_time: None,
            items: vec![GuestOrderItemInput {
                food_id: 100,
                quantity: Some(1),
            }],
        }
    }

    #[test]
    fn guest_invite_code_contains_at_least_128_bits_of_entropy() {
        let first = generate_guest_invite_code();
        let second = generate_guest_invite_code();
        assert_eq!(first.len(), 32);
        assert_ne!(first, second);
        assert_eq!(URL_SAFE_NO_PAD.decode(first).unwrap().len(), 24);
    }

    #[test]
    fn invitation_snapshot_requires_active_unexpired_capacity() {
        let now = Utc::now();
        let future = now + Duration::hours(1);
        assert!(validate_invitation_snapshot(
            "ACTIVE",
            Some(future),
            MAX_GUEST_INVITATION_USES - 1,
            MAX_GUEST_INVITATION_USES,
            now
        )
        .is_ok());
        assert_eq!(
            validate_invitation_snapshot("REVOKED", Some(future), 0, 1, now),
            Err(InvitationValidityError::Inactive)
        );
        assert_eq!(
            validate_invitation_snapshot("ACTIVE", None, 0, 1, now),
            Err(InvitationValidityError::MissingExpiry)
        );
        assert_eq!(
            validate_invitation_snapshot("ACTIVE", Some(now), 0, 1, now),
            Err(InvitationValidityError::Expired)
        );
        assert_eq!(
            validate_invitation_snapshot(
                "ACTIVE",
                Some(future),
                MAX_GUEST_INVITATION_USES,
                MAX_GUEST_INVITATION_USES,
                now
            ),
            Err(InvitationValidityError::Exhausted)
        );
    }

    #[test]
    fn guest_order_validation_rejects_duplicate_or_invalid_items() {
        let mut duplicate = valid_order_input();
        duplicate.items.push(GuestOrderItemInput {
            food_id: 100,
            quantity: Some(1),
        });
        assert!(validate_guest_order_input(&duplicate).is_err());

        let mut invalid_quantity = valid_order_input();
        invalid_quantity.items[0].quantity = Some(0);
        assert!(validate_guest_order_input(&invalid_quantity).is_err());

        let mut empty = valid_order_input();
        empty.items.clear();
        assert!(validate_guest_order_input(&empty).is_err());
    }

    #[test]
    fn target_kitchen_members_cannot_create_guest_orders() {
        assert!(validate_guest_order_actor(false).is_ok());
        let error = validate_guest_order_actor(true).unwrap_err();
        assert_eq!(error.fsd_code().as_str(), "USER_ALREADY_IN_GROUP");
    }
}
