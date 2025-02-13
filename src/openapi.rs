
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

#[derive(OpenApi)]
#[openapi(
    paths(
        wx_login,
        wx_register,
        wx_change_info,
        get_user_info,
        is_register,
        get_invitation,
        new_invitation
    ),
    components(
        schemas(models::users::Login, CustomError),
        schemas(models::users::UserInfo, CustomError),
        schemas(models::users::IsRegister, CustomError),
        schemas(models::invitation::Invitation, CustomError)
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "api文档测试", description = "测试添加openapi")
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