// API 层 - 所有 API 模块聚合
// 包含 orders, wishes, users, footprint, foods, dashboard, sign_in, notification 等路由模块

pub mod admin;
pub mod couple_space;
pub mod dashboard;
pub mod foods;
pub mod footprint;
pub mod ws;
pub mod notification;
pub mod orders;
pub mod sign_in;
pub mod swagger;
pub mod upload;
pub mod users;
pub mod wishes;
pub mod groups;     // 双人组管理 - FSD v2
pub mod kitchens;   // 主人家厨房 - FSD v2
pub mod economy;    // 经济查询 - FSD v2

use ntex::web::ServiceConfig;

/// 配置所有 API 路由
pub fn configure(cfg: &mut ServiceConfig) {
    swagger::configure(cfg);
    ws::configure(cfg);
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
    groups::configure(cfg);    // FSD v2
    kitchens::configure(cfg);  // FSD v2
    economy::configure(cfg);   // FSD v2
}
