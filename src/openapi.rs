// OpenAPI 文档自动生成
// 使用 utoipa 自动从代码生成 OpenAPI 3.0 文档

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Achievement (成就)
        crate::api::achievement::routes::get_achievements,
        crate::api::achievement::routes::get_achievement_wall,
        // Auth (微信登录)
        crate::api::auth::routes::wechat_login,
        // Groups (双人组管理)
        crate::api::groups::routes::create_group,
        crate::api::groups::routes::get_group,
        crate::api::groups::routes::swap_role,
        crate::api::groups::routes::settlement_check,
        crate::api::groups::routes::fulfillment_stats,
        crate::api::groups::routes::create_invite,
        crate::api::groups::routes::list_foods,
        crate::api::groups::routes::create_group_order,
        crate::api::groups::routes::create_group_wish,
        // Kitchens (主人家厨房)
        crate::api::kitchens::routes::access_kitchen,
        crate::api::kitchens::routes::get_kitchen_foods,
        crate::api::kitchens::routes::create_guest_order,
        // Economy (经济查询)
        crate::api::economy::routes::get_points_balance,
        crate::api::economy::routes::get_points_transactions,
        crate::api::economy::routes::get_diamonds_balance,
        crate::api::economy::routes::get_diamonds_transactions,
        crate::api::economy::routes::get_group_exp,
        crate::api::economy::routes::get_exp_transactions,
        // Orders (订单)
        crate::api::orders::routes::create_order,
        crate::api::orders::routes::get_orders,
        crate::api::orders::routes::get_order_detail,
        crate::api::orders::routes::create_order_rating,
        crate::api::orders::routes::get_order_rating,
        crate::api::orders::routes::accept_order,
        crate::api::orders::routes::complete_order,
        crate::api::orders::routes::confirm_order,
        crate::api::orders::routes::cancel_order,
        crate::api::orders::routes::reject_order,
        crate::api::orders::routes::order_timeout,
        crate::api::orders::routes::update_guest_remark,
        // Wishes (心愿)
        crate::api::wishes::routes::create_group_wish,
        crate::api::wishes::routes::list_group_wishes,
        crate::api::wishes::routes::get_wish,
        crate::api::wishes::routes::wish_quote,
        crate::api::wishes::routes::wish_deadline,
        crate::api::wishes::routes::wish_confirm_agreement,
        crate::api::wishes::routes::wish_reject,
        crate::api::wishes::routes::wish_select,
        crate::api::wishes::routes::submit_feedback,
        crate::api::wishes::routes::wish_close,
        crate::api::wishes::routes::wish_expire,
        crate::api::wishes::routes::get_wish_checkins,
        crate::api::wishes::routes::pending_fulfillment,
        // Admin (后台管理)
        crate::api::admin::routes::get_stats,
        crate::api::admin::routes::list_groups,
        crate::api::admin::routes::list_all_users,
        crate::api::admin::routes::get_config,
        crate::api::admin::routes::update_config,
        crate::api::admin::routes::wish_quality_reward,
        crate::api::admin::routes::order_reward_review,
        // Dashboard (数据看板)
        crate::api::dashboard::routes::get_group_dashboard,
        crate::api::dashboard::routes::get_admin_dashboard,
        crate::api::dashboard::routes::get_dashboard_trends,
        // Users (用户)
        crate::api::users::register,
        crate::api::users::login,
        crate::api::users::get_current_info,
        crate::api::users::get_user_info,
        crate::api::users::is_register,
        crate::api::users::change_info,
        // WebSocket
        crate::api::ws::ws_info,
        crate::api::ws::ws_status,
        // Footprints (足迹)
        crate::api::footprints::routes::create_footprint,
        crate::api::footprints::routes::list_footprints,
        crate::api::footprints::routes::delete_footprint,
        crate::api::footprints::routes::expand_capacity,
        // Notifications (通知)
        crate::api::notifications::routes::get_notifications,
        crate::api::notifications::routes::get_unread_count,
        crate::api::notifications::routes::mark_single_as_read,
        crate::api::notifications::routes::mark_all_as_read,
        crate::api::notifications::routes::delete_notification,
        // Sign-in (签到)
        crate::api::sign_in::routes::sign_in,
        crate::api::sign_in::routes::sign_in_status,
        crate::api::sign_in::routes::get_sign_ins,
        // Upload (文件上传)
        crate::api::upload::routes::get_presigned_url,
        crate::api::upload::routes::get_presigned_urls,
        crate::api::upload::routes::confirm_upload,
        crate::api::upload::routes::delete_file,
    )
)]
pub struct ApiDoc;

/// 生成 OpenAPI 3.0 JSON 文档
pub fn openapi_json() -> String {
    ApiDoc::openapi().to_pretty_json().unwrap_or_default()
}
