// API 层 - 所有 API 模块聚合
// 包含 orders, wishes, users, footprint, foods, dashboard, sign_in, notification 等路由模块

pub mod admin;
pub mod couple_space;
pub mod dashboard;
pub mod foods;
pub mod footprint;
pub mod notification;
pub mod orders;
pub mod sign_in;
pub mod upload;
pub mod users;
pub mod wishes;

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
    couple_space::configure(cfg);
    upload::configure(cfg);
    admin::configure(cfg);
}
