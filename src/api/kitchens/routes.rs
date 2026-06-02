// API - 主人家厨房路由
// FSD.latest.md compliant - 做客系统

use ntex::web::{self, HttpResponse, ServiceConfig, types::Path};
use crate::errors::CustomError;

/// 配置做客厨房路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/kitchens/invitations")
            .route("/{invite_code}", web::get().to(access_kitchen))
            .route("/{invite_code}/foods", web::get().to(get_kitchen_foods))
            .route("/{invite_code}/orders", web::post().to(create_guest_order))
    );
}

/// 访问主人家厨房
/// GET /api/kitchens/invitations/{invite_code}
///
/// 做客用户必须使用长期账号，通过邀请链接访问主人家厨房
/// 返回厨房信息和邀请有效期
async fn access_kitchen(
    invite_code: Path<String>,
) -> Result<HttpResponse, CustomError> {
    let _ = invite_code.into_inner();
    Err(CustomError::internal("access_kitchen not implemented".to_string()))
}

/// 查看主人家厨房菜单
/// GET /api/kitchens/invitations/{invite_code}/foods
///
/// 做客用户可查看主人家厨房菜单
/// 仅返回授权范围内的菜单
async fn get_kitchen_foods(
    invite_code: Path<String>,
) -> Result<HttpResponse, CustomError> {
    let _ = invite_code.into_inner();
    Err(CustomError::internal("get_kitchen_foods not implemented".to_string()))
}

/// 创建做客订单
/// POST /api/kitchens/invitations/{invite_code}/orders
///
/// 做客用户下单，订单归属主人家小组
/// 由主人家Seller完成
/// 做客用户不获得主人组爱心积分
async fn create_guest_order(
    invite_code: Path<String>,
) -> Result<HttpResponse, CustomError> {
    let _ = invite_code.into_inner();
    Err(CustomError::internal("create_guest_order not implemented".to_string()))
}