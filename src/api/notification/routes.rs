// API 层 - 通知路由

use ntex::web::{self, HttpResponse};
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;

/// 配置通知路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // TODO: 添加通知相关路由
}
