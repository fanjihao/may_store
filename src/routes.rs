use crate::{
    config::AppState,
    dashboard, foods, game_im, game_ws,
    openapi::{openapi_json, serve_swagger},
    orders, upload, users, wishes, wx,
};
use ntex::web;
use std::sync::Arc;

pub fn route(_state: Arc<AppState>, cfg: &mut web::ServiceConfig) {
    // API Documentation & Misc
    cfg.service(web::scope("/api-doc/openapi.json").route("", web::get().to(openapi_json)))
        .service(web::scope("/swagger-ui").route("/{tail:.*}", web::get().to(serve_swagger)))
        .service(
            web::scope("/upload-token").route("", web::get().to(upload::routes::get_qiniu_token)),
        );

    game_routes(cfg);
    user_routes(cfg);
    team_routes(cfg);
    dish_routes(cfg);
    tag_routes(cfg);
    ingredient_routes(cfg);
    order_routes(cfg);
    rating_routes(cfg);
    wish_routes(cfg);
    checkin_routes(cfg);
    wechat_routes(cfg);
    dashboard_routes(cfg);
}

/// 游戏 (Game)
fn game_routes(cfg: &mut web::ServiceConfig) {
    // Socket mode (self-hosted WebSocket)
    cfg.service(web::resource("/ws/game").route(web::get().to(game_ws::routes::ws_game)));
    cfg.service(
        web::resource("/game/room-code").route(web::get().to(game_ws::routes::get_room_code)),
    );

    // Tencent Cloud IM
    cfg.service(web::resource("/im/usersig").route(web::get().to(game_im::routes::get_user_sig)));

    // Mini-game (IM based)
    cfg.service(web::resource("/game/rooms").route(web::get().to(game_im::routes::list_rooms)));
    cfg.service(
        web::resource("/game/rooms/{group_id}/start")
            .route(web::post().to(game_im::routes::start_game)),
    );
    cfg.service(
        web::resource("/game/rooms/{group_id}/vote").route(web::post().to(game_im::routes::vote)),
    );
}

/// 用户 (User)
fn user_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        // 注册
        web::scope("/register").route("", web::post().to(users::routes::user::register)),
    )
    .service(
        // 登录
        web::scope("/login").route("", web::post().to(users::routes::user::login)),
    )
    .service(
        // 用户
        web::scope("/users")
            .route("", web::get().to(users::routes::user::get_current_info))
            .route("", web::post().to(users::routes::user::change_info))
            .route("/is-register", web::get().to(users::routes::user::is_register))
            .route("/role-switch", web::post().to(users::routes::user::switch_role))
            .route(
                "/getInfoByUsername",
                web::get().to(users::routes::user::get_user_info),
            )
            .route(
                "/sweet-talk",
                web::post().to(users::routes::sweet_talk::add_sweet_talk),
            )
            .route(
                "/sweet-talk/{id}",
                web::put().to(users::routes::sweet_talk::update_sweet_talk),
            )
            .route(
                "/sweet-talks",
                web::get().to(users::routes::sweet_talk::get_sweet_talks),
            ),
    );
}

/// 团队 (Team)
fn team_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        // 关联
        web::scope("/invitation")
            .route("", web::get().to(users::routes::group::get_invitation))
            .route("", web::post().to(users::routes::group::new_invitation))
            .route(
                "/{id}",
                web::put().to(users::routes::group::confirm_invitation),
            )
            .route(
                "/{id}",
                web::delete().to(users::routes::group::cancel_invitation),
            )
            .route("/unbind", web::post().to(users::routes::group::unbind_request))
            .route(
                "/bind",
                web::post().to(users::routes::group::bind_user_directly),
            )
            .route(
                "/group/{id}",
                web::get().to(users::routes::group::get_group_info),
            )
            .route(
                "/groups/{group_id}",
                web::put().to(users::routes::group::update_group),
            ),
    );
}

/// 菜品 (Dish)
fn dish_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/foods")
            .route("", web::post().to(foods::routes::food::create_food))
            .route("", web::get().to(foods::routes::food::get_foods))
            .route(
                "/marks",
                web::get().to(foods::routes::food::get_marked_foods),
            )
            .route(
                "/blind_box/draw",
                web::post().to(foods::routes::food::draw_blind_box),
            )
            .route("/mark", web::post().to(foods::routes::food::mark_food))
            .route(
                "/mark/{food_id}/{mark_type}",
                web::delete().to(foods::routes::food::unmark_food),
            )
            .route("/{id}", web::get().to(foods::routes::food::get_food_detail))
            .route("/{id}", web::put().to(foods::routes::food::update_food))
            .route("/{id}", web::delete().to(foods::routes::food::delete_food)),
    );
}

/// 标签 (Tag)
fn tag_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/food_tags")
            .route("", web::post().to(foods::routes::tag::create_tag))
            .route("", web::get().to(foods::routes::tag::get_tags))
            .route(
                "/sort",
                web::post().to(foods::routes::tag::update_tags_sort),
            )
            .route("/{id}", web::put().to(foods::routes::tag::update_tag))
            .route("/{id}", web::delete().to(foods::routes::tag::delete_tag)),
    );
}

/// 食材 (Ingredient)
fn ingredient_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/ingredients")
            .route(
                "",
                web::get().to(foods::routes::ingredient::list_ingredients),
            )
            .route(
                "",
                web::post().to(foods::routes::ingredient::create_ingredient),
            )
            .route(
                "/sort",
                web::post().to(foods::routes::ingredient::update_ingredients_sort),
            )
            .route(
                "/{id}",
                web::get().to(foods::routes::ingredient::get_ingredient),
            )
            .route(
                "/{id}",
                web::put().to(foods::routes::ingredient::update_ingredient),
            )
            .route(
                "/{id}",
                web::delete().to(foods::routes::ingredient::delete_ingredient),
            ),
    );
}

/// 订单 (Order)
fn order_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/orders")
            .route("", web::post().to(orders::routes::create_order))
            .route("", web::get().to(orders::routes::get_orders))
            .route(
                "/status",
                web::put().to(orders::routes::update_order_status),
            )
            .route("/{id}", web::get().to(orders::routes::get_order_detail))
            .route("/{id}", web::delete().to(orders::routes::delete_order)),
    )
    .service(
        web::scope("/orders-incomplete")
            .route("/{id}", web::get().to(orders::routes::get_incomplete_order)),
    );
}

/// 评分 (Rating)
fn rating_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/orders-rating")
            .route(
                "/{order_id}",
                web::post().to(orders::routes::create_order_rating),
            )
            .route(
                "/{order_id}",
                web::get().to(orders::routes::get_order_rating),
            ),
    );
}

/// 心愿 (Wish)
fn wish_routes(cfg: &mut web::ServiceConfig) {
    // 1. Core Wish Resources
    cfg.service(
        web::scope("/wishes")
            .route("", web::get().to(wishes::routes::list_wishes))
            .route("", web::post().to(wishes::routes::create_wish))
            .route("/{id}", web::get().to(wishes::routes::get_wish))
            .route("/{id}", web::put().to(wishes::routes::update_wish))
            .route("/{id}", web::delete().to(wishes::routes::delete_wish))
            // 2. Actions
            .route("/{id}/redeem", web::post().to(wishes::routes::redeem_wish))
            .route(
                "/{id}/feedback",
                web::put().to(wishes::routes::submit_feedback),
            ),
    );
}

/// 签到 (Check-in)
fn checkin_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/users").route("/checkin", web::post().to(users::routes::sign::daily_checkin)),
    )
    .service(
        web::scope("/sign")
            .route("", web::post().to(users::routes::sign::sign_in))
            .route("/info", web::get().to(users::routes::sign::get_sign_info)),
    );
}

/// 微信 (WeChat)
fn wechat_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/wx")
            .route("/sign-verify", web::get().to(wx::routes::wx_sign_verify))
            .route(
                "/sign-verify",
                web::post().to(wx::routes::wx_offical_received),
            )
            .route("/templates", web::get().to(wx::routes::get_templates)),
    );
}

/// 看板 (Dashboard)
fn dashboard_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/groups").route(
        "/{group_id}/activities",
        web::get().to(dashboard::routes::get_group_activities),
    ))
    .service(
        web::scope("/dashboard")
            .route(
                "/top-foods",
                web::get().to(dashboard::routes::get_top_food_orders),
            )
            .route(
                "/my/orders-today",
                web::get().to(dashboard::routes::get_my_today_orders),
            )
            .route(
                "/my/order-stats",
                web::get().to(dashboard::routes::get_my_order_stats),
            )
            .route(
                "/my/points-journey",
                web::get().to(dashboard::routes::get_points_journey),
            )
            .route(
                "/week-order-dates",
                web::get().to(dashboard::routes::get_week_order_dates),
            )
            .route(
                "/date-foods",
                web::get().to(dashboard::routes::get_date_foods),
            ),
    );
}
