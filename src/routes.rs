use std::sync::Arc;
use ntex::web;
use crate::{dashboard, foods, openapi::{openapi_json, serve_swagger}, orders, upload, users, wishes, wx_official, AppState};

pub fn route(_state: Arc<AppState>, cfg: &mut web::ServiceConfig) {
    cfg
        .service(
            web::scope("/api-doc/openapi.json")
            .route("", web::get().to(openapi_json))
        )
        .service(
            web::scope("/swagger-ui")
                .route("/{tail:.*}", web::get().to(serve_swagger))
        )
        // 个人中心
        .service( // 注册
            web::scope("/wx-register")
            .route("", web::post().to(users::new::wx_register))
        )
        .service( // 登录
            web::scope("/wx-login")
            .route("", web::post().to(users::view::wx_login))
        )
        .service( // 用户
            web::scope("/users")
            .route("", web::get().to(users::view::get_user_info))
            .route("", web::post().to(users::update::wx_change_info))
            .route("/is-register", web::get().to(users::view::is_register))
        )
        .service( // 关联
            web::scope("/invitation")
            .route("", web::get().to(users::invitation::get_invitation))
            .route("", web::post().to(users::invitation::new_invitation))
            .route("/{id}", web::put().to(users::invitation::confirm_invitation))
            .route("/{id}", web::delete().to(users::invitation::cancel_invitation)),
        )
        .service( // 上传
            web::scope("/upload")
            .route("", web::post().to(upload::upload::upload_file))
        )
        .service(
            web::scope("/upload-token")
            .route("", web::get().to(upload::upload::get_qiniu_token))
        )
        .service( // 菜品
            web::scope("/food")
            .route("/records", web::get().to(foods::view::apply_record))
            .route("/apply", web::post().to(foods::new::new_food_apply))
            .route("/update_status", web::post().to(foods::update::update_record_status))
            .route("/update", web::post().to(foods::update::food_update))
            .route("/delete/{id}", web::delete().to(foods::update::delete_record))
            .route("/mark/{id}", web::put().to(foods::update::favorite_dishes))
        )
        .service( // 菜品
            web::scope("/dishes")
            .route("", web::post().to(foods::view::get_foods))
        )
        .service( // 菜品类型
            web::scope("/foodclass")
            .route("", web::get().to(foods::view::all_food_class))
        )
        .service( // 足迹
            web::scope("/footprints")
            .route("/{id}", web::get().to(orders::footprints::footprints_list))
        )
        .service( // 菜品标签
            web::scope("/foodtag")
            .route("", web::get().to(foods::view::get_tags))
            .route("", web::post().to(foods::new::create_tags))
            .route("/{id}", web::delete().to(foods::update::delete_tags))
            .route("/sort", web::put().to(foods::update::update_tags_sort))
        )
        .service( // 订单
            web::scope("/orders")
            .route("", web::get().to(orders::view::get_orders))
            .route("/{id}", web::get().to(orders::view::get_order_detail))
            .route("", web::post().to(orders::new::create_order))
            .route("", web::put().to(orders::update::update_order))
            .route("/{id}", web::delete().to(orders::delete::delete_order))
            .route("/incomplete/{id}", web::get().to(orders::view::get_incomplete_order))
        )
        .service( // 订单
            web::scope("/dashboard")
            .route("/ranking", web::get().to(dashboard::view::order_ranking))
            .route("/collect", web::get().to(dashboard::view::order_collect))
            .route("/today-order", web::get().to(dashboard::view::today_order))
            .route("/today-points", web::get().to(dashboard::view::today_points))
            .route("/lottery", web::post().to(dashboard::view::lottery))
        )
        .service( // 心愿兑换
            web::scope("/wishes")
            .route("", web::get().to(wishes::view::all_wishes))
            .route("", web::post().to(wishes::new::new_wishes))
            .route("", web::put().to(wishes::update::clock_in_wish))
            .route("/unlocked", web::put().to(wishes::update::update_wish_status))
            .route("/{id}", web::delete().to(wishes::delete::delete_wishes))
        )
        .service( // 公众号
            web::scope("/wxOffical")
                .route("", web::get().to(wx_official::verify::wx_offical_account))
                .route("", web::post().to(wx_official::verify::wx_offical_received)),
        )
        .service(
            web::scope("/create-menu")
                .route("", web::get().to(wx_official::verify::wx_offical_create_menu)),
        )
        .service(
            web::scope("/template")
                .route("", web::post().to(wx_official::send_to_user::send_template)),
        )
        .service(
            web::scope("/weather")
                .route("", web::get().to(wx_official::send_to_user::get_weather)),
        );
}
