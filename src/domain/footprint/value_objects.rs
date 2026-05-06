// 领域层 - 足迹值对象
// 包含足迹状态枚举

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 足迹记录状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq)]
pub enum RecordStatus {
    /// 草稿
    Draft = 1,
    /// 正式发布
    Official = 0,
}

impl RecordStatus {
    pub fn is_draft(&self) -> bool {
        *self == RecordStatus::Draft
    }

    pub fn from_i16(v: i16) -> Self {
        if v == 1 {
            RecordStatus::Draft
        } else {
            RecordStatus::Official
        }
    }

    pub fn to_i16(&self) -> i16 {
        *self as i16
    }
}
