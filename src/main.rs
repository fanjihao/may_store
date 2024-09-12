mod errors;
mod utils;
mod models;
mod upload;
mod users;
mod foods;
mod orders;
mod wx_official;

use dotenvy::dotenv;
use errors::CustomError;
use idgenerator::{IdGeneratorOptions, IdInstance};
use ntex::web::{self, middleware, App, HttpServer};
use orders::update::check_order_expiration;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::{env, sync::Arc};

#[derive(Debug, Clone)]
pub struct AppState {
    pub db_pool: Pool<Postgres>,
}

#[ntex::main]
async fn main() -> Result<(), CustomError> {
    dotenv().ok();

    // log
    env::set_var("RUST_LOG", "ntex=info");
    env_logger::init();

    // 雪花id
    let options = IdGeneratorOptions::new().worker_id(1).worker_id_bit_len(6);
    let _ = IdInstance::init(options)?;

    let db_url = env::var("DATABASE_URL").expect("Please set DATABASE_URL");

    // state
    let app_state: Arc<AppState> = Arc::new(AppState {
        db_pool: PgPoolOptions::new()
            .max_connections(100)
            .connect(&db_url)
            .await?,
    });
    let app_state_clone = Arc::clone(&app_state);

    let server = HttpServer::new(move || {
        App::new()
            .state(Arc::clone(&app_state))
            .wrap(middleware::Logger::default())
            .configure(|cfg| route(Arc::clone(&app_state), cfg))
    })
    .bind("0.0.0.0:9831")?
    .run();

    // 启动 HTTP 服务器
    let server_handle = tokio::spawn(server);

    // 定时任务：每分钟检查订单失效时间
    let task = tokio::spawn(check_order_expiration(app_state_clone));

    // 等待 HTTP 服务器和定时任务完成
    let _ = tokio::try_join!(server_handle, task)?;

    Ok(())
}

fn route(_state: Arc<AppState>, cfg: &mut web::ServiceConfig) {
    cfg
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
            .route("/update", web::post().to(foods::update::update_record))
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
        .service( // 菜品标签
            web::scope("/foodtag")
            .route("", web::get().to(foods::view::get_tags))
        )
        .service( // 订单
            web::scope("/orders")
            .route("", web::get().to(orders::view::get_orders))
            .route("/{id}", web::get().to(orders::view::get_order_detail))
            .route("", web::post().to(orders::new::create_order))
            .route("", web::put().to(orders::update::update_order))
            .route("/{id}", web::delete().to(orders::delete::delete_order))
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
