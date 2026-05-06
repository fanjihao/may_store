// 应用服务层 - 情侣空间服务

use sqlx::PgPool;
use crate::domain::couple_space::{MemorialDay, MemorialDayCreate, MemorialDayQuery, MemorialDayUpdate};
use crate::errors::CustomError;

pub struct MemorialDayService;

impl MemorialDayService {
    pub async fn list_memorial_days(
        db: &PgPool,
        user_id: i64,
        query: &MemorialDayQuery,
    ) -> Result<Vec<MemorialDay>, CustomError> {
        todo!("迁移获取纪念日列表")
    }

    pub async fn create_memorial_day(
        db: &PgPool,
        user_id: i64,
        input: &MemorialDayCreate,
    ) -> Result<MemorialDay, CustomError> {
        todo!("迁移创建纪念日")
    }

    pub async fn update_memorial_day(
        db: &PgPool,
        user_id: i64,
        id: i64,
        input: &MemorialDayUpdate,
    ) -> Result<MemorialDay, CustomError> {
        todo!("迁移更新纪念日")
    }

    pub async fn delete_memorial_day(
        db: &PgPool,
        user_id: i64,
        id: i64,
    ) -> Result<(), CustomError> {
        todo!("迁移删除纪念日")
    }

    pub async fn get_default_memorial_day(
        db: &PgPool,
        user_id: i64,
    ) -> Result<Option<MemorialDay>, CustomError> {
        todo!("迁移获取默认纪念日")
    }
}
