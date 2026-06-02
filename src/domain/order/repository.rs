// 领域层 - 订单仓储 trait
// 定义订单数据的持久化接口，实现依赖倒置原则

use crate::errors::CustomError;
use super::entities::{OrderRecord, OrderItemRecord};

/// 订单仓储接口（领域层定义，基础设施实现）
/// 定义订单的查询和操作能力，不依赖具体数据库实现
#[allow(dead_code)]
pub trait OrderRepository: Send + Sync {
    /// 根据ID查询订单
    async fn find_by_id(&self, order_id: i64) -> Result<Option<OrderRecord>, CustomError>;

    /// 根据用户ID查询订单列表
    async fn find_by_user_id(&self, user_id: i64, limit: i64) -> Result<Vec<OrderRecord>, CustomError>;

    /// 根据组ID查询订单列表
    async fn find_by_group_id(&self, group_id: i64, limit: i64) -> Result<Vec<OrderRecord>, CustomError>;

    /// 保存订单
    async fn save(&self, order: &OrderRecord) -> Result<(), CustomError>;

    /// 更新订单状态
    async fn update_status(&self, order_id: i64, status: &str) -> Result<(), CustomError>;

    /// 删除订单（软删除）
    async fn delete(&self, order_id: i64) -> Result<(), CustomError>;
}

/// 订单项仓储接口
#[allow(dead_code)]
pub trait OrderItemRepository: Send + Sync {
    /// 根据订单ID查询订单项
    async fn find_by_order_id(&self, order_id: i64) -> Result<Vec<OrderItemRecord>, CustomError>;

    /// 保存订单项
    async fn save(&self, item: &OrderItemRecord) -> Result<(), CustomError>;

    /// 删除订单项
    async fn delete_by_order_id(&self, order_id: i64) -> Result<(), CustomError>;
}
