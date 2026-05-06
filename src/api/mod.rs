// API 层 - 所有 API 模块聚合
// 包含 orders, wishes, users, footprint, foods, dashboard, sign_in, notification 等路由模块

pub mod orders;
pub mod wishes;
pub mod users;
pub mod footprint;
pub mod foods;
pub mod dashboard;
pub mod sign_in;
pub mod notification;
pub mod wx;
pub mod couple_space;
pub mod upload;
pub mod admin;

use ntex::web::ServiceConfig;

/// 配置所有 API 路由
pub fn configure(cfg: &mut ServiceConfig) {
    orders::configure(cfg);
    wishes::configure(cfg);
    users::configure(cfg);
    footprint::configure(cfg);
    foods::configure(cfg);
    dashboard::configure(cfg);
    sign_in::configure(cfg);
    notification::configure(cfg);
    wx::configure(cfg);
    couple_space::configure(cfg);
    upload::configure(cfg);
    admin::configure(cfg);
}