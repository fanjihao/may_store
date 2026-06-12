// API 层 - 所有 API 模块聚合
// FSD.latest.md compliant - 仅保留 FSD 核心模块

pub mod achievement; // 成就 - FSD v2
pub mod admin;
pub mod auth; // 微信登录 - FSD v2
pub mod dashboard; // 数据看板 - FSD v2
pub mod economy; // 经济查询 - FSD v2
pub mod footprints; // 足迹 - FSD v2
pub mod groups; // 双人组管理 - FSD v2
pub mod kitchens; // 主人家厨房 - FSD v2
pub mod memorial_days; // 纪念日 - FSD §24.9
pub mod tags; // 菜品标签 - FSD §24.4
pub mod footprint_groups; // 足迹分组 - FSD §24.10
pub mod food_marks; // 菜品标记 - FSD §24.6
pub mod support_tickets; // 客服工单 - FSD §15.3
pub mod notifications; // 通知 - FSD v2
pub mod orders;
pub mod swagger;
pub mod sign_in; // 签到 - FSD v2
pub mod upload; // 文件上传 - FSD v2
pub mod users; // 用户基础信息
pub mod wishes;
pub mod ws;

use ntex::web::ServiceConfig;

/// 配置所有 API 路由
pub fn configure(cfg: &mut ServiceConfig) {
    swagger::configure(cfg);
    ws::configure(cfg);
    auth::configure(cfg);
    achievement::configure(cfg);
    dashboard::configure(cfg);
    economy::configure(cfg);
    footprints::configure(cfg);
    notifications::configure(cfg);
    orders::configure(cfg);
    wishes::configure(cfg);
    groups::configure(cfg);
    kitchens::configure(cfg);
    memorial_days::configure(cfg);
    tags::configure(cfg);
    footprint_groups::configure(cfg);
    food_marks::configure(cfg);
    support_tickets::configure(cfg);
    sign_in::configure(cfg);
    upload::configure(cfg);
    users::configure(cfg);
    admin::configure(cfg);
}
