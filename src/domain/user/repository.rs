// 领域层 - 用户仓储 trait
// 定义用户数据的持久化接口，实现依赖倒置原则

use crate::errors::CustomError;
use super::entities::UserRecord;

/// 用户仓储接口（领域层定义，基础设施实现）
/// 定义用户的查询和操作能力，不依赖具体数据库实现
#[allow(dead_code)]
pub trait UserRepository: Send + Sync {
    /// 根据ID查询用户
    async fn find_by_id(&self, user_id: i64) -> Result<Option<UserRecord>, CustomError>;

    /// 根据用户名查询用户
    async fn find_by_username(&self, username: &str) -> Result<Option<UserRecord>, CustomError>;

    /// 根据open_id查询用户（微信登录）
    async fn find_by_open_id(&self, open_id: &str) -> Result<Option<UserRecord>, CustomError>;

    /// 保存用户
    async fn save(&self, user: &UserRecord) -> Result<(), CustomError>;

    /// 更新用户信息
    async fn update(&self, user_id: i64, data: &UserUpdateData) -> Result<(), CustomError>;

    /// 检查用户名是否存在
    async fn exists_by_username(&self, username: &str) -> Result<bool, CustomError>;
}

/// 用户更新数据
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct UserUpdateData {
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}
