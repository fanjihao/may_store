// API 层 - 游戏 WebSocket 路由
// 使用 tokio-tungstenite 实现 WebSocket，支持微信小程序连接

pub mod connection;
pub mod messages;

use futures_util::{SinkExt, StreamExt};
use ntex::web::{self, ServiceConfig};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;

use crate::api::ws::connection::{ConnectionInfo, ConnectionManager};
use crate::config::AppState;
use crate::middlewares::auth::ensure_active_account;
use crate::middlewares::jwt;
use crate::utils::response::ApiResponse;

/// WebSocket 全局连接管理器
static CONNECTION_MANAGER: once_cell::sync::OnceCell<Arc<ConnectionManager>> =
    once_cell::sync::OnceCell::new();

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WsStatusPublic {
    status: &'static str,
    online_count: usize,
    server_time: String,
}

/// 获取全局连接管理器
pub fn get_connection_manager() -> Arc<ConnectionManager> {
    CONNECTION_MANAGER
        .get_or_init(|| Arc::new(ConnectionManager::new()))
        .clone()
}

/// 配置 WebSocket 路由（HTTP 端点）
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/ws")
            .route("/info", web::get().to(ws_info))
            .route("/status", web::get().to(ws_status)),
    );
}

/// WebSocket 信息端点
#[utoipa::path(
    get,
    path = "/ws/info",
    tag = "WebSocket",
    responses(
        (status = 200, description = "获取成功")
    ),
    security(())
)]
pub async fn ws_info() -> impl web::Responder {
    let info = serde_json::json!({
        "message": "WebSocket 连接信息",
        "description": "请连接到 WebSocket 服务器",
        "host": "127.0.0.1",
        "port": 9832,
        "url": "ws://127.0.0.1:9832",
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
        "wechatMiniProgram": "wx.connectSocket({ url: 'ws://127.0.0.1:9832' })"
    });

    ApiResponse::success(info)
}

/// WebSocket 状态端点
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

    ApiResponse::success(WsStatusPublic {
        status: "running",
        online_count,
        server_time: chrono::Utc::now().to_rfc3339(),
    })
}

/// 启动 WebSocket 服务器（在独立端口）
///
/// 接收 `Arc<AppState>` 而非裸 jwt_secret —— 这样 `process_messages` 才能
/// 调用统一的 `jwt::verify`，并直接查询 DB 校验账号仍为 ACTIVE。
pub async fn start_websocket_server(
    addr: &str,
    app_state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    let manager = get_connection_manager();
    let listener = TcpListener::bind(addr).await?;
    log::info!("WebSocket 服务器已启动: {}", addr);

    while let Ok((tcp_stream, peer_addr)) = listener.accept().await {
        let manager = manager.clone();
        let state = app_state.clone();
        tokio::spawn(async move {
            handle_websocket_connection(tcp_stream, manager, peer_addr, state).await;
        });
    }

    Ok(())
}

/// 处理 WebSocket 连接
async fn handle_websocket_connection(
    tcp_stream: tokio::net::TcpStream,
    manager: Arc<ConnectionManager>,
    peer_addr: std::net::SocketAddr,
    state: Arc<AppState>,
) {
    match accept_async(tcp_stream).await {
        Ok(ws_stream) => {
            log::info!("新的 WebSocket 连接: {}", peer_addr);
            process_messages(ws_stream, manager, peer_addr, state).await;
        }
        Err(e) => {
            log::error!("WebSocket 握手失败 from {}: {}", peer_addr, e);
        }
    }
}

/// 处理 WebSocket 消息循环
///
/// 用 tokio::select! 多路复用:
/// - ws_rx: 客户端发来的消息
/// - internal_rx: 服务端通过 ConnectionManager::send_to_user 主动推送的消息
async fn process_messages(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    manager: Arc<ConnectionManager>,
    peer_addr: std::net::SocketAddr,
    state: Arc<AppState>,
) {
    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    let mut authenticated_connection: Option<(i64, Uuid)> = None;

    // 每条连接的内部消息通道。auth 成功时把 sender 交给 ConnectionManager,
    // 之后服务端就能通过 send_to_user(uid, ...) 把消息投到这条通道
    let (internal_tx, mut internal_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // 发送连接成功消息
    let _ = ws_tx
        .send(Message::Text(r#"{"type":"connected","data":{}}"#.into()))
        .await;

    loop {
        tokio::select! {
            // 客户端 -> 服务端
            ws_msg = ws_rx.next() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        let text = text.to_string();
                        log::debug!("收到消息 from {}: {}", peer_addr, text);

                        if let Ok(envelope) =
                            serde_json::from_str::<crate::api::ws::messages::WsEnvelope>(&text)
                        {
                            match envelope.msg_type.as_str() {
                                "ping" => {
                                    let _ = ws_tx
                                        .send(Message::Text(r#"{"type":"pong","data":{}}"#.into()))
                                        .await;
                                }
                                "auth" => {
                                    if let Some(token) = envelope.data.get("token").and_then(|t| t.as_str())
                                    {
                                        // JWT 撤销校验后还要直接查 DB；账号非 ACTIVE 时绝不登记连接。
                                        let result: Result<i64, crate::errors::CustomError> = async {
                                            let claims = jwt::verify(
                                                token,
                                                &state.jwt_secret,
                                                jwt::TokenType::Access,
                                                &state.redis_cache,
                                            )
                                            .await?;
                                            let uid = claims.user_id()?;
                                            ensure_active_account(&state, uid).await?;
                                            Ok(uid)
                                        }
                                        .await;

                                        match result {
                                            Ok(uid) => {
                                                let connection_id = Uuid::new_v4();
                                                manager
                                                    .add_connection(
                                                        uid,
                                                        ConnectionInfo {
                                                            connection_id,
                                                            user_id: Some(uid),
                                                            connected_at: chrono::Utc::now(),
                                                            authenticated: true,
                                                            sender: internal_tx.clone(),
                                                        },
                                                    )
                                                    .await;
                                                if let Some((previous_user_id, previous_connection_id)) =
                                                    authenticated_connection
                                                        .replace((uid, connection_id))
                                                {
                                                    manager
                                                        .remove_connection(
                                                            previous_user_id,
                                                            previous_connection_id,
                                                        )
                                                        .await;
                                                }
                                                let _ = ws_tx.send(Message::Text(format!(
                                                    r#"{{"type":"auth_resp","data":{{"success":true,"userId":{}}}}}"#,
                                                    uid
                                                ).into())).await;
                                                log::info!("用户 {} 认证成功 from {}", uid, peer_addr);
                                            }
                                            Err(e) => {
                                                log::warn!(
                                                    "WebSocket 用户认证失败 from {}: {}",
                                                    peer_addr,
                                                    e
                                                );
                                                let _ = ws_tx.send(Message::Text(
                                                    r#"{"type":"auth_resp","data":{"success":false,"message":"认证失败，请重新登录"}}"#
                                                        .into(),
                                                )).await;
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    if authenticated_connection.is_none() {
                                        let _ = ws_tx.send(Message::Text(r#"{"type":"error","data":{"code":401,"message":"请先认证"}}"#.into())).await;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        log::info!("WebSocket 连接关闭: {}", peer_addr);
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_tx.send(Message::Pong(data)).await;
                    }
                    Some(Err(e)) => {
                        log::error!("WebSocket 错误 from {}: {}", peer_addr, e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            // 服务端 -> 客户端(主动推送)
            internal_msg = internal_rx.recv() => {
                match internal_msg {
                    Some(text) => {
                        if ws_tx.send(Message::Text(text.into())).await.is_err() {
                            log::info!("推送失败,连接可能已关闭: {}", peer_addr);
                            break;
                        }
                    }
                    None => {
                        // 所有 sender 都丢了,理论上不该发生
                        break;
                    }
                }
            }
        }
    }

    // 清理连接
    if let Some((uid, connection_id)) = authenticated_connection {
        if manager.remove_connection(uid, connection_id).await {
            log::info!("用户 {} 连接已清理", uid);
        }
    }
}

// 私有 `verify_token` 已删除 —— WS 与 HTTP 统一走 `crate::middlewares::jwt::verify`

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_status_omits_online_user_ids() {
        let value = serde_json::to_value(WsStatusPublic {
            status: "running",
            online_count: 2,
            server_time: "2026-07-20T00:00:00Z".to_string(),
        })
        .unwrap();
        let object = value.as_object().unwrap();

        assert_eq!(object.len(), 3);
        assert!(object.contains_key("status"));
        assert!(object.contains_key("onlineCount"));
        assert!(object.contains_key("serverTime"));
        assert!(!object.contains_key("onlineUsers"));
    }
}
