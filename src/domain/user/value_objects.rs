// 领域层 - 用户值对象
// 包含用户角色、性别、登录方式等枚举

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use sqlx::Type;

/// 用户角色枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type)]
#[sqlx(type_name = "user_role_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserRole {
    /// 点单方
    Ordering,
    /// 接单方
    Receiving,
    /// 管理员
    Admin,
}

/// 性别枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type)]
#[sqlx(type_name = "gender_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Gender {
    Male,
    Female,
    Other,
    Unknown,
}

/// 登录方式枚举
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type)]
#[sqlx(type_name = "login_method_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LoginMethod {
    Password,
    #[serde(rename = "PHONE_CODE")]
    PhoneCode,
    OAuth,
    Mixed,
    WeiXin,
}
