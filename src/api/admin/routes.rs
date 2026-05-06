// API 层 - 后台管理路由

use ntex::web::{self, HttpResponse};
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;

/// 配置后台管理路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // TODO: 添加后台管理路由
}
