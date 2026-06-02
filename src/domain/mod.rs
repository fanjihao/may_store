// 领域层 - 所有领域模块聚合
// 包含 user, order, economy, footprint, wish, achievement, sign_in, event, foods, dashboard, couple_space, group

pub mod user;      // 用户领域
pub mod order;     // 订单领域
pub mod economy;   // 经济系统领域
pub mod footprint; // 足迹领域
pub mod wish;      // 心愿领域
pub mod achievement; // 成就领域
pub mod sign_in;   // 签到领域
pub mod event;     // 事件领域
pub mod foods;     // 菜品领域
pub mod dashboard; // 看板领域
pub mod couple_space; // 情侣空间领域
pub mod group;     // 双人组领域 - FSD v2
