use std::sync::Arc;

use ntex::web::{HttpRequest, HttpResponse};
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::Config;

use crate::errors::CustomError;
use crate::models;

pub async fn openapi_json() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .json(&ApiDoc::openapi())
}

pub async fn serve_swagger(req: HttpRequest) -> HttpResponse {
    let mut config = Config::from("/api-doc/openapi.json");
    
    // 设置 Swagger UI 的配置
    config = config.persist_authorization(true);
        
    let config = Arc::new(config);
    
    let path = req.uri().path();
    let tail = path.strip_prefix("/swagger-ui/")
        .unwrap_or_default();

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
        Err(_) => HttpResponse::InternalServerError().finish()
    }
}
use crate::users::{
    view::*,
    new::*,
    update::*,
    invitation::*,
};
use crate::foods::{
    view::*,
    new::*,
    update::*,
};
use crate::orders::{
    delete::*,
    view::*,
    new::*,
    update::*,
};
use crate::wishes::{
    delete::*,
    view::*,
    new::*,
    update::*,
};
use crate::dashboard::view::*;
use crate::upload::upload::*;
#[derive(OpenApi)]
#[openapi(
    paths(
        wx_login,
        wx_register,
        wx_change_info,
        get_user_info,
        is_register,
        get_invitation,
        new_invitation,
        confirm_invitation,
        cancel_invitation,
        apply_record,
        all_food_class,
        get_foods,
        get_tags,
        update_record,
        delete_record,
        favorite_dishes,
        new_food_apply,
        // dashboard
        today_points,
        order_collect,
        order_ranking,
        today_order,
        // orders
        delete_order,
        create_order,
        update_order,
        get_orders,
        get_order_detail,
        // upload
        upload_file,
        // wishes
        delete_wishes,
        new_wishes,
        clock_in_wish,
        update_wish_status,
        all_wishes
    ),
    components(
        schemas(models::users::Login, CustomError),
        schemas(models::users::UserInfo, CustomError),
        schemas(models::users::IsRegister, CustomError),
        schemas(models::invitation::Invitation, CustomError),
        schemas(models::invitation::BindStruct, CustomError),
        schemas(models::foods::FoodApply, CustomError),
        schemas(models::foods::FoodApplyStruct, CustomError),
        schemas(models::foods::ShowClass, CustomError),
        schemas(models::foods::DishesByType, CustomError),
        schemas(models::foods::FoodTags, CustomError),
        schemas(models::foods::UpdateFood, CustomError),
        schemas(models::foods::NewFood, CustomError),
        schemas(models::dashboard::OrderCollectOut, CustomError),
        schemas(models::dashboard::TodayPointsOut, CustomError),
        schemas(models::orders::OrderDto, CustomError),
        schemas(models::orders::UpdateOrder, CustomError),
        schemas(models::orders::OrderListDto, CustomError),
        schemas(models::orders::OrderOut, CustomError),
        schemas(models::wishes::WishedListOut, CustomError),
        schemas(models::wishes::WishCostDto, CustomError)
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "api文档测试", description = "测试添加openapi"),
        (name = "用户", description = "用户相关接口"),
        (name = "菜品", description = "菜品相关接口")
    ),
    servers(
        (url = "http://localhost:9831", description = "本地服务器")
    )
)]
struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        // Header 认证
        components.add_security_scheme(
            "cookie_auth",
            SecurityScheme::ApiKey(
                ApiKey::Header(
                    ApiKeyValue::new("Authorization")
                )
            )
        );
    }
}