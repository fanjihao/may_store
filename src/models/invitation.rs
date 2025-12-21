use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::FromRow;

// ========== 新的邀请/绑定相关模型 (替换旧 Invitation/BindStruct) ==========
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct InvitationRequestOut {
    pub request_id: i64,
    pub requester_id: i64,
    pub requester_username: Option<String>,
    pub requester_avatar: Option<String>,
    pub target_user_id: i64,
    pub status: i16, // 0待处理 1同意 2拒绝 3取消
    pub remark: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub handled_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NewInvitationInput {
    pub target_user_id: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfirmInvitationInput {
    pub accept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvitationListOut {
    pub incoming: Vec<InvitationRequestOut>,
    pub outgoing: Vec<InvitationRequestOut>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UnbindRequestInput {
    pub target_user_id: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GroupMemberOut {
    pub user_id: i64,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub role_in_group: Option<String>, // PAIR 模式下 ORDERING/RECEIVING/ADMIN
    pub is_primary: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GroupInfoOut {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub group_type: String,
    pub invite_code: Option<String>,
    pub status: i16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub members: Vec<GroupMemberOut>,
    pub total_orders: i64,      // 该组总订单数
    pub completed_orders: i64,  // 该组已完成订单数
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BindUserDirectlyInput {
    pub target_user_id: i64,
}