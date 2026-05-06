// 领域层 - 经济系统值对象
// 包含积分和钻石的值对象定义

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 积分值对象
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
pub struct Point(pub i32);

impl Point {
    pub fn new(v: i32) -> Self {
        Point(v)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn add(&self, other: Point) -> Point {
        Point(self.0 + other.0)
    }

    pub fn subtract(&self, other: Point) -> Point {
        Point(self.0 - other.0)
    }
}

/// 钻石值对象
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
pub struct Diamond(pub i32);

impl Diamond {
    pub fn new(v: i32) -> Self {
        Diamond(v)
    }

    pub fn value(&self) -> i32 {
        self.0
    }

    pub fn add(&self, other: Diamond) -> Diamond {
        Diamond(self.0 + other.0)
    }

    pub fn subtract(&self, other: Diamond) -> Diamond {
        Diamond(self.0 - other.0)
    }
}
