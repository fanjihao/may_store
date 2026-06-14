// OpenAPI 文档自动生成
// 使用 utoipa 自动从代码生成 OpenAPI 3.0 文档
// FSD.latest.md compliant - 100% API coverage
//
// 维护规则:
// 1. paths() 列表必须与实际路由 1:1 对齐(routes.rs::configure 的每个 route)
// 2. components(schemas) 不能重复——每个 type 只列一次
// 3. tag 名必须与各 handler #[utoipa::path(tag = ...)] 完全一致
// 4. 所有需鉴权的 handler 应声明 security(("bearer_auth" = []))
//    公开端点声明 security(())

use utoipa::{Modify, OpenApi};

/// 注册 JWT Bearer 鉴权方案,使 Swagger UI 的 Authorize 按钮可用
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if openapi.components.is_none() {
            openapi.components = Some(utoipa::openapi::Components::new());
        }
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            utoipa::openapi::security::SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "用 wx-login / refresh 取得的 access token 作为 Bearer 凭证。\
                         AdminToken 端点额外要求 admin_users 表中 ACTIVE 状态的记录。",
                    ))
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        // ==================== Auth (认证) ====================
        crate::api::auth::routes::wechat_login,
        crate::api::auth::routes::refresh_token,
        crate::api::auth::routes::logout,
        // ==================== Users (用户中心) ====================
        crate::api::users::get_current_info,
        crate::api::users::update_info,
        crate::api::users::get_user_groups,
        crate::api::users::delete_account,
        // ==================== Groups (双人组) ====================
        crate::api::groups::routes::create_group,
        crate::api::groups::routes::get_group,
        crate::api::groups::routes::swap_role,
        crate::api::groups::routes::settlement_check,
        crate::api::groups::routes::fulfillment_stats,
        crate::api::groups::routes::create_invite,
        crate::api::groups::routes::join_group,
        crate::api::groups::routes::exit_group,
        crate::api::groups::routes::get_group_members,
        crate::api::groups::routes::create_group_order,
        // ==================== Foods CRUD (菜品 §5) ====================
        crate::api::foods::routes::create_food,
        crate::api::foods::routes::list_foods,
        crate::api::foods::routes::get_food,
        crate::api::foods::routes::update_food,
        crate::api::foods::routes::delete_food,
        crate::api::foods::routes::hide_food,
        // ==================== Food Marks (菜品标记 §24.6) ====================
        crate::api::food_marks::routes::mark_food,
        crate::api::food_marks::routes::unmark_food,
        crate::api::food_marks::routes::get_food_mark,
        // ==================== Tags (菜品标签 §24.4) ====================
        crate::api::tags::routes::list_tags,
        crate::api::tags::routes::create_tag,
        crate::api::tags::routes::update_tag,
        crate::api::tags::routes::delete_tag,
        // ==================== Memorial Days (纪念日 §24.9) ====================
        crate::api::memorial_days::routes::list_memorial_days,
        crate::api::memorial_days::routes::create_memorial_day,
        crate::api::memorial_days::routes::get_memorial_day,
        crate::api::memorial_days::routes::update_memorial_day,
        crate::api::memorial_days::routes::delete_memorial_day,
        crate::api::memorial_days::routes::upcoming_memorial_days,
        // ==================== Footprint Groups (足迹分组 §24.10) ====================
        crate::api::footprint_groups::routes::list_footprint_groups,
        crate::api::footprint_groups::routes::create_footprint_group,
        crate::api::footprint_groups::routes::update_footprint_group,
        crate::api::footprint_groups::routes::delete_footprint_group,
        // ==================== Support Tickets (客服工单) ====================
        crate::api::support_tickets::routes::create_ticket,
        crate::api::support_tickets::routes::list_my_tickets,
        crate::api::support_tickets::routes::get_ticket,
        // ==================== Kitchens (主人家厨房) ====================
        crate::api::kitchens::routes::access_kitchen,
        crate::api::kitchens::routes::get_kitchen_foods,
        crate::api::kitchens::routes::create_guest_order,
        // ==================== Economy (经济查询) ====================
        crate::api::economy::routes::get_points_balance,
        crate::api::economy::routes::get_points_transactions,
        crate::api::economy::routes::get_diamonds_balance,
        crate::api::economy::routes::get_diamonds_transactions,
        crate::api::economy::routes::get_group_exp,
        crate::api::economy::routes::get_exp_transactions,
        // ==================== Orders (订单) ====================
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
        // ==================== Wishes (心愿) —— create_group_wish 唯一来源 ====================
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
        // ==================== Admin (后台管理) ====================
        crate::api::admin::routes::get_stats,
        crate::api::admin::routes::list_groups,
        crate::api::admin::routes::list_all_users,
        crate::api::admin::routes::get_config,
        crate::api::admin::routes::update_config,
        crate::api::admin::routes::wish_quality_reward,
        crate::api::admin::routes::order_reward_review,
        crate::api::admin::routes::get_pending_review_orders,
        crate::api::admin::routes::review_order,
        crate::api::admin::routes::get_audit_logs,
        crate::api::admin::routes::update_group_configs,
        crate::api::admin::routes::compensate_points,
        crate::api::admin::routes::compensate_diamonds,
        crate::api::admin::routes::list_pending_food_audits,
        crate::api::admin::routes::audit_food,
        // ==================== Dashboard (数据看板) ====================
        crate::api::dashboard::routes::get_group_dashboard,
        crate::api::dashboard::routes::get_admin_dashboard,
        crate::api::dashboard::routes::get_dashboard_trends,
        // ==================== Footprints (足迹) ====================
        crate::api::footprints::routes::create_footprint,
        crate::api::footprints::routes::list_footprints,
        crate::api::footprints::routes::delete_footprint,
        crate::api::footprints::routes::expand_capacity,
        // ==================== Notifications (通知) ====================
        crate::api::notifications::routes::get_notifications,
        crate::api::notifications::routes::get_unread_count,
        crate::api::notifications::routes::mark_single_as_read,
        crate::api::notifications::routes::mark_all_as_read,
        crate::api::notifications::routes::delete_notification,
        // ==================== Sign-in (签到) ====================
        crate::api::sign_in::routes::sign_in,
        crate::api::sign_in::routes::sign_in_status,
        crate::api::sign_in::routes::get_sign_ins,
        // ==================== Upload (七牛云直传) ====================
        crate::api::upload::routes::get_upload_token,
        crate::api::upload::routes::get_upload_tokens,
        crate::api::upload::routes::confirm_upload,
        crate::api::upload::routes::delete_file,
        // ==================== Achievement (成就) ====================
        crate::api::achievement::routes::get_achievements,
        crate::api::achievement::routes::get_achievement_wall,
        // ==================== WebSocket ====================
        crate::api::ws::ws_info,
        crate::api::ws::ws_status,
    ),
    components(
        schemas(
            // -- Auth --
            crate::api::auth::routes::WechatLoginInput,
            crate::api::auth::routes::WechatLoginResponse,
            crate::api::auth::routes::RefreshTokenInput,
            crate::api::auth::routes::RefreshTokenResponse,
            crate::api::auth::routes::LogoutInput,
            // -- User --
            crate::domain::user::UserPublic,
            crate::api::users::UpdateInfoInput,
            crate::api::users::UserGroupItem,
            crate::api::users::UserGroupsResponse,
            crate::api::users::DeleteAccountInput,
            crate::api::users::DeleteAccountResponse,
            // -- Groups --
            crate::api::groups::routes::CreateGroupResponse,
            // -- Foods --
            crate::api::foods::routes::FoodImage,
            crate::api::foods::routes::FoodIngredient,
            crate::api::foods::routes::FoodStep,
            crate::api::foods::routes::FoodCreateInput,
            crate::api::foods::routes::FoodUpdateInput,
            crate::api::foods::routes::FoodHideInput,
            crate::api::foods::routes::FoodDetail,
            crate::api::foods::routes::FoodSummary,
            crate::api::foods::routes::FoodListResponse,
            // -- Orders --
            crate::api::orders::routes::OrderCancelInput,
            crate::api::orders::routes::OrderRejectInput,
            crate::api::orders::routes::GuestRemarkInput,
            crate::api::orders::routes::OrderConfirmInput,
            // -- Notifications --
            crate::api::notifications::routes::NotificationItem,
            crate::api::notifications::routes::NotificationsResponse,
            crate::api::notifications::routes::UnreadCountResponse,
            crate::api::notifications::routes::BatchMarkReadRequest,
            // -- Upload --
            crate::api::upload::routes::UploadTokenRequest,
            crate::api::upload::routes::UploadTokenResponse,
            crate::api::upload::routes::UploadTokensRequest,
            crate::api::upload::routes::UploadTokensResponse,
            crate::api::upload::routes::UploadTokenItem,
            crate::api::upload::routes::UploadTokenFileItem,
            crate::api::upload::routes::BusinessRefType,
            crate::api::upload::routes::ConfirmUploadRequest,
            crate::api::upload::routes::ConfirmUploadResponse,
            crate::api::upload::routes::QiniuCallbackRequest,
            crate::api::upload::routes::DeleteFileResponse,
            crate::api::upload::routes::ErrorBody,
            // -- Admin --
            crate::api::admin::routes::StatsResponse,
            crate::api::admin::routes::GroupListItem,
            crate::api::admin::routes::UserListItem,
            crate::api::admin::routes::ConfigResponse,
            crate::api::admin::routes::UpdateConfigInput,
            crate::api::admin::routes::WishQualityRewardInput,
            crate::api::admin::routes::WishQualityRewardResponse,
            crate::api::admin::routes::OrderRewardReviewInput,
            crate::api::admin::routes::OrderRewardReviewResponse,
            crate::api::admin::routes::UpdateGroupConfigsInput,
            crate::api::admin::routes::CompensatePointsInput,
            crate::api::admin::routes::CompensateDiamondsInput,
            crate::api::admin::routes::PendingReviewQuery,
            crate::api::admin::routes::ReviewOrderInput,
            crate::api::admin::routes::AuditLogQuery,
            // -- Sign-in --
            crate::api::sign_in::routes::SignInStatusResponse,
            crate::api::sign_in::routes::DailyCheckinResponse,
            // -- Footprints --
            crate::api::footprints::routes::CreateFootprintRequest,
            crate::api::footprints::routes::FootprintItem,
            crate::api::footprints::routes::FootprintsListResponse,
            crate::api::footprints::routes::ExpandCapacityRequest,
            crate::api::footprints::routes::ExpandCapacityResponse,
        )
    ),
    tags(
        (name = "认证", description = "微信登录、刷新令牌、注销"),
        (name = "用户", description = "用户中心、资料管理"),
        (name = "双人组", description = "双人组管理与角色"),
        (name = "菜品", description = "菜品 CRUD (§5)"),
        (name = "菜品标记 (§24.6)", description = "用户对菜品的 LIKE / NOT_RECOMMEND"),
        (name = "菜品标签 (§24.4)", description = "组内菜品标签管理"),
        (name = "纪念日 (§24.9)", description = "组内纪念日管理"),
        (name = "足迹分组 (§24.10)", description = "足迹分组管理"),
        (name = "客服工单", description = "用户提交客服工单"),
        (name = "做客厨房", description = "做客系统"),
        (name = "订单", description = "订单创建与状态管理"),
        (name = "评分", description = "订单评分"),
        (name = "心愿", description = "心愿协商、选择与履约"),
        (name = "经济查询", description = "积分、钻石、经验查询"),
        (name = "签到", description = "每日签到与奖励"),
        (name = "足迹", description = "组内足迹"),
        (name = "成就", description = "成就系统"),
        (name = "通知", description = "系统通知"),
        (name = "文件上传（七牛直传）", description = "对象存储上传"),
        (name = "后台管理", description = "管理员功能"),
        (name = "数据看板", description = "运营数据看板"),
        (name = "WebSocket", description = "实时通知")
    ),
    info(
        title = "心愿菜单 API",
        description = "心愿菜单生产级 API 文档 - FSD.latest.md compliant",
        version = "1.0.0",
        contact(name = "API Support", email = "api@example.com")
    )
)]
pub struct ApiDoc;

/// 生成 OpenAPI 3.0 JSON 文档
pub fn openapi_json() -> String {
    ApiDoc::openapi().to_pretty_json().unwrap_or_default()
}
