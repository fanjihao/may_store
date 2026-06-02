// API 层 - 所有 API 模块聚合
// FSD.latest.md compliant - 仅保留 FSD 核心模块

pub mod admin;
pub mod orders;
pub mod swagger;
pub mod ws;
pub mod wishes;
pub mod groups;     // 双人组管理 - FSD v2
pub mod kitchens;   // 主人家厨房 - FSD v2

use ntex::web::ServiceConfig;

/// 配置所有 API 路由
pub fn configure(cfg: &mut ServiceConfig) {
    swagger::configure(cfg);
    ws::configure(cfg);
    orders::configure(cfg);
    wishes::configure(cfg);
    groups::configure(cfg);    // FSD v2
    kitchens::configure(cfg);  // FSD v2
    admin::configure(cfg);
}
