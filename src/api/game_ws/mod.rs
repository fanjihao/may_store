// API 层 - 游戏 WebSocket 路由
// 提供实时双向通信，支持微信小程序 WebSocket 连接
//
// 注意: ntex 2.x 的 WebSocket API 需要深入理解其服务工厂和分发器模式。
// 当前实现为基础结构，完整的 WebSocket 支持需要进一步适配。

pub mod connection;
pub mod messages;

use std::sync::Arc;
use ntex::web::{self, ServiceConfig};

use crate::api::game_ws::connection::ConnectionManager;

/// WebSocket 全局连接管理器
static CONNECTION_MANAGER: once_cell::sync::OnceCell<Arc<ConnectionManager>> = once_cell::sync::OnceCell::new();

/// 获取全局连接管理器
pub fn get_connection_manager() -> Arc<ConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| Arc::new(ConnectionManager::new())).clone()
}

/// 配置 WebSocket 路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/ws")
            .route("/connect", web::get().to(ws_connect))
            .route("/status", web::get().to(ws_status)),
    );
}

/// WebSocket 连接端点
/// 客户端通过此端点建立 WebSocket 连接
/// 微信小程序中使用 wx.connectSocket() 连接此端点
///
/// 消息格式：
/// - 连接后发送认证消息: {"type": "auth", "data": {"token": "xxx"}}
/// - 心跳: {"type": "ping", "data": {}}
/// - 服务器响应: {"type": "pong", "data": {}} 或 {"type": "auth_resp", "data": {"success": true, "userId": 123}}
///
/// 完整的 WebSocket 实现需要使用 ntex::web::ws::start 并实现 ServiceFactory。
/// 当前返回连接指南信息。
#[utoipa::path(
    get,
    path = "/ws/connect",
    tag = "WebSocket",
    responses(
        (status = 200, description = "WebSocket 连接信息"),
        (status = 400, description = "请求参数错误")
    ),
    security(())
)]
pub async fn ws_connect() -> impl web::Responder {
    let info = serde_json::json!({
        "message": "WebSocket 连接端点",
        "description": "请使用 WebSocket 客户端连接此端点",
        "url": "/ws/connect",
        "protocol": "wss",
        "authMessage": {
            "type": "auth",
            "data": {
                "token": "your_jwt_token_here"
            }
        },
        "pingMessage": {
            "type": "ping",
            "data": {}
        },
        "responseExamples": {
            "connected": {"type": "connected", "data": {}},
            "authSuccess": {"type": "auth_resp", "data": {"success": true, "userId": 123}},
            "authFailed": {"type": "auth_resp", "data": {"success": false, "message": "错误信息"}},
            "pong": {"type": "pong", "data": {}},
            "error": {"type": "error", "data": {"code": 401, "message": "请先认证"}}
        },
        "wechatMiniProgram": "wx.connectSocket({ url: 'wss://your-domain.com/ws/connect' })"
    });

    web::HttpResponse::Ok().json(&info)
}

/// WebSocket 状态端点
/// 获取当前 WebSocket 服务状态
#[utoipa::path(
    get,
    path = "/ws/status",
    tag = "WebSocket",
    responses(
        (status = 200, description = "获取成功")
    ),
    security(())
)]
pub async fn ws_status() -> impl web::Responder {
    let manager = get_connection_manager();
    let online_count = manager.online_count().await;
    let online_users = manager.online_users().await;

    web::HttpResponse::Ok().json(&serde_json::json!({
        "status": "running",
        "onlineCount": online_count,
        "onlineUsers": online_users,
        "serverTime": chrono::Utc::now().to_rfc3339()
    }))
}
