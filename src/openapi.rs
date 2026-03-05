use ntex::web::{HttpRequest, HttpResponse};
use std::sync::Arc;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

// Re-export model modules for macro path resolution
use crate::{dashboard, foods, game_im, models, orders, upload, users};
// 注意：不要导入 crate::wishes::models 为 wishes 避免遮蔽根模块 wishes

#[derive(OpenApi)]
#[openapi(
    paths(
        // 用户 (User)
        users::new::register,
        users::view::login,
        users::view::get_current_info,
        users::update::change_info,
        users::view::is_register,
        users::role::switch_role,
        users::view::get_user_info,
        users::sweet_talk::add_sweet_talk,
        users::sweet_talk::update_sweet_talk,
        users::sweet_talk::get_sweet_talks,

        // 团队 (Team)
        users::invitation::get_invitation,
        users::invitation::new_invitation,
        users::invitation::confirm_invitation,
        users::invitation::cancel_invitation,
        users::invitation::unbind_request,
        users::invitation::bind_user_directly,
        users::invitation::get_group_info,
        users::group_update::update_group,

        // 游戏 (Game)
        game_im::routes::get_user_sig,
        game_im::routes::list_rooms,
        game_im::routes::start_game,
        game_im::routes::vote,

        // 菜品 (Dish)
        foods::routes::food::create_food,
        foods::routes::food::get_foods,
        foods::routes::food::get_marked_foods,
        foods::routes::food::draw_blind_box,
        foods::routes::food::mark_food,
        foods::routes::food::unmark_food,
        foods::routes::food::get_food_detail,
        foods::routes::food::update_food,
        foods::routes::food::delete_food,

        // 标签 (Tag)
        foods::routes::tag::create_tag,
        foods::routes::tag::get_tags,
        foods::routes::tag::update_tags_sort,
        foods::routes::tag::update_tag,
        foods::routes::tag::delete_tag,

        // 食材 (Ingredient)
        foods::routes::ingredient::list_ingredients,
        foods::routes::ingredient::create_ingredient,
        foods::routes::ingredient::update_ingredients_sort,
        foods::routes::ingredient::get_ingredient,
        foods::routes::ingredient::update_ingredient,
        foods::routes::ingredient::delete_ingredient,

        // 订单 (Order)
        orders::new::create_order,
        orders::view::get_orders,
        orders::update::update_order_status,
        orders::view::get_order_detail,
        orders::delete::delete_order,
        orders::view::get_incomplete_order,

        // 评分 (Rating)
        orders::rating::create_order_rating,
        orders::rating::get_order_rating,

        // 心愿 (Wish)
        crate::wishes::routes::create_wish,
        crate::wishes::routes::list_wishes,
        crate::wishes::routes::get_wish,
        crate::wishes::routes::update_wish,
        crate::wishes::routes::delete_wish,
        crate::wishes::routes::redeem_wish,
        crate::wishes::routes::submit_feedback,

        // 微信 (WeChat)
        crate::wx::routes::wx_sign_verify,
        crate::wx::routes::get_templates,

        // 看板 (Dashboard)
        crate::dashboard::routes::get_group_activities,
        crate::dashboard::routes::get_top_food_orders,
        crate::dashboard::routes::get_my_today_orders,
        crate::dashboard::routes::get_my_order_stats,
        crate::dashboard::routes::get_points_journey,
        crate::dashboard::routes::get_week_order_dates,
        crate::dashboard::routes::get_date_foods,

        // 签到 (Check-in)
        users::checkin::daily_checkin,
        users::sign::sign_in,
        users::sign::get_sign_info,

        // 上传
        upload::routes::get_qiniu_token,
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
            // 签到
            models::sign::SignInResponse,
            models::sign::SignRecordOut,
            models::sign::SignInfoResponse,
            // 情话
            models::sweet_talk::SweetTalkRequest,
            models::sweet_talk::SweetTalkOut,
            models::sweet_talk::SweetTalkQuery,
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
            foods::models::food::FoodCreateInput,
            foods::models::food::FoodUpdateInput,
            foods::models::food::FoodOut,
            foods::models::tag::FoodTagOut,
            foods::models::tag::TagCreateInput,
            foods::models::tag::TagUpdateInput,
            foods::models::tag::BatchTagSortInput,
            foods::models::tag::TagSortItem,
            foods::models::food::FoodFilterQuery,
            foods::models::food::FoodMarkActionInput,
            foods::models::food::BlindBoxDrawInput,
            foods::models::food::BlindBoxDrawResultOut,
            // 食材
            foods::models::ingredient::IngredientCreateInput,
            foods::models::ingredient::IngredientUpdateInput,
            foods::models::ingredient::IngredientOut,
            foods::routes::ingredient::IngredientQuery,
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
            crate::wishes::models::WishCreateInput,
            crate::wishes::models::WishUpdateInput,
            crate::wishes::models::WishOut,
            crate::wishes::models::WishQuery,
            crate::wishes::models::WishFeedbackInput,
            crate::wishes::models::WishFeedbackOut,
            dashboard::models::GroupActivityEventOut,
            dashboard::models::TopFoodOrderOut,
            dashboard::models::TopFoodRankingResponse,
            dashboard::models::TodayOrderEntryOut,
            dashboard::models::TodayOrdersResponse,
            dashboard::models::OrderStatsOut,
            dashboard::models::JourneyOrderOut,
            dashboard::models::PointsJourneyOut,
            dashboard::models::WeekOrderDatesOut,
            dashboard::models::WeekDateInfo,
            dashboard::models::DateFoodsResponse,
            dashboard::models::DateFoodOut,

            // IM
            game_im::models::ImUserSigOut,
            game_im::models::ImRoomOut,
            game_im::models::ImRoomListOut,
            game_im::models::ImStartGameOut,
            game_im::models::ImVoteIn,
            game_im::models::ImVoteOut,
            // 微信
            crate::wx::models::WxSubscriptionTemplateOut,
        ),
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "用户", description = "用户相关接口"),
        (name = "团队", description = "团队与关联组相关接口"),
        (name = "游戏", description = "多人在线游戏相关接口"),
        (name = "菜品", description = "菜品库相关接口"),
        (name = "标签", description = "菜品分类标签接口"),
        (name = "食材", description = "食材字典相关接口"),
        (name = "订单", description = "订单流程相关接口"),
        (name = "评分", description = "订单评分与反馈接口"),
        (name = "心愿", description = "心愿清单与兑换接口"),
        (name = "微信", description = "微信服务与消息模板接口"),
        (name = "看板", description = "数据概览与组内动态接口"),
        (name = "签到", description = "签到与打卡相关接口"),
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
