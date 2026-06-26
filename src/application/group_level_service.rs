// 应用服务层 - 组等级计算
//
// 核心: 给一个 (当前 exp, 等级阶梯), 算出当前等级 + 离下一级还差多少
// 不写 association_groups.level —— 那个字段保留为冗余缓存, 不在读路径更新
// (后台写 exp 时可以选择同步 level 字段, 但 get_group 这条读路径永远实时算)

use crate::domain::group::{GroupLevelConfig, GroupLevelProgress};
use sqlx::{PgPool, Row};

pub struct GroupLevelService;

impl GroupLevelService {
    /// 拉所有等级配置 (按 level ASC 排)
    pub async fn list_levels(db: &PgPool) -> Result<Vec<GroupLevelConfig>, sqlx::Error> {
        let rows = sqlx::query("SELECT level, required_exp FROM group_level_configs ORDER BY level ASC")
            .fetch_all(db)
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| GroupLevelConfig {
                level: r.get("level"),
                required_exp: r.get("required_exp"),
            })
            .collect())
    }

    /// 算等级进度
    /// 算法:
    ///   - levels 按 level ASC, 取"最大的、required_exp <= group.exp"的等级
    ///   - 当前等级 = max level s.t. required_exp <= exp
    ///   - 下一级 = 当前 + 1 (如果存在)
    ///   - 若 group.exp 小于最低 required_exp (一般是 Lv 1 = 0), 默认为 Lv 1
    pub async fn compute_progress(
        db: &PgPool,
        group_exp: i64,
    ) -> Result<GroupLevelProgress, sqlx::Error> {
        let levels = Self::list_levels(db).await?;
        Ok(Self::compute_progress_from_levels(&levels, group_exp))
    }

    /// 纯函数版本, 便于测试
    pub fn compute_progress_from_levels(
        levels: &[GroupLevelConfig],
        group_exp: i64,
    ) -> GroupLevelProgress {
        if levels.is_empty() {
            // 没有任何等级配置 —— 兜底默认 Lv 1, 经验 0
            return GroupLevelProgress {
                current_level: 1,
                current_level_required_exp: 0,
                next_level: 1,
                next_level_required_exp: 0,
                exp_in_current_level: group_exp.max(0),
                exp_to_next_level: 0,
                is_max_level: true,
            };
        }

        // 当前等级 = 最大的、required_exp <= group.exp
        let current_level = levels
            .iter()
            .rev()
            .find(|l| l.required_exp <= group_exp)
            .map(|l| l.level)
            .unwrap_or(levels[0].level);

        let current_level_required_exp = levels
            .iter()
            .find(|l| l.level == current_level)
            .map(|l| l.required_exp)
            .unwrap_or(0);

        // 下一级
        let next_level_config = levels.iter().find(|l| l.level == current_level + 1);
        let (next_level, next_level_required_exp, is_max_level) = match next_level_config {
            Some(cfg) => (cfg.level, cfg.required_exp, false),
            None => (current_level, current_level_required_exp, true),
        };

        GroupLevelProgress {
            current_level,
            current_level_required_exp,
            next_level,
            next_level_required_exp,
            exp_in_current_level: (group_exp - current_level_required_exp).max(0),
            exp_to_next_level: (next_level_required_exp - group_exp).max(0),
            is_max_level,
        }
    }
}
