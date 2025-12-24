use ntex::web::{HttpRequest, HttpResponse};
use std::sync::Arc;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

// Re-export model modules for macro path resolution
use crate::{foods, game_im, models, orders, users};
// 注意：不要导入 models::wishes 为 wishes 避免遮蔽根模块 wishes

#[derive(OpenApi)]
#[openapi(
    paths(
        // 用户相关
        users::view::login,
        users::new::register,
        users::update::change_info,
        users::view::get_current_info,
        users::view::get_user_info,
        users::view::is_register,
        users::checkin::daily_checkin,
        users::invitation::get_invitation,
        users::invitation::new_invitation,
        users::invitation::confirm_invitation,
        users::invitation::cancel_invitation,
        users::invitation::unbind_request,
        users::invitation::get_group_info,
        users::role::switch_role,
        // 菜品相关
        foods::new::create_food,
        foods::update::update_food,
        foods::delete::delete_food,
        foods::new::create_tag,
        foods::delete::delete_tag,
        foods::view::get_tags,
        foods::view::get_foods,
        foods::view::get_food_detail,
        foods::update::mark_food,
        foods::update::unmark_food,
        foods::view::get_marked_foods,
        foods::view::draw_blind_box,
        // 订单相关（新结构）
        orders::new::create_order,
        orders::update::update_order_status,
        orders::view::get_orders,
        orders::view::get_order_detail,
        orders::view::get_incomplete_order,
        orders::delete::delete_order,
        orders::rating::create_order_rating,
        orders::rating::get_order_rating,
        // 心愿相关
        crate::wishes::new::create_wish,
        crate::wishes::view::get_wishes,
        crate::wishes::view::get_wish_detail,
        crate::wishes::update::disable_wish,
        crate::wishes::claim::claim_wish,
        crate::wishes::claim::get_claim,
        crate::wishes::claim::update_wish_claim,
        crate::wishes::checkin::create_wish_claim_checkin,
        crate::wishes::checkin::list_wish_claim_checkins,
        crate::dashboard::metrics::get_top_food_orders,
        crate::dashboard::metrics::get_my_today_orders,
        crate::dashboard::metrics::get_my_order_stats,
        crate::dashboard::metrics::get_points_journey,
        crate::dashboard::activities::get_group_activities,

        // IM
        game_im::sign::get_user_sig,

        // Mini-game (IM)
        game_im::rooms::list_rooms,
        game_im::rooms::create_room,
        game_im::rooms::join_room,
        game_im::rooms::dismiss_room,
        game_im::werewolf::set_ready,
        game_im::werewolf::start_game,
        game_im::werewolf::vote,
    ),
    components(
        // 用户
        schemas(
            models::users::LoginInput,
            models::users::LoginResponse,
            models::users::UserPublic,
            models::users::IsRegisterResponse,
            models::users::DailyCheckinOut,
            users::role::RoleSwitchResult,
            users::role::RoleSwitchInput,
        ),
        // 邀请
        schemas(
            models::invitation::NewInvitationInput,
            models::invitation::ConfirmInvitationInput,
            models::invitation::InvitationRequestOut,
            models::invitation::InvitationListOut,
            models::invitation::GroupMemberOut,
            models::invitation::GroupInfoOut,
            models::invitation::UnbindRequestInput,
        ),
        // 菜品
        schemas(
            models::foods::FoodCreateInput,
            models::foods::FoodUpdateInput,
            models::foods::FoodOut,
            models::foods::FoodTagOut,
            models::foods::TagCreateInput,
            models::foods::FoodFilterQuery,
            models::foods::FoodMarkActionInput,
            models::foods::BlindBoxDrawInput,
            models::foods::BlindBoxDrawResultOut,
        ),
        // 订单新模型
        schemas(
            models::orders::OrderCreateInput,
            models::orders::OrderStatusUpdateInput,
            models::orders::OrderItemOut,
            models::orders::OrderStatusHistoryOut,
            models::orders::OrderOutNew,
            models::orders::OrderStatusUpdateInput,
            models::orders::OrderQuery,
            models::orders::OrderRatingCreateInput,
            models::orders::OrderRatingOut,
        ),
        // 心愿模型
        schemas(
            models::wishes::WishCreateInput,
            models::wishes::WishUpdateInput,
            models::wishes::WishOut,
            models::wishes::WishQuery,
            models::wishes::WishClaimCreateInput,
            models::wishes::WishClaimUpdateInput,
            models::wishes::WishClaimOut,
            models::wishes::WishClaimCheckinCreateInput,
            models::wishes::WishClaimCheckinOut,
            crate::dashboard::activities::GroupActivityEventOut,
            crate::dashboard::metrics::TopFoodOrderOut,
            crate::dashboard::metrics::TopFoodRankingResponse,
            crate::dashboard::metrics::TodayOrderEntryOut,
            crate::dashboard::metrics::TodayOrdersResponse,
            crate::dashboard::metrics::OrderStatsOut,
            crate::dashboard::metrics::JourneyOrderOut,
            crate::dashboard::metrics::PointsJourneyOut,

            // IM
            models::game_im::ImUserSigOut,
            models::game_im::ImRoomOut,
            models::game_im::ImRoomListOut,
            models::game_im::ImCreateRoomIn,
            models::game_im::ImJoinRoomOut,
            models::game_im::ImDismissRoomIn,
            models::game_im::ImDismissRoomOut,
            models::game_im::ImReadyIn,
            models::game_im::ImStartGameOut,
            models::game_im::ImVoteIn,
            models::game_im::ImVoteOut,
        ),
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "用户", description = "用户相关接口"),
        (name = "菜品", description = "菜品相关接口"),
        (name = "订单", description = "订单相关接口"),
        (name = "心愿", description = "心愿与兑换相关接口"),
        (name = "看板", description = "组活动与概览接口"),
        (name = "IM", description = "腾讯云 IM（UserSig / 后台联调）"),
        (name = "小游戏", description = "基于腾讯云 IM 的实时多人小游戏"),
    ),
    servers((url = "http://localhost:9831", description = "本地服务器"))
)]
pub struct ApiDoc;

pub async fn openapi_json() -> HttpResponse {
    let doc = ApiDoc::openapi();
    HttpResponse::Ok().json(&doc)
}

pub async fn serve_swagger(req: HttpRequest) -> HttpResponse {
    let config =
        utoipa_swagger_ui::Config::new(["/api-doc/openapi.json"]).persist_authorization(true);
    let config = Arc::new(config);
    let path = req.uri().path();
    let tail = path.strip_prefix("/swagger-ui/").unwrap_or("");
    match utoipa_swagger_ui::serve(tail, config) {
        Ok(swagger_file) => {
            if let Some(file) = swagger_file {
                HttpResponse::Ok()
                    .content_type(&file.content_type)
                    .body(file.bytes.to_vec())
            } else {
                HttpResponse::NotFound().finish()
            }
        }
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "cookie_auth",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("Authorization"))),
            );
        }
    }
}
