// OpenAPI 文档自动生成
// 使用 utoipa 自动从代码生成 OpenAPI 3.0 文档

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Auth
        crate::api::auth::routes::wechat_login,
        // Groups
        crate::api::groups::routes::create_group,
        crate::api::groups::routes::get_group,
        crate::api::groups::routes::swap_role,
        crate::api::groups::routes::settlement_check,
        crate::api::groups::routes::fulfillment_stats,
        crate::api::groups::routes::create_invite,
        crate::api::groups::routes::list_foods,
        crate::api::groups::routes::create_group_order,
        crate::api::groups::routes::create_group_wish,
        // Kitchens
        crate::api::kitchens::routes::access_kitchen,
        crate::api::kitchens::routes::get_kitchen_foods,
        crate::api::kitchens::routes::create_guest_order,
        // Economy
        crate::api::economy::routes::get_points,
        crate::api::economy::routes::get_transactions,
        crate::api::economy::routes::get_group_exp,
        // Orders
        crate::api::orders::routes::create_order,
        crate::api::orders::routes::get_orders,
        crate::api::orders::routes::get_order_detail,
        crate::api::orders::routes::create_order_rating,
        crate::api::orders::routes::get_order_rating,
        crate::api::orders::routes::accept_order,
        crate::api::orders::routes::complete_order,
        crate::api::orders::routes::confirm_order,
        // Wishes
        crate::api::wishes::routes::list_wishes,
        crate::api::wishes::routes::get_wish,
        crate::api::wishes::routes::submit_feedback,
        crate::api::wishes::routes::wish_quote,
        crate::api::wishes::routes::wish_deadline,
        crate::api::wishes::routes::wish_confirm_agreement,
        crate::api::wishes::routes::wish_reject,
        crate::api::wishes::routes::wish_select,
        // Admin
        crate::api::admin::routes::get_stats,
        crate::api::admin::routes::list_groups,
        crate::api::admin::routes::list_all_users,
        crate::api::admin::routes::get_config,
        crate::api::admin::routes::update_config,
        crate::api::admin::routes::wish_quality_reward,
        crate::api::admin::routes::order_reward_review,
        // Users
        crate::api::users::register,
        crate::api::users::login,
        crate::api::users::get_current_info,
        crate::api::users::get_user_info,
        crate::api::users::is_register,
        crate::api::users::change_info,
        // WebSocket
        crate::api::ws::ws_info,
        crate::api::ws::ws_status,
        // Footprints
        crate::api::footprints::routes::get_overview,
        crate::api::footprints::routes::list_record_groups,
        crate::api::footprints::routes::create_record,
        crate::api::footprints::routes::submit_record,
        crate::api::footprints::routes::get_record,
        crate::api::footprints::routes::update_record,
        crate::api::footprints::routes::delete_record,
        crate::api::footprints::routes::list_records,
        // Notifications
        crate::api::notifications::routes::get_unread_count,
        crate::api::notifications::routes::mark_as_read,
        // Sign-in
        crate::api::sign_in::routes::daily_sign_in,
        crate::api::sign_in::routes::get_sign_info,
    )
)]
pub struct ApiDoc;

/// 生成 OpenAPI 3.0 JSON 文档
pub fn openapi_json() -> String {
    ApiDoc::openapi().to_pretty_json().unwrap_or_default()
}
