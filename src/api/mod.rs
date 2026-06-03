// API 层 - 所有 API 模块聚合
// FSD.latest.md compliant - 仅保留 FSD 核心模块

pub mod admin;
pub mod auth; // 微信登录 - FSD v2
pub mod economy; // 经济查询 - FSD v2
pub mod footprints; // 足迹 - FSD v2
pub mod groups; // 双人组管理 - FSD v2
pub mod kitchens; // 主人家厨房 - FSD v2
pub mod notifications; // 通知 - FSD v2
pub mod orders;
pub mod swagger;
pub mod sign_in; // 签到 - FSD v2
pub mod users; // 用户基础信息
pub mod wishes;
pub mod ws;

use ntex::web::ServiceConfig;

/// 配置所有 API 路由
pub fn configure(cfg: &mut ServiceConfig) {
    swagger::configure(cfg);
    ws::configure(cfg);
    auth::configure(cfg);
    economy::configure(cfg);
    footprints::configure(cfg);
    notifications::configure(cfg);
    orders::configure(cfg);
    wishes::configure(cfg);
    groups::configure(cfg);
    kitchens::configure(cfg);
    sign_in::configure(cfg);
    users::configure(cfg);
    admin::configure(cfg);
}
