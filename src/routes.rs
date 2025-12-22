use crate::{
    dashboard,
    foods,
    openapi::{ openapi_json, serve_swagger },
    orders,
    upload,
    users,
    wishes,
    AppState,
};
use ntex::web;
use std::sync::Arc;

pub fn route(_state: Arc<AppState>, cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api-doc/openapi.json").route("", web::get().to(openapi_json)))
        .service(web::scope("/swagger-ui").route("/{tail:.*}", web::get().to(serve_swagger)))
        // 个人中心
        .service(
            // 注册
            web::scope("/register").route("", web::post().to(users::new::register))
        )
        .service(
            // 登录
            web::scope("/login").route("", web::post().to(users::view::login))
        )
        .service(
            // 用户
            web
                ::scope("/users")
                .route("", web::get().to(users::view::get_current_info))
                .route("", web::post().to(users::update::change_info))
                .route("/is-register", web::get().to(users::view::is_register))
                .route("/role-switch", web::post().to(users::role::switch_role))
                .route("/checkin", web::post().to(users::checkin::daily_checkin))
                .route("/getInfoByUsername", web::get().to(users::view::get_user_info))
        )
        .service(
            // 关联
            web
                ::scope("/invitation")
                .route("", web::get().to(users::invitation::get_invitation))
                .route("", web::post().to(users::invitation::new_invitation))
                .route("/{id}", web::put().to(users::invitation::confirm_invitation))
                .route("/{id}", web::delete().to(users::invitation::cancel_invitation))
                .route("/unbind", web::post().to(users::invitation::unbind_request))
                .route("/bind", web::post().to(users::invitation::bind_user_directly))
                .route("/group/{id}", web::get().to(users::invitation::get_group_info))
                .route("/groups/{group_id}", web::put().to(users::group_update::update_group))
        );

    // 菜品相关新路由
    cfg.service(
        web
            ::scope("/foods")
            .route("", web::post().to(foods::new::create_food))
            .route("", web::get().to(foods::view::get_foods))
            .route("/marks", web::get().to(foods::view::get_marked_foods))
            .route("/blind_box/draw", web::post().to(foods::view::draw_blind_box))
            .route("/mark", web::post().to(foods::update::mark_food))
            .route("/mark/{food_id}/{mark_type}", web::delete().to(foods::update::unmark_food))
            .route("/{id}", web::get().to(foods::view::get_food_detail))
            .route("/{id}", web::put().to(foods::update::update_food))
            .route("/{id}", web::delete().to(foods::delete::delete_food))
    );
    cfg.service(
        web
            ::scope("/food_tags")
            .route("", web::post().to(foods::new::create_tag))
            .route("", web::get().to(foods::view::get_tags))
            .route("/{id}", web::delete().to(foods::delete::delete_tag))
    );

    // 订单相关路由
    cfg.service(
        web
            ::scope("/orders")
            .route("", web::post().to(orders::new::create_order))
            .route("", web::get().to(orders::view::get_orders))
            .route("/status", web::put().to(orders::update::update_order_status))
            .route("/{id}", web::get().to(orders::view::get_order_detail))
            .route("/{id}", web::delete().to(orders::delete::delete_order))
    );
    // 订单评分
    cfg.service(
        web
            ::scope("/orders-rating")
            .route("/{order_id}", web::post().to(orders::rating::create_order_rating))
            .route("/{order_id}", web::get().to(orders::rating::get_order_rating))
    );
    cfg.service(
        web
            ::scope("/orders-incomplete")
            .route("/{id}", web::get().to(orders::view::get_incomplete_order))
    );

    // 心愿相关路由
    cfg.service(
        web
            ::scope("/wishes")
            .route("", web::post().to(wishes::new::create_wish))
            .route("", web::get().to(wishes::view::get_wishes))
            .route("/{id}", web::get().to(wishes::view::get_wish_detail))
            .route("/{id}", web::delete().to(wishes::update::disable_wish))
    );
    cfg.service(
        web
            ::scope("/wish_claims")
            .route("", web::post().to(wishes::claim::claim_wish))
            .route("", web::get().to(wishes::claim::get_claim))
            .route("/status", web::put().to(wishes::claim::update_wish_claim))
            .route(
                "/{claim_id}/checkins",
                web::post().to(wishes::checkin::create_wish_claim_checkin)
            )
            .route("/{claim_id}/checkins", web::get().to(wishes::checkin::list_wish_claim_checkins))
    );
    cfg.service(
        web::scope("/upload-token").route("", web::get().to(upload::upload::get_qiniu_token))
    );

    // 看板 / 组活动
    cfg.service(
        web
            ::scope("/groups")
            .route(
                "/{group_id}/activities",
                web::get().to(dashboard::activities::get_group_activities)
            )
    );
    // 看板 / 综合指标
    cfg.service(
        web
            ::scope("/dashboard")
            .route("/top-foods", web::get().to(dashboard::metrics::get_top_food_orders))
            .route("/my/orders-today", web::get().to(dashboard::metrics::get_my_today_orders))
            .route("/my/order-stats", web::get().to(dashboard::metrics::get_my_order_stats))
            .route("/my/points-journey", web::get().to(dashboard::metrics::get_points_journey))
    );
}
