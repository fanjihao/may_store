use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

// ========== 新的邀请/绑定相关模型 (替换旧 Invitation/BindStruct) ==========
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct NewInvitationInput {
    pub target_user_id: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInvitationInput {
    pub accept: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationListOut {
    pub incoming: Vec<InvitationRequestOut>,
    pub outgoing: Vec<InvitationRequestOut>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnbindRequestInput {
    pub target_user_id: i64,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupMemberOut {
    pub user_id: i64,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub role_in_group: Option<String>, // PAIR 模式下 ORDERING/RECEIVING/ADMIN
    pub is_primary: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfoOut {
    pub group_id: i64,
    pub group_name: Option<String>,
    pub group_type: String,
    pub invite_code: Option<String>,
    pub status: i16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub members: Vec<GroupMemberOut>,
    pub total_orders: i64,     // 该组总订单数
    pub completed_orders: i64, // 该组已完成订单数
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BindUserDirectlyInput {
    pub target_user_id: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdateInput {
    pub group_name: String,
}

#[derive(sqlx::FromRow)]
pub struct RequestRow {
    pub request_id: i64,
    pub requester_id: i64,
    pub target_user_id: i64,
    pub status: i16,
}

#[derive(sqlx::FromRow)]
pub struct RoleRow {
    pub user_id: i64,
    pub role: crate::users::models::user::UserRoleEnum,
}

#[derive(sqlx::FromRow)]
pub struct CancelRow {
    pub request_id: i64,
    pub status: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct GroupPointConfig {
    pub group_id: i64,
    pub breeder_closed_points: i32,
    pub confirmed_finished_points: i32,
    pub confirmed_unfinished_points: i32,
    pub timeout_points: i32,
}

impl Default for GroupPointConfig {
    fn default() -> Self {
        Self {
            group_id: 0,
            breeder_closed_points: -8,
            confirmed_finished_points: 10,
            confirmed_unfinished_points: -5,
            timeout_points: -3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupPointConfigUpdateInput {
    pub breeder_closed_points: Option<i32>,
    pub confirmed_finished_points: Option<i32>,
    pub confirmed_unfinished_points: Option<i32>,
    pub timeout_points: Option<i32>,
}
