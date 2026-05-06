// API 层 - 上传路由

use ntex::web::{self, HttpResponse};
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;

/// 配置上传路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/upload-token").route(web::get().to(get_qiniu_token)));
}

async fn get_qiniu_token(
    state: web::types::State<Arc<AppState>>,
) -> Result<HttpResponse, CustomError> {
    // TODO: 迁移自 upload/routes.rs
    todo!("迁移获取上传令牌路由")
}
