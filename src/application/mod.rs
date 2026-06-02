// 应用服务层 - 所有应用服务聚合
// 包含业务用例编排、事务管理、事件处理

pub mod order_service;     // 订单服务
pub mod wish_service;       // 心愿服务
pub mod footprint_service; // 足迹服务
pub mod sign_in_service;   // 签到服务
pub mod notification_service; // 通知服务
pub mod user_service;      // 用户服务
pub mod food_service;      // 菜品服务
pub mod dashboard_service; // 看板服务
pub mod couple_space_service; // 情侣空间服务
pub mod economy_service;   // 经济系统服务 - FSD v2
pub mod event_handlers;    // 事件处理器
