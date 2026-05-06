// 应用服务层 - 签到服务
// 包含签到、连续签到奖励等业务用例

use std::sync::Arc;
use sqlx::PgPool;
use crate::config::AppState;
use crate::domain::sign_in::{SignInfoResponse, SignInResponse, DailyCheckinOut};
use crate::errors::CustomError;

/// 签到应用服务
pub struct SignService;

impl SignService {
    /// 用户签到
    pub async fn sign_in(
        user_id: i64,
        state: &Arc<AppState>,
    ) -> Result<SignInResponse, CustomError> {
        // TODO: 迁移自 users/service/sign.rs
        todo!("迁移签到逻辑")
    }

    /// 获取签到信息
    pub async fn get_sign_info(
        user_id: i64,
        state: &Arc<AppState>,
    ) -> Result<SignInfoResponse, CustomError> {
        // TODO: 迁移自 users/routes/sign.rs::get_sign_info
        todo!("迁移签到信息逻辑")
    }

    /// 每日签到
    pub async fn daily_checkin(
        token: crate::middlewares::auth::UserToken,
        state: &Arc<AppState>,
    ) -> Result<DailyCheckinOut, CustomError> {
        // TODO: 迁移自 users/routes/sign.rs::daily_checkin
        todo!("迁移每日签到逻辑")
    }
}