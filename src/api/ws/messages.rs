// WebSocket 消息类型定义
// 定义客户端和服务器之间的通信消息格式

use serde::{Deserialize, Serialize};

/// WebSocket 消息类型枚举
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessageType {
    /// 心跳 ping - 客户端发送
    Ping,
    /// 心跳 pong - 服务器响应
    Pong,
    /// 认证消息 - 客户端发送 token
    Auth(WsAuthData),
    /// 认证响应 - 服务器返回认证结果
    AuthResp(WsAuthRespData),
    /// 通知消息 - 服务器推送
    Notification(WsNotificationData),
    /// 订单状态变更 - 服务器推送
    OrderUpdate(WsOrderUpdateData),
    /// 组员变化 - 服务器推送(加入/退出/换角色)
    GroupMemberChange(WsGroupMemberChangeData),
    /// 错误消息
    Error(WsErrorData),
    /// 未知消息
    Unknown,
}

/// 认证数据
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsAuthData {
    pub token: String,
}

/// 认证响应数据
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsAuthRespData {
    pub success: bool,
    pub user_id: Option<i64>,
    pub message: Option<String>,
}

/// 通知数据
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsNotificationData {
    pub id: i64,
    pub title: String,
    pub content: Option<String>,
    pub created_at: String,
}

/// 订单状态变更数据
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsOrderUpdateData {
    pub order_id: i64,
    pub status: String,
    pub message: String,
}

/// 组员摘要信息(actor / buyer / seller 都用这个)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsGroupMemberInfo {
    pub user_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// 组员变化数据
///
/// action 取值:
/// - "joined":  有人通过邀请码加入了组
/// - "exited":  有人退出了组
/// - "swapped": 互换 buyer/seller 角色
///
/// actor: 这次动作的发起人
/// buyer / seller: 变化后(对 join/exit 是操作后,对 swap 是互换后)的角色归属;
///                 swap 时两者都不为空,join/exit 时可能为 null(比如组里没人了)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsGroupMemberChangeData {
    pub group_id: i64,
    pub action: String,
    pub actor: WsGroupMemberInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer: Option<WsGroupMemberInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seller: Option<WsGroupMemberInfo>,
}

/// 错误数据
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsErrorData {
    pub code: i32,
    pub message: String,
}

/// WebSocket 消息外壳
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsEnvelope {
    /// 消息类型
    #[serde(rename = "type")]
    pub msg_type: String,
    /// 消息数据
    pub data: serde_json::Value,
}

#[allow(dead_code)]
impl WsEnvelope {
    /// 创建心跳 pong 消息
    pub fn pong() -> Self {
        Self {
            msg_type: "pong".to_string(),
            data: serde_json::json!({}),
        }
    }

    /// 创建认证响应消息
    pub fn auth_resp(success: bool, user_id: Option<i64>, message: Option<String>) -> Self {
        Self {
            msg_type: "auth_resp".to_string(),
            data: serde_json::json!({
                "success": success,
                "userId": user_id,
                "message": message
            }),
        }
    }

    /// 创建通知消息
    pub fn notification(notification: &WsNotificationData) -> Self {
        Self {
            msg_type: "notification".to_string(),
            data: serde_json::json!(notification),
        }
    }

    /// 创建订单更新消息
    pub fn order_update(order_update: &WsOrderUpdateData) -> Self {
        Self {
            msg_type: "order_update".to_string(),
            data: serde_json::json!(order_update),
        }
    }

    /// 创建组员变化消息
    pub fn group_member_change(change: &WsGroupMemberChangeData) -> Self {
        Self {
            msg_type: "group_member_change".to_string(),
            data: serde_json::json!(change),
        }
    }

    /// 创建错误消息
    pub fn error(code: i32, message: &str) -> Self {
        Self {
            msg_type: "error".to_string(),
            data: serde_json::json!({
                "code": code,
                "message": message
            }),
        }
    }
}
