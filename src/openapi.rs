use ntex::web::{HttpRequest, HttpResponse};
use std::sync::Arc;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

// Re-export model modules for macro path resolution
use crate::{dashboard, foods, game_im, orders, upload, users};

#[derive(OpenApi)]
#[openapi(
    paths(
        // 用户 (User)
        users::routes::user::register,
        users::routes::user::login,
        users::routes::user::get_current_info,
        users::routes::user::change_info,
        users::routes::user::is_register,
        users::routes::user::switch_role,
        users::routes::user::get_user_info,
        users::routes::sweet_talk::add_sweet_talk,
        users::routes::sweet_talk::update_sweet_talk,
        users::routes::sweet_talk::get_sweet_talks,

        // 团队 (Team)
        users::routes::group::get_invitation,
        users::routes::group::new_invitation,
        users::routes::group::confirm_invitation,
        users::routes::group::cancel_invitation,
        users::routes::group::unbind_request,
        users::routes::group::bind_user_directly,
        users::routes::group::get_group_info,
        users::routes::group::update_group,

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
        orders::routes::create_order,
        orders::routes::get_orders,
        orders::routes::update_order_status,
        orders::routes::get_order_detail,
        orders::routes::delete_order,
        orders::routes::get_incomplete_order,

        // 评分 (Rating)
        orders::routes::create_order_rating,
        orders::routes::get_order_rating,

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
        users::routes::sign::daily_checkin,
        users::routes::sign::sign_in,
        users::routes::sign::get_sign_info,

        // 上传
        upload::routes::get_qiniu_token,
    ),
    components(
        // 用户
        schemas(
            users::models::user::RegisterInput,
            users::models::user::ProfileUpdateInput,
            users::models::user::LoginInput,
            users::models::user::LoginResponse,
            users::models::user::UserPublic,
            users::models::user::IsRegisterResponse,
            users::models::user::DailyCheckinOut,
            users::models::user::RoleSwitchResult,
            users::models::user::RoleSwitchInput,
            // 签到
            users::models::sign::SignInResponse,
            users::models::sign::SignRecordOut,
            users::models::sign::SignInfoResponse,
            // 情话
            users::models::sweet_talk::SweetTalkRequest,
            users::models::sweet_talk::SweetTalkOut,
            users::models::sweet_talk::SweetTalkQuery,
        ),
        // 邀请
        schemas(
            users::models::group::NewInvitationInput,
            users::models::group::ConfirmInvitationInput,
            users::models::group::InvitationRequestOut,
            users::models::group::InvitationListOut,
            users::models::group::GroupMemberOut,
            users::models::group::GroupInfoOut,
            users::models::group::UnbindRequestInput,
            users::models::group::GroupUpdateInput,
            users::models::group::BindUserDirectlyInput,
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
            foods::models::ingredient::IngredientQuery,
        ),
        // 订单新模型
        schemas(
            orders::models::OrderCreateInput,
            orders::models::OrderStatusUpdateInput,
            orders::models::OrderItemOut,
            orders::models::OrderStatusHistoryOut,
            orders::models::OrderOutNew,
            orders::models::OrderStatusUpdateInput,
            orders::models::OrderQuery,
            orders::models::OrderRatingCreateInput,
            orders::models::OrderRatingOut,
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
