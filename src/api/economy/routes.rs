// API - 经济查询路由
// FSD.latest.md compliant - 积分/钻石/经验流水查询

use ntex::web::{self, HttpResponse, ServiceConfig, types::Path};
use crate::errors::CustomError;

/// 配置经济查询路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/groups/{group_id}")
            .route("/points", web::get().to(get_points))
            .route("/transactions", web::get().to(get_transactions))
            .route("/exp", web::get().to(get_group_exp))
    );
}

/// 获取用户组内积分
/// GET /api/groups/{group_id}/points?user_id=xxx
///
/// 返回:
/// - available_love_point: 可用爱心积分
/// - frozen_love_point: 冻结爱心积分
async fn get_points(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("get_points not implemented".to_string()))
}

/// 获取积分流水
/// GET /api/groups/{group_id}/transactions?user_id=xxx&type=EARN
async fn get_transactions(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("get_transactions not implemented".to_string()))
}

/// 获取组经验信息
/// GET /api/groups/{group_id}/exp
///
/// 返回:
/// - level: 组等级
/// - exp: 当前经验
/// - exp_to_next_level: 到下一级还需经验
async fn get_group_exp(
    group_id: Path<i64>,
) -> Result<HttpResponse, CustomError> {
    let _ = group_id.into_inner();
    Err(CustomError::internal("get_group_exp not implemented".to_string()))
}