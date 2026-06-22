// 领域层 - 用户实体
// 包含用户、用户Token等数据库记录和 DTO

use chrono::{NaiveDate, DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use super::{Gender, LoginMethod, UserRole};

/// 用户记录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserRecord {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub gender: Gender,
    pub birthday: Option<NaiveDate>,
    pub role: UserRole,
    pub love_point: i32,
    pub diamond: i32,
    pub group_id: Option<i64>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[sqlx(default)]
    pub password_hash: Option<String>,
    #[sqlx(default)]
    pub password_algo: Option<String>,
    #[sqlx(default)]
    pub open_id: Option<String>,
    #[sqlx(default)]
    pub username_change: Option<bool>,
    #[sqlx(default)]
    pub login_method: Option<LoginMethod>,
    #[sqlx(default)]
    pub password_updated_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub is_temp_password: Option<bool>,
    #[sqlx(default)]
    pub push_id: Option<String>,
    #[sqlx(default)]
    pub last_role_switch_at: Option<DateTime<Utc>>,
}

impl From<UserRecord> for UserPublic {
    fn from(r: UserRecord) -> Self {
        Self {
            user_id: r.user_id,
            username: r.username.clone(),
            // 兜底: 老用户(没有设过昵称)的 nick_name 是 NULL,这里用 username 顶
            // 新用户走 create_wechat_user 时 nick_name 已经设成 username,不受影响
            nick_name: r.nick_name.or(Some(r.username)),
            avatar: r.avatar,
            role: r.role,
            group_id: r.group_id,
        }
    }
}

/// 登录输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub username: String,
    pub password: Option<String>,
    pub phone_code: Option<String>,
    pub weixin_code: Option<String>,
    pub login_method: LoginMethod,
}

/// 注册输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar: Option<String>,
    pub open_id: Option<String>,
    pub gender: Option<Gender>,
    pub birthday: Option<NaiveDate>,
}

/// 用户信息公开（不含敏感信息）
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserPublic {
    pub user_id: i64,
    pub username: String,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub role: UserRole,
    pub group_id: Option<i64>,
}

/// 用户Token（用于认证）
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserToken {
    pub user_id: i64,
    pub username: String,
    pub role: UserRole,
    pub group_id: Option<i64>,
    pub exp: usize,
}

/// 用户资料更新输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpdateInput {
    pub username: String,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub phone: Option<String>,
    pub gender: Option<Gender>,
    pub birthday: Option<NaiveDate>,
    pub new_password: Option<String>,
    pub old_password: Option<String>,
    pub new_username: Option<String>,
}

/// 甜言蜜语记录
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SweetTalkRecord {
    pub id: i64,
    pub from_user_id: i64,
    pub to_user_id: i64,
    pub content: String,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

/// 查询用户名是否注册
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct IsRegisterQuery {
    pub username: String,
}

/// 是否注册响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IsRegisterResponse {
    pub is_register: bool,
}

/// 登录响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserPublic,
}

/// 用户信息响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserInfoResponse {
    pub is_register: bool,
    pub user: Option<UserPublic>,
}

/// 角色切换输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSwitchInput {
    pub role: UserRole,
}

/// 角色切换结果
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoleSwitchResult {
    pub success: bool,
    pub new_role: UserRole,
    pub remaining_switches: i32,
}

/// 邀请列表输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationListOut {
    pub incoming: Vec<InvitationRequestOut>,
    pub outgoing: Vec<InvitationRequestOut>,
}

/// 邀请请求输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvitationRequestOut {
    pub id: i64,
    pub from_user_id: i64,
    pub from_username: String,
    pub from_nick_name: Option<String>,
    pub from_avatar: Option<String>,
    pub to_user_id: i64,
    pub to_username: String,
    pub to_nick_name: Option<String>,
    pub to_avatar: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<Utc>,
}

/// 新邀请输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewInvitationInput {
    pub to_username: String,
}

/// 确认邀请输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmInvitationInput {
    pub accept: bool,
}

/// 解绑请求输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnbindRequestInput {
    pub reason: Option<String>,
}

/// 群组信息输出
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfoOut {
    pub group_id: i64,
    pub group_name: String,
    pub member_count: i32,
    pub members: Vec<UserPublic>,
}

/// 群组更新输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupUpdateInput {
    pub group_name: Option<String>,
}

/// 直接绑定用户输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BindUserDirectlyInput {
    pub user_id: i64,
}

/// 群组积分配置
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupPointConfig {
    pub group_id: i64,
    pub order_point_percent: i32,
}

/// 群组积分配置更新输入
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupPointConfigUpdateInput {
    pub order_point_percent: Option<i32>,
}
