// API - 主人家厨房路由
// FSD.latest.md compliant - 做客系统

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
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

/// 配置做客厨房路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/kitchens/invitations")
            .route("/{invite_code}", web::get().to(access_kitchen))
            .route("/{invite_code}/foods", web::get().to(get_kitchen_foods))
            .route("/{invite_code}/orders", web::post().to(create_guest_order)),
    );
}

/// 访问主人家厨房响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccessKitchenResponse {
    pub group_id: i64,
    pub group_name: String,
    pub buyer_nick_name: Option<String>,
    pub seller_nick_name: Option<String>,
    pub buyer_avatar: Option<String>,
    pub seller_avatar: Option<String>,
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

    // 查找邀请
    let invite = sqlx::query_as::<_, (i64, chrono::DateTime<chrono::Utc>, i64, i64)>(
        r#"SELECT gi.group_id, gi.expires_at, gi.max_uses, gi.used_count
           FROM guest_invitations gi
           WHERE gi.invite_code = $1 AND gi.status = 'ACTIVE'::user_status_enum"#,
    )
    .bind(&code)
    .fetch_optional(db)
    .await?;

    let (group_id, expires_at, max_uses, used_count) = match invite {
        Some(inv) => inv,
        None => return Err(CustomError::NotFound("邀请码不存在或已失效".into())),
    };

    // 检查是否过期
    if Utc::now() > expires_at {
        return Err(CustomError::BadRequest("邀请码已过期".into()));
    }

    // 检查是否已用完
    if used_count >= max_uses {
        return Err(CustomError::BadRequest("邀请码已用完".into()));
    }

    // 获取主人家组信息
    let group_row = sqlx::query(
        r#"SELECT g.group_id, g.group_name, g.buyer_user_id, g.seller_user_id,
                  buyer.nick_name as buyer_nick, buyer.avatar as buyer_avatar,
                  seller.nick_name as seller_nick, seller.avatar as seller_avatar
           FROM association_groups g
           LEFT JOIN users buyer ON buyer.user_id = g.buyer_user_id
           LEFT JOIN users seller ON seller.user_id = g.seller_user_id
           WHERE g.group_id = $1"#,
    )
    .bind(group_id)
    .fetch_optional(db)
    .await?;

    let row = match group_row {
        Some(r) => r,
        None => return Err(CustomError::NotFound("组不存在".into())),
    };

    Ok(ApiResponse::success(serde_json::json!({
        "groupId": row.get::<i64, _>("group_id"),
        "groupName": row.get::<String, _>("group_name"),
        "buyerNickName": row.get::<Option<String>, _>("buyer_nick"),
        "sellerNickName": row.get::<Option<String>, _>("seller_nick"),
        "buyerAvatar": row.get::<Option<String>, _>("buyer_avatar"),
        "sellerAvatar": row.get::<Option<String>, _>("seller_avatar"),
        "inviteCode": code,
        "expiresAt": expires_at,
        "status": "ok"
    })))
}

/// 厨房菜品项
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KitchenFoodItem {
    pub food_id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub images: Option<serde_json::Value>,
    pub tags: Option<serde_json::Value>,
    pub ingredients: Option<serde_json::Value>,
    pub steps: Option<serde_json::Value>,
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

    // 验证邀请码
    let group_id: i64 = sqlx::query_scalar(
        r#"SELECT gi.group_id FROM guest_invitations gi
           WHERE gi.invite_code = $1 AND gi.status = 'ACTIVE'::user_status_enum
           AND gi.expires_at > NOW() AND gi.used_count < gi.max_uses"#,
    )
    .bind(&code)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::NotFound("邀请码无效或已过期".into()))?;

    // 获取主人家菜品
    let foods = sqlx::query(
        r#"SELECT food_id, group_id, name, description, images, tags, ingredients, steps, status
           FROM foods WHERE group_id=$1 AND food_status='NORMAL'::food_status_enum
           ORDER BY created_at DESC"#,
    )
    .bind(group_id)
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
                "status": r.get::<String, _>("status")
            })
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
        (status = 201, description = "创建成功", body = CreateGuestOrderResponse),
        (status = 401, description = "未登录"),
        (status = 404, description = "邀请码无效或已过期")
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

    // 验证邀请码并获取主人家组
    let invite_row = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT gi.group_id, gi.id
           FROM guest_invitations gi
           WHERE gi.invite_code = $1 AND gi.status = 'ACTIVE'::user_status_enum
           AND gi.expires_at > NOW() AND gi.used_count < gi.max_uses"#,
    )
    .bind(&code)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::NotFound("邀请码无效或已过期".into()))?;

    let (group_id, invite_id) = invite_row;

    // 获取主人家Seller
    let seller_id: Option<i64> =
        sqlx::query_scalar("SELECT seller_user_id FROM association_groups WHERE group_id=$1")
            .bind(group_id)
            .fetch_one(db)
            .await?;

    // 创建做客订单
    let order_id = idgenerator::IdInstance::next_id();

    sqlx::query(
        r#"INSERT INTO orders (order_id, user_id, group_id, type, status, creator_role_snapshot, assignee_id, assignee_role_snapshot, title, content, guest_user_id, guest_invite_id, guest_remark, is_guest, created_at)
           VALUES ($1, $2, $3, 'GUEST'::order_type_enum, 'CREATED'::order_status_enum, 'ORDERING', $4, 'RECEIVING', $5, $6, $7, $8, $9, true, NOW())"#
    )
    .bind(order_id)
    .bind(token.user_id)
    .bind(group_id)
    .bind(seller_id)
    .bind(&input.title)
    .bind(&input.content)
    .bind(token.user_id)
    .bind(invite_id)
    .bind(&input.guest_remark)
    .execute(db)
    .await?;

    // 增加邀请已使用次数
    sqlx::query("UPDATE guest_invitations SET used_count = used_count + 1 WHERE id = $1")
        .bind(invite_id)
        .execute(db)
        .await?;

    Ok(ApiResponse::success(serde_json::json!({
        "orderId": order_id,
        "status": "ok"
    })))
}

/// 做客订单输入
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuestOrderInput {
    pub title: String,
    pub content: String,
    pub guest_remark: Option<String>,
}
