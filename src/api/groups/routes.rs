// API - 双人组管理路由
// FSD.latest.md compliant endpoints

use ntex::web::{self, HttpResponse, ServiceConfig, types::Path};
use crate::errors::CustomError;
use crate::domain::group::{GroupDetailInfo, FulfillmentStats, SettlementCheckResult};

/// 配置双人组路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups")
            .route("", web::post().to(create_group))
            .route("/{group_id}", web::get().to(get_group))
            .route("/{group_id}/invite", web::post().to(create_invite))
            .route("/{group_id}/swap-role", web::post().to(swap_role))
            .route("/{group_id}/settlement-check", web::get().to(settlement_check))
            .route("/{group_id}/fulfillment-stats", web::get().to(fulfillment_stats))
    );
}

/// 创建双人组
/// POST /api/groups
async fn create_group(
) -> Result<HttpResponse, CustomError> {
    // TODO: 实现创建组逻辑
    Err(CustomError::internal("create_group not implemented".to_string()))
}

/// 获取组信息
/// GET /api/groups/{group_id}
async fn get_group(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    // TODO: 实现获取组信息逻辑
    let _ = group_id.into_inner();
    Err(CustomError::internal("get_group not implemented".to_string()))
}

/// 创建邀请
/// POST /api/groups/{group_id}/invite
async fn create_invite(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("not implemented".to_string()))
}

/// 角色互换
/// POST /api/groups/{group_id}/swap-role
///
/// 前置条件:
/// - 小组无未完结在途订单
/// - 操作人无CLAIMED状态且自己作为发起人或履约人的在途心愿
/// - 互换后当前Buyer与Seller对调
///
/// 配置开关:
/// - swap_ignore_ongoing_wish = true时允许带在途心愿互换身份
async fn swap_role(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("swap_role not implemented".to_string()))
}

/// 退出组前结清检查
/// GET /api/groups/{group_id}/settlement-check
///
/// 检查:
/// - 无自己发起且未完结的心愿
/// - 无自己作为履约人且未完结的心愿
/// - 无本组冻结爱心积分
/// - 无待处理的逾期补偿或管理员钻石奖励
async fn settlement_check(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("settlement_check not implemented".to_string()))
}

/// 查看组内双方履约统计
/// GET /api/groups/{group_id}/fulfillment-stats
///
/// 返回:
/// - fulfillment_total: 作为履约人的总心愿数
/// - fulfillment_finished: 按期完成数量
/// - fulfillment_expired: 逾期数量
/// - fulfillment_rate: 按期完成率
/// - avg_fulfillment_hours: 平均履约时长
/// - pending_fulfillment_count: 当前待履约数量
async fn fulfillment_stats(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("fulfillment_stats not implemented".to_string()))
}