// WebSocket 连接管理器
// 管理所有活跃的 WebSocket 连接，支持广播和定向消息发送

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

/// WebSocket 连接信息
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// 用户 ID
    pub user_id: Option<i64>,
    /// 连接建立时间戳
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// 是否已认证
    pub authenticated: bool,
    /// 给本连接发消息用的通道(服务端 -> 这一条 ws)
    pub sender: mpsc::UnboundedSender<String>,
}

impl Default for ConnectionInfo {
    fn default() -> Self {
        // 仅占位用,真实连接走 process_messages 里创建
        let (tx, _rx) = mpsc::unbounded_channel();
        Self {
            user_id: None,
            connected_at: chrono::Utc::now(),
            authenticated: false,
            sender: tx,
        }
    }
}

/// WebSocket 连接管理器
/// 使用 user_id -> ConnectionInfo 的映射管理连接
#[allow(dead_code)]
pub struct ConnectionManager {
    /// 用户连接映射: user_id -> connection info
    users: Arc<RwLock<HashMap<i64, ConnectionInfo>>>,
    /// 广播通道，用于系统级消息推送
    broadcast_tx: broadcast::Sender<String>,
}

#[allow(dead_code)]
impl ConnectionManager {
    /// 创建新的连接管理器
    #[allow(dead_code)]
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
        }
    }

    /// 添加新连接
    pub async fn add_connection(&self, user_id: i64, info: ConnectionInfo) {
        let mut users = self.users.write().await;
        users.insert(user_id, info);
        log::info!("WebSocket 连接已添加: user_id={}", user_id);
    }

    /// 移除连接
    pub async fn remove_connection(&self, user_id: i64) {
        let mut users = self.users.write().await;
        users.remove(&user_id);
        log::info!("WebSocket 连接已移除: user_id={}", user_id);
    }

    /// 更新用户认证状态
    pub async fn update_auth(&self, user_id: i64, authenticated: bool) {
        let mut users = self.users.write().await;
        if let Some(info) = users.get_mut(&user_id) {
            info.authenticated = authenticated;
        }
    }

    /// 检查用户是否已连接
    pub async fn is_connected(&self, user_id: i64) -> bool {
        let users = self.users.read().await;
        users.contains_key(&user_id)
    }

    /// 获取在线用户数
    pub async fn online_count(&self) -> usize {
        let users = self.users.read().await;
        users.len()
    }

    /// 获取所有在线用户 ID
    pub async fn online_users(&self) -> Vec<i64> {
        let users = self.users.read().await;
        users.keys().copied().collect()
    }

    /// 广播消息给所有已认证用户
    pub async fn broadcast(&self, message: &str) {
        let _ = self.broadcast_tx.send(message.to_string());
    }

    /// 给指定用户发一条消息
    ///
    /// 返回 true 表示送达(用户在线且发送成功),false 表示用户不在线或通道已关闭
    /// 当前实现是"在线才发、离线不存",因为业务侧说暂时不管离线
    pub async fn send_to_user(&self, user_id: i64, message: &str) -> bool {
        let users = self.users.read().await;
        match users.get(&user_id) {
            Some(info) => info.sender.send(message.to_string()).is_ok(),
            None => false,
        }
    }

    /// 订阅广播消息
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcast_tx.subscribe()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
