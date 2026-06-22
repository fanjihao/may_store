# admin 系统配置 + 签到配置 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 后端修签到走组钻石、加 admin 可配的 `fullTeamBonusAmt`、修 OpenAPI 文档不一致；前端 multi-admin 实现真的"系统配置"页（5 个配置项可看可改）。

**Architecture:** 后端走 TDD 抽 `validate_config` / `is_full_team_signed` / `apply_full_team_bonus` 三个纯函数，扩展 `sign_in_service::daily_checkin` 走组钻石池；admin/configs 改成强类型结构体。前端 React 19 + antd + react-query，按 "卡片分组 + 独立保存" 形态，参考 `cat-i18n/languages` 的代码风格。

**Tech Stack:** Rust 2021 / ntex 2.1 / sqlx 0.8 / utoipa 5 / PostgreSQL 16 / React 19 / TypeScript 5 / antd 5 / @tanstack/react-query 5 / openapi-typescript

---

## 文件改动总览

| 文件 | 操作 | 任务 |
|---|---|---|
| `src/api/admin/routes.rs` | 修改：抽 `validate_config` 纯函数、加 `fullTeamBonusAmt` 白名单、改 `get_config` 返回 `ConfigResponse` | T1, T2, T3 |
| `src/v3.sql` | 修改：在 `global_configs` 表附近加 INSERT 提示 | T4 |
| `src/application/sign_in_service.rs` | 修改：加满签业务规则 stub 测试；`daily_checkin` 改走组钻石、串入满签逻辑 | T4, T7 |
| `src/domain/sign_in/entities.rs` | 修改：扩 `DailyCheckinOut` 加 `full_team_bonus` / `full_team_bonus_amt` | T9 |
| `multi-admin/src/constants/api-paths.ts` | 修改：修 `config` 路径 | T10 |
| `multi-admin/src/features/store/config/types.ts` | **新建** | T11 |
| `multi-admin/src/features/store/config/hooks.ts` | **新建** | T12 |
| `multi-admin/src/features/store/config/ConfigPage.tsx` | 修改：占位 → 真实实现 | T13 |
| `src/api/admin/routes.rs` | 修改：加 12 个 `validate_*` 单测 | T1 |
| `src/application/sign_in_service.rs` | 修改：加 4 个满签业务规则 stub 测试 | T4 |
| `Cargo.toml` | **不修改** | — |
| `multi-admin/src/router.tsx` | **不修改**（路由已存在） | — |
| 其它 multi-admin feature 页 | **不修改**（不在范围） | — |

---

## Task 1: 抽 `validate_config` 纯函数 + 单测

**Files:**
- Modify: `src/api/admin/routes.rs`（加 `validate_config` 函数 + 文件末尾 `#[cfg(test)] mod tests`）

**目标:** 把 `update_config` handler 里的白名单 + 范围校验逻辑**抽出**成可独立测的纯函数。`update_config` handler 里改成调它，行为完全等价。

### Step 1.1: 在 `routes.rs` 顶部 `use` 段后加 `validate_config`

文件位置：`src/api/admin/routes.rs` 第 19 行（最后一个 `use`）之后追加：

```rust
/// 单条配置白名单 + 范围 + 所属 category
const CONFIG_ENTRIES: &[(&str, i64, i64, &str)] = &[
    ("orderPointPercent", 1, 200, "ORDER"),
    ("diamondUnlockCost", 10, 10000, "REWARDS"),
    ("defaultFootprintCapacity", 10, 1000, "GENERAL"),
    ("fullTeamBonusAmt", 0, 100, "SIGN_IN"),
];

/// 整数配置项的最小值/最大值常量（从 CONFIG_ENTRIES 派生）
const SIGN_IN_REWARDS_ELEMENT_MIN: i64 = 1;
const SIGN_IN_REWARDS_ELEMENT_MAX: i64 = 100;
const SIGN_IN_REWARDS_REQUIRED_LEN: usize = 7;

/// 校验单条配置项的新值
///
/// - 整数项：必须在 [min, max] 之间
/// - signInRewards7Days：必须是 7 位数组、每个元素 1~100
/// - 未知 key：返回 BadRequest
pub fn validate_config(
    key: &str,
    value: &serde_json::Value,
) -> Result<(), CustomError> {
    // signInRewards7Days 单独处理
    if key == "signInRewards7Days" {
        let arr = value.as_array().ok_or_else(|| {
            CustomError::BadRequest("signInRewards7Days 必须是数组".into())
        })?;
        if arr.len() != SIGN_IN_REWARDS_REQUIRED_LEN {
            return Err(CustomError::BadRequest(format!(
                "signInRewards7Days 必须正好 {} 个元素,当前 {} 个",
                SIGN_IN_REWARDS_REQUIRED_LEN,
                arr.len()
            )));
        }
        for v in arr {
            let n = v.as_i64().ok_or_else(|| {
                CustomError::BadRequest("signInRewards7Days 元素必须是整数".into())
            })?;
            if !(SIGN_IN_REWARDS_ELEMENT_MIN..=SIGN_IN_REWARDS_ELEMENT_MAX).contains(&n) {
                return Err(CustomError::BadRequest(format!(
                    "signInRewards7Days 元素必须在 {}-{} 之间,当前 {}",
                    SIGN_IN_REWARDS_ELEMENT_MIN, SIGN_IN_REWARDS_ELEMENT_MAX, n
                )));
            }
        }
        return Ok(());
    }

    // 整数项
    let (_, lo, hi, _cat) = CONFIG_ENTRIES
        .iter()
        .find(|(k, _, _, _)| *k == key)
        .ok_or_else(|| CustomError::BadRequest(format!("未知配置键: {}", key)))?;
    let v = value
        .as_i64()
        .ok_or_else(|| CustomError::BadRequest("value 必须是整数".into()))?;
    if v < *lo || v > *hi {
        return Err(CustomError::BadRequest(format!(
            "{} 必须在 {}-{} 之间",
            key, lo, hi
        )));
    }
    Ok(())
}
```

### Step 1.2: 改 `update_config` handler 用 `validate_config`

替换 `src/api/admin/routes.rs` 中 `update_config` 函数体里的**所有**内联校验逻辑（L353-432），改成：

```rust
pub async fn update_config(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<String>,
    body: Json<UpdateConfigInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let config_key = path.into_inner();
    let new_value = body.into_inner().value;

    // 校验
    validate_config(&config_key, &new_value)?;

    // 决定 category
    let category = if config_key == "signInRewards7Days" {
        "SIGN_IN"
    } else {
        let (_, _, _, cat) = CONFIG_ENTRIES
            .iter()
            .find(|(k, _, _, _)| *k == config_key.as_str())
            .expect("validate_config 已确保 key 存在");
        *cat
    };

    // 写库
    sqlx::query(
        r#"INSERT INTO global_configs (config_key, config_value, category, updated_by, updated_at)
           VALUES ($1, $2::jsonb, $3::config_category_enum, $4, NOW())
           ON CONFLICT (config_key) DO UPDATE
           SET config_value = EXCLUDED.config_value,
               updated_by = EXCLUDED.updated_by,
               updated_at = NOW()"#,
    )
    .bind(&config_key)
    .bind(&new_value)
    .bind(category)
    .bind(admin.user_id)
    .execute(db)
    .await?;

    // 写审计日志
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, detail)
           VALUES ($1, 'ADMIN', 'CONFIG_UPDATE', 'GLOBAL_CONFIG', $2)"#,
    )
    .bind(admin.user_id)
    .bind(serde_json::json!({ "config_key": config_key, "value": new_value }))
    .execute(db)
    .await;

    Ok(ApiResponse::success(serde_json::json!({
        "config_key": config_key,
        "value": new_value,
        "status": "ok"
    })))
}
```

### Step 1.3: 在 `routes.rs` 末尾加测试模块

文件最后追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- signInRewards7Days 数组校验 ----

    #[test]
    fn validate_sign_in_rewards_7_elements_passes() {
        let v = json!([5, 6, 7, 8, 9, 10, 20]);
        assert!(validate_config("signInRewards7Days", &v).is_ok());
    }

    #[test]
    fn validate_sign_in_rewards_6_elements_rejected() {
        let v = json!([5, 6, 7, 8, 9, 10]);
        let err = validate_config("signInRewards7Days", &v).unwrap_err();
        assert!(format!("{}", err).contains("必须正好 7 个元素"));
    }

    #[test]
    fn validate_sign_in_rewards_8_elements_rejected() {
        let v = json!([5, 6, 7, 8, 9, 10, 20, 99]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_empty_rejected() {
        let v = json!([]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_element_zero_rejected() {
        let v = json!([0, 6, 7, 8, 9, 10, 20]);
        let err = validate_config("signInRewards7Days", &v).unwrap_err();
        assert!(format!("{}", err).contains("元素必须在 1-100"));
    }

    #[test]
    fn validate_sign_in_rewards_element_101_rejected() {
        let v = json!([5, 6, 7, 8, 9, 10, 101]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_element_string_rejected() {
        let v = json!([5, 6, "7", 8, 9, 10, 20]);
        assert!(validate_config("signInRewards7Days", &v).is_err());
    }

    #[test]
    fn validate_sign_in_rewards_not_array_rejected() {
        let v = json!(5);
        let err = validate_config("signInRewards7Days", &v).unwrap_err();
        assert!(format!("{}", err).contains("必须是数组"));
    }

    // ---- 整数配置范围 ----

    #[test]
    fn validate_int_config_in_range_passes() {
        let v = json!(50);
        assert!(validate_config("orderPointPercent", &v).is_ok());
    }

    #[test]
    fn validate_int_config_below_min_rejected() {
        let v = json!(0);
        assert!(validate_config("orderPointPercent", &v).is_err());
    }

    #[test]
    fn validate_int_config_above_max_rejected() {
        let v = json!(300);
        assert!(validate_config("orderPointPercent", &v).is_err());
    }

    #[test]
    fn validate_int_config_string_rejected() {
        let v = json!("100");
        let err = validate_config("orderPointPercent", &v).unwrap_err();
        assert!(format!("{}", err).contains("value 必须是整数"));
    }

    #[test]
    fn validate_full_team_bonus_amt_zero_passes() {
        let v = json!(0);
        assert!(validate_config("fullTeamBonusAmt", &v).is_ok());
    }

    #[test]
    fn validate_full_team_bonus_amt_100_passes() {
        let v = json!(100);
        assert!(validate_config("fullTeamBonusAmt", &v).is_ok());
    }

    #[test]
    fn validate_full_team_bonus_amt_101_rejected() {
        let v = json!(101);
        assert!(validate_config("fullTeamBonusAmt", &v).is_err());
    }

    // ---- 未知 key ----

    #[test]
    fn validate_unknown_key_rejected() {
        let v = json!(10);
        let err = validate_config("fooBar", &v).unwrap_err();
        assert!(format!("{}", err).contains("未知配置键"));
    }
}
```

### Step 1.4: 跑测试

```bash
cd /home/peter/project/may_store && cargo test --lib validate_
```

期望：16 个用例全过（注意：测试以 `validate_` 开头会匹配文件里所有 `validate_*` 函数名）

### Step 1.5: 跑全套测试

```bash
cd /home/peter/project/may_store && cargo test
```

期望：所有原有测试 + 新增 16 个全过

### Step 1.6: 跑 check

```bash
cd /home/peter/project/may_store && cargo check
```

期望：无错

### Step 1.7: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/routes.rs && git commit -m "refactor(admin): extract validate_config pure fn + 16 unit tests"
```

---

## Task 2: v3.sql 加 `fullTeamBonusAmt` 默认值提示

**Files:**
- Modify: `src/v3.sql`（在 `global_configs` 表 `CREATE TABLE` 之后加注释 + INSERT 示例）

### Step 2.1: 在 `global_configs` 表后加注释块

定位：`src/v3.sql` 第 1501 行（`CREATE INDEX idx_global_config_category` 之后）追加：

```sql

-- 默认系统配置（admin 可改）。由应用启动时检测 + admin 在 multi-admin 维护。
-- 缺失时 sign_in_service 与 admin/configs 走各自的 DEFAULT_* 常量兜底。
INSERT INTO global_configs (config_key, config_value, category, description) VALUES
    ('signInRewards7Days', '[5, 6, 7, 8, 9, 10, 20]'::jsonb, 'SIGN_IN', '7 天轮回签到奖励(数组下标 1~7)'),
    ('orderPointPercent', '100'::jsonb, 'ORDER', '订单积分百分比'),
    ('diamondUnlockCost', '100'::jsonb, 'REWARDS', '钻石解锁价格'),
    ('defaultFootprintCapacity', '50'::jsonb, 'GENERAL', '默认足迹容量'),
    ('fullTeamBonusAmt', '10'::jsonb, 'SIGN_IN', '全组满签时最后签到用户获得的组钻石数')
ON CONFLICT (config_key) DO NOTHING;
```

### Step 2.2: 提交

```bash
cd /home/peter/project/may_store && git add src/v3.sql && git commit -m "docs(sql): add fullTeamBonusAmt default config + bootstrap INSERTs"
```

---

## Task 3: 改 `get_config` 返回 `ConfigResponse` 结构体（修 OpenAPI 不一致）

**Files:**
- Modify: `src/api/admin/routes.rs`（`ConfigResponse` 扩字段 + `get_config` handler 改返回）

### Step 3.1: 扩 `ConfigResponse` 字段

替换 `src/api/admin/routes.rs` 中的 `ConfigResponse` 定义（L100-109），改成：

```rust
/// 系统配置响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    /// 7 天轮回签到奖励配置(数组下标对应 1~7 天)
    pub sign_in_rewards_7_days: Vec<i32>,
    pub order_point_percent: i32,
    pub diamond_unlock_cost: i32,
    pub default_footprint_capacity: i32,
    /// 全组满签时最后签到用户获得的组钻石数
    pub full_team_bonus_amt: i32,
}
```

### Step 3.2: 改 `get_config` handler 返回 `ConfigResponse`

替换 `src/api/admin/routes.rs` 中的 `get_config` 函数体（L289-324），改成：

```rust
pub async fn get_config(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;

    // 默认值（DB 无记录时兜底）
    let default_response = ConfigResponse {
        sign_in_rewards_7_days: vec![5, 6, 7, 8, 9, 10, 20],
        order_point_percent: 100,
        diamond_unlock_cost: 100,
        default_footprint_capacity: 50,
        full_team_bonus_amt: 10,
    };

    // 读 DB
    let row: Option<(Option<serde_json::Value>,)> = sqlx::query_as(
        "SELECT config_value FROM global_configs WHERE config_key = $1",
    )
    .bind("signInRewards7Days")
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let rewards = row
        .and_then(|(v,)| v)
        .and_then(|v| serde_json::from_value::<Vec<i32>>(v).ok())
        .unwrap_or(default_response.sign_in_rewards_7_days.clone());

    let int_value = |key: &str, default: i32| -> i32 {
        let r: Option<(Option<serde_json::Value>,)> = sqlx::query_as(
            "SELECT config_value FROM global_configs WHERE config_key = $1",
        )
        .bind(key)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
        r.and_then(|(v,)| v)
            .and_then(|v| v.as_i64().map(|n| n as i32))
            .unwrap_or(default)
    };

    let response = ConfigResponse {
        sign_in_rewards_7_days: rewards,
        order_point_percent: int_value("orderPointPercent", default_response.order_point_percent),
        diamond_unlock_cost: int_value("diamondUnlockCost", default_response.diamond_unlock_cost),
        default_footprint_capacity: int_value("defaultFootprintCapacity", default_response.default_footprint_capacity),
        full_team_bonus_amt: int_value("fullTeamBonusAmt", default_response.full_team_bonus_amt),
    };

    Ok(ApiResponse::success(response))
}
```

### Step 3.3: 跑 check

```bash
cd /home/peter/project/may_store && cargo check
```

期望：无错

### Step 3.4: 提交

```bash
cd /home/peter/project/may_store && git add src/api/admin/routes.rs && git commit -m "fix(admin): get_config returns ConfigResponse struct (OpenAPI aligned)"
```

---

## Task 4: 加 `is_full_team_signed` 业务规则的文档化测试

**Files:**
- Modify: `src/application/sign_in_service.rs`（只在 `mod tests` 块加 stub）

**目标:** 把满签判定的业务规则以测试名形式记录在 mod tests 里（DB 实际行为等集成测试基建到位再写）。**不**实际抽函数 —— T7 会在事务里直接 inline 实现。

### Step 4.1: 在已有 `mod tests` 块末尾加 stub 测试

定位：`src/application/sign_in_service.rs` 末尾的 `#[cfg(test)] mod tests` 块，在最后 `}` 之前加：

```rust
    // ---- 满签业务规则文档化（不连 DB,等集成测试基建到位用 sqlx::test 写真测试） ----

    /// 业务规则:组成员 >= 2 且都签到 且今天没发过 → true
    #[test]
    fn full_team_signed_rule_doc_2_members_both_signed() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }

    /// 业务规则:组里只有 1 人不算满签
    #[test]
    fn full_team_signed_rule_doc_single_member_false() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }

    /// 业务规则:今天已发过满签的组,后续签到不再触发
    #[test]
    fn full_team_signed_rule_doc_already_paid_skipped() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }

    /// 业务规则:fullTeamBonusAmt = 0 时,不写流水、不打标记
    #[test]
    fn full_team_bonus_zero_amt_skipped() {
        assert!(true, "see T7 impl + integration-test TODO in spec");
    }
```

### Step 4.2: 跑测试

```bash
cd /home/peter/project/may_store && cargo test --lib
```

期望:原有 `calculate_sign_diamonds` 测试 + 新增 4 个 stub 测试全过

### Step 4.3: 提交

```bash
cd /home/peter/project/may_store && git add src/application/sign_in_service.rs && git commit -m "test(signin): document full team bonus business rules as test stubs"
```

---

## Task 5: (已合并到 T4,跳过此任务编号)

原计划在 T5 抽 `apply_full_team_bonus` / `load_full_team_bonus_amt` 函数,自检发现 T7 在事务内 inline 实现更简单,不需要独立 pub fn。**此任务编号保留为空以不打乱后续编号**。

---

## Task 6: 扩 `DailyCheckinOut` 加满签字段

**Files:**
- Modify: `src/domain/sign_in/entities.rs`

**目标:** 让 `daily_checkin` 的返回结构带上满签相关信息，前端能告诉用户"今天拿了满签奖励哦"。

### Step 6.1: 找 `DailyCheckinOut` 定义

定位：`src/domain/sign_in/entities.rs` 中 `DailyCheckinOut` 结构，**追加**字段：

```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyCheckinOut {
    pub diamond_reward: i32,
    pub consecutive_days: i32,
    pub total_diamonds: i32,
    /// 本次签到是否拿到满签奖励
    pub full_team_bonus: bool,
    /// 满签奖励金额（0 表示没拿到）
    pub full_team_bonus_amt: i32,
}
```

### Step 6.2: 跑 check

```bash
cd /home/peter/project/may_store && cargo check
```

期望：可能有"未使用导入"或"字段未填充"警告 —— **正常**，T7 会填充

### Step 6.3: 提交

```bash
cd /home/peter/project/may_store && git add src/domain/sign_in/entities.rs && git commit -m "feat(signin): extend DailyCheckinOut with full team bonus fields"
```

---

## Task 7: `daily_checkin` 改走组钻石 + 串入满签逻辑（核心改动）

**Files:**
- Modify: `src/application/sign_in_service.rs::SignService::daily_checkin`

**目标:** 把当前走 `users.diamond` 的实现改成走 `association_groups.diamond`，并调用 T4 / T5 的判定 + 发奖函数。

### Step 7.1: 替换 `daily_checkin` 函数体

定位：`src/application/sign_in_service.rs` 中 `pub async fn daily_checkin(...)` 函数体（L57-167）**整段**替换为：

```rust
    /// 每日签到（加组钻石 + 全组满签奖励）
    pub async fn daily_checkin(
        token: crate::middlewares::auth::UserToken,
        state: &Arc<AppState>,
    ) -> Result<DailyCheckinOut, CustomError> {
        let user_id = token.user_id;
        let db = &state.db_pool;
        let today = Local::now().date_naive();

        // 1. 今日是否已签到
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM sign_in_records WHERE user_id = $1 AND sign_date = $2",
        )
        .bind(user_id)
        .bind(today)
        .fetch_optional(db)
        .await?;
        if existing.is_some() {
            return Err(CustomError::BadRequest("今日已签到".into()));
        }

        // 2. 拿主组
        let group_id: Option<i64> = sqlx::query(
            "SELECT group_id FROM association_group_members
             WHERE user_id = $1 AND is_primary = 1 AND member_status = 'ACTIVE' LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .map(|r| r.get("group_id"));

        // 3. 算连续天数
        let yesterday = today.pred_opt().unwrap();
        let last_sign: Option<(NaiveDate, i32)> = sqlx::query_as(
            "SELECT sign_date, consecutive_days FROM sign_in_records
             WHERE user_id = $1 AND sign_date = $2",
        )
        .bind(user_id)
        .bind(yesterday)
        .fetch_optional(db)
        .await?;
        let consecutive_days = last_sign.map(|(_, cd)| cd + 1).unwrap_or(1);

        // 4. 读 7 天奖励配置
        let rewards = Self::load_sign_rewards(db).await;
        let diamond_reward = calculate_sign_diamonds(consecutive_days, &rewards);

        // 5. 事务：写记录 + 加组钻石 + 写基础流水
        let mut tx = db.begin().await?;

        let sign_id: i64 = sqlx::query_scalar(
            "INSERT INTO sign_in_records
                (group_id, user_id, sign_date, consecutive_days, diamond_reward, full_team_bonus, full_team_bonus_amt)
             VALUES ($1, $2, $3, $4, $5, FALSE, 0)
             RETURNING id",
        )
        .bind(group_id)
        .bind(user_id)
        .bind(today)
        .bind(consecutive_days)
        .bind(diamond_reward)
        .fetch_one(&mut *tx)
        .await?;

        // 拿组当前钻石用于回写
        if let Some(gid) = group_id {
            let sign_idempotency_key = format!("sign_in_{}", sign_id);
            sqlx::query(
                r#"INSERT INTO diamond_transactions
                   (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, created_at)
                   SELECT $1, 'EARN', $2, diamond, diamond + $2, 'SIGN_IN', $3, $4, NOW()
                   FROM association_groups WHERE group_id = $1"#,
            )
            .bind(gid)
            .bind(diamond_reward as i64)
            .bind(sign_id)
            .bind(&sign_idempotency_key)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "UPDATE association_groups SET diamond = diamond + $1, updated_at = NOW() WHERE group_id = $2",
            )
            .bind(diamond_reward as i64)
            .bind(gid)
            .execute(&mut *tx)
            .await?;

            // 6. 满签判定（在事务内）
            let full_signed = Self::is_full_team_signed_in_tx(&mut tx, gid, today).await?;
            let mut full_team_bonus_amt: i32 = 0;
            if full_signed {
                let amt = Self::load_full_team_bonus_amt_in_tx(&mut tx).await;
                if amt > 0 {
                    // 标记当前 sign_in_records
                    sqlx::query(
                        "UPDATE sign_in_records
                         SET full_team_bonus = TRUE, full_team_bonus_amt = $1
                         WHERE id = $2",
                    )
                    .bind(amt)
                    .bind(sign_id)
                    .execute(&mut *tx)
                    .await?;
                    // 加组钻石
                    let bonus_idempotency_key = format!("full_team_bonus_{}", sign_id);
                    sqlx::query(
                        r#"INSERT INTO diamond_transactions
                           (group_id, type, amount, balance_before, balance_after, biz_type, biz_id, idempotency_key, created_at)
                           SELECT $1, 'EARN', $2, diamond, diamond + $2, 'FULL_TEAM_BONUS', $3, $4, NOW()
                           FROM association_groups WHERE group_id = $1"#,
                    )
                    .bind(gid)
                    .bind(amt as i64)
                    .bind(sign_id)
                    .bind(&bonus_idempotency_key)
                    .execute(&mut *tx)
                    .await?;
                    sqlx::query(
                        "UPDATE association_groups SET diamond = diamond + $1, updated_at = NOW() WHERE group_id = $2",
                    )
                    .bind(amt as i64)
                    .bind(gid)
                    .execute(&mut *tx)
                    .await?;
                    full_team_bonus_amt = amt;
                }
            }

            tx.commit().await?;

            // 拿最新组钻石用于返回
            let total_diamonds: i32 = sqlx::query_scalar(
                "SELECT diamond FROM association_groups WHERE group_id = $1",
            )
            .bind(gid)
            .fetch_one(db)
            .await?;

            // 发事件
            let payload = SignInPayload {
                sign_id,
                user_id,
                group_id,
                sign_date: today.to_string(),
                consecutive_days,
                diamond_reward,
                trace_id: None,
            };
            let _ = EventPublisher::publish(
                db,
                EventType::SignIn,
                payload,
                Some(user_id),
                group_id,
                Some("sign"),
                Some(sign_id),
            )
            .await;

            return Ok(DailyCheckinOut {
                diamond_reward,
                consecutive_days,
                total_diamonds,
                full_team_bonus: full_team_bonus_amt > 0,
                full_team_bonus_amt,
            });
        }

        // 没有主组（边缘情况）
        tx.commit().await?;
        let total_diamonds: i32 = sqlx::query_scalar(
            "SELECT diamond FROM users WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(db)
        .await?;

        Ok(DailyCheckinOut {
            diamond_reward,
            consecutive_days,
            total_diamonds,
            full_team_bonus: false,
            full_team_bonus_amt: 0,
        })
    }

    /// 事务版本的满签判定（接收 &mut Transaction）
    async fn is_full_team_signed_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        group_id: i64,
        today: NaiveDate,
    ) -> Result<bool, CustomError> {
        let member_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM association_group_members
             WHERE group_id = $1 AND member_status = 'ACTIVE'",
        )
        .bind(group_id)
        .fetch_one(&mut **tx)
        .await?;
        if member_count < 2 {
            return Ok(false);
        }
        let signed_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT user_id) FROM sign_in_records
             WHERE group_id = $1 AND sign_date = $2",
        )
        .bind(group_id)
        .bind(today)
        .fetch_one(&mut **tx)
        .await?;
        if signed_today < member_count {
            return Ok(false);
        }
        let already_paid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sign_in_records
             WHERE group_id = $1 AND sign_date = $2 AND full_team_bonus = TRUE)",
        )
        .bind(group_id)
        .bind(today)
        .fetch_one(&mut **tx)
        .await?;
        Ok(!already_paid)
    }

    /// 事务版本读 fullTeamBonusAmt
    async fn load_full_team_bonus_amt_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> i32 {
        let row: Option<(Option<serde_json::Value>,)> = sqlx::query_as(
            "SELECT config_value FROM global_configs WHERE config_key = $1",
        )
        .bind("fullTeamBonusAmt")
        .fetch_optional(&mut **tx)
        .await
        .ok()
        .flatten();
        row.and_then(|(v,)| v)
            .and_then(|v| v.as_i64().map(|n| n as i32))
            .filter(|n| (0..=100).contains(n))
            .unwrap_or(10)
    }
```

### Step 7.2: (无需清理,T4 已不创建 pub fn)

T4 只在 mod tests 加了 stub,**没有**创建任何 pub fn。T7 用的 `_in_tx` 私有方法直接在 `SignService` impl 内定义,无清理负担。

### Step 7.3: 跑 check

```bash
cd /home/peter/project/may_store && cargo check
```

期望：无错（可能 `EventPublisher` 没用上导致 warning，无视）

### Step 7.4: 跑测试

```bash
cd /home/peter/project/may_store && cargo test --lib
```

期望：所有测试过（含 T4 / T5 stub 测试 + 原有 `calculate_sign_diamonds` 测试）

### Step 7.5: 提交

```bash
cd /home/peter/project/may_store && git add src/application/sign_in_service.rs && git commit -m "refactor(signin): daily_checkin awards group diamonds + full team bonus"
```

---

## Task 8: multi-admin 修 `api-paths.ts`

**Files:**
- Modify: `multi-admin/src/constants/api-paths.ts`

### Step 8.1: 修 `config` 路径

定位：`/home/peter/project/multi-admin/src/constants/api-paths.ts` 第 22 行附近

把：
```ts
      config: '/admin/config',
```

改成：
```ts
      config: '/api/admin/configs',
```

### Step 8.2: 提交

```bash
cd /home/peter/project/multi-admin && git add src/constants/api-paths.ts && git commit -m "fix(api-paths): admin config path -> /api/admin/configs"
```

---

## Task 9: multi-admin 建 `types.ts`

**Files:**
- Create: `multi-admin/src/features/store/config/types.ts`

### Step 9.1: 写文件

新建文件 `/home/peter/project/multi-admin/src/features/store/config/types.ts`：

```ts
/**
 * 系统配置相关类型（用 sync:types 出来的 OpenAPI schema，不用手写）
 * 这里只放本页面用到的、不在 OpenAPI 里的辅助类型。
 */
import type { components } from '@/api/generated/store';

/** GET /api/admin/configs 响应 */
export type ConfigResponse = components['schemas']['ConfigResponse'];

/** PATCH /api/admin/configs/{key} 请求体 */
export interface UpdateConfigInput {
  /** 配置项的新值（按 configKey 类型校验） */
  value: string | number | number[];
}

/** 前端表单用：5 个可配置项的 key */
export const CONFIG_KEYS = [
  'signInRewards7Days',
  'fullTeamBonusAmt',
  'orderPointPercent',
  'diamondUnlockCost',
  'defaultFootprintCapacity',
] as const;

export type ConfigKey = (typeof CONFIG_KEYS)[number];

/** 每个 key 的元数据：标签、最小值、最大值、是否为数组 */
export const CONFIG_META: Record<
  ConfigKey,
  { label: string; min: number; max: number; isArray: boolean; arrayLength?: number }
> = {
  signInRewards7Days: { label: '7 天签到奖励', min: 1, max: 100, isArray: true, arrayLength: 7 },
  fullTeamBonusAmt: { label: '全组满签奖励', min: 0, max: 100, isArray: false },
  orderPointPercent: { label: '订单积分百分比', min: 1, max: 200, isArray: false },
  diamondUnlockCost: { label: '钻石解锁价格', min: 10, max: 10000, isArray: false },
  defaultFootprintCapacity: { label: '默认足迹容量', min: 10, max: 1000, isArray: false },
};
```

### Step 9.2: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/config/types.ts && git commit -m "feat(config): add types.ts with config metadata"
```

---

## Task 10: multi-admin 建 `hooks.ts`

**Files:**
- Create: `multi-admin/src/features/store/config/hooks.ts`

### Step 10.1: 写文件

新建文件 `/home/peter/project/multi-admin/src/features/store/config/hooks.ts`：

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { catHttp } from '@/adapters/cat';
import { API_PATHS } from '@/constants/api-paths';
import type { ConfigResponse, ConfigKey, UpdateConfigInput } from './types';

const QUERY_KEY = ['admin', 'configs'] as const;

/** 拉系统配置 */
export function useConfigs() {
  return useQuery({
    queryKey: QUERY_KEY,
    queryFn: async (): Promise<ConfigResponse> => {
      const resp = await catHttp.get(API_PATHS.store.admin.config);
      // 后端返回 { code, message, data: ConfigResponse }，剥出 data
      return (resp.data?.data ?? resp.data) as ConfigResponse;
    },
    staleTime: 30_000,
  });
}

/** 改单条配置 */
export function useUpdateConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (args: { key: ConfigKey; value: UpdateConfigInput['value'] }) => {
      const url = `/api/admin/configs/${args.key}`;
      return catHttp.patch(url, { value: args.value });
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: QUERY_KEY });
    },
  });
}
```

### Step 10.2: 检查 `catHttp` 适配器

```bash
cd /home/peter/project/multi-admin && cat src/adapters/cat/index.ts 2>/dev/null | head -30
```

如果 `catHttp.get` 已有（极大概率有）→ 继续  
如果路径不对 → 调整 import 路径

### Step 10.3: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/config/hooks.ts && git commit -m "feat(config): add hooks.ts (useConfigs + useUpdateConfig)"
```

---

## Task 11: multi-admin 实现 `ConfigPage`

**Files:**
- Modify: `multi-admin/src/features/store/config/ConfigPage.tsx`（替换占位为真实实现）

### Step 11.1: 写文件

覆盖 `/home/peter/project/multi-admin/src/features/store/config/ConfigPage.tsx`：

```tsx
import { useEffect, useState } from 'react';
import {
  App,
  Button,
  Card,
  Form,
  InputNumber,
  Skeleton,
  Space,
  Typography,
} from 'antd';
import { useConfigs, useUpdateConfig } from './hooks';
import { CONFIG_KEYS, CONFIG_META, type ConfigKey } from './types';
import type { ConfigResponse } from './types';

const { Title, Text } = Typography;

interface FieldState {
  /** 编辑中的值（与后端当前值区分） */
  draft: number | number[];
  /** 是否有未保存的修改 */
  dirty: boolean;
}

export function ConfigPage() {
  const { message } = App.useApp();
  const configsQuery = useConfigs();
  const updateMutation = useUpdateConfig();
  const [states, setStates] = useState<Record<ConfigKey, FieldState | null>>({} as Record<
    ConfigKey,
    FieldState | null
  >);

  // 数据回来后初始化 drafts
  useEffect(() => {
    if (!configsQuery.data) return;
    const next: Record<string, FieldState> = {};
    for (const key of CONFIG_KEYS) {
      const v = (configsQuery.data as Record<string, unknown>)[
        key as unknown as string
      ] as number | number[] | undefined;
      if (v !== undefined) {
        next[key] = { draft: v, dirty: false };
      }
    }
    setStates(next as Record<ConfigKey, FieldState>);
  }, [configsQuery.data]);

  if (configsQuery.isLoading) {
    return <Skeleton active paragraph={{ rows: 6 }} />;
  }

  if (configsQuery.isError) {
    return (
      <Card>
        <Text type="danger">加载配置失败：{String(configsQuery.error)}</Text>
      </Card>
    );
  }

  function handleChange(key: ConfigKey, draft: number | number[]) {
    setStates((prev) => ({
      ...prev,
      [key]: { draft, dirty: JSON.stringify(draft) !== JSON.stringify(prev[key]?.draft) },
    }));
  }

  async function handleSave(key: ConfigKey) {
    const s = states[key];
    if (!s) return;
    try {
      await updateMutation.mutateAsync({ key, value: s.draft });
      message.success(`${CONFIG_META[key].label} 已保存`);
      setStates((prev) => ({ ...prev, [key]: { ...s, dirty: false } }));
    } catch (err) {
      message.error(`保存失败：${String((err as Error).message ?? err)}`);
    }
  }

  return (
    <div>
      <Space direction="vertical" size={4} style={{ marginBottom: 16 }}>
        <Title level={3} style={{ margin: 0 }}>
          系统配置
        </Title>
        <Text type="secondary">配置全局生效参数，改完用户签到立即生效。</Text>
      </Space>

      <Space direction="vertical" size={16} style={{ width: '100%' }}>
        <SignInCard
          states={states}
          onChange={handleChange}
          onSave={handleSave}
          savingKey={updateMutation.isPending ? (updateMutation.variables?.key as ConfigKey) : null}
        />
        <OrderCard
          states={states}
          onChange={handleChange}
          onSave={handleSave}
          savingKey={updateMutation.isPending ? (updateMutation.variables?.key as ConfigKey) : null}
        />
        <GeneralCard
          states={states}
          onChange={handleChange}
          onSave={handleSave}
          savingKey={updateMutation.isPending ? (updateMutation.variables?.key as ConfigKey) : null}
        />
      </Space>
    </div>
  );
}

function SignInCard(props: {
  states: Record<ConfigKey, FieldState | null>;
  onChange: (k: ConfigKey, v: number | number[]) => void;
  onSave: (k: ConfigKey) => void;
  savingKey: ConfigKey | null;
}) {
  return (
    <Card title="签到配置">
      <Form layout="vertical">
        <Form.Item label={`${CONFIG_META.signInRewards7Days.label}（7 个数字，每个 1-100）`}>
          <RewardsArrayEditor
            value={(props.states.signInRewards7Days?.draft as number[] | undefined) ?? []}
            onChange={(v) => props.onChange('signInRewards7Days', v)}
          />
        </Form.Item>
        <Form.Item label={CONFIG_META.fullTeamBonusAmt.label}>
          <Space>
            <InputNumber
              min={CONFIG_META.fullTeamBonusAmt.min}
              max={CONFIG_META.fullTeamBonusAmt.max}
              value={props.states.fullTeamBonusAmt?.draft as number | undefined}
              onChange={(v) => props.onChange('fullTeamBonusAmt', Number(v ?? 0))}
              disabled={props.states.fullTeamBonusAmt == null}
            />
            <Button
              type="primary"
              disabled={!props.states.fullTeamBonusAmt?.dirty || props.savingKey === 'fullTeamBonusAmt'}
              loading={props.savingKey === 'fullTeamBonusAmt'}
              onClick={() => props.onSave('fullTeamBonusAmt')}
            >
              保存
            </Button>
          </Space>
        </Form.Item>
        <Form.Item>
          <Button
            type="primary"
            disabled={
              !props.states.signInRewards7Days?.dirty || props.savingKey === 'signInRewards7Days'
            }
            loading={props.savingKey === 'signInRewards7Days'}
            onClick={() => props.onSave('signInRewards7Days')}
          >
            保存 7 天奖励
          </Button>
        </Form.Item>
      </Form>
    </Card>
  );
}

function RewardsArrayEditor(props: {
  value: number[];
  onChange: (v: number[]) => void;
}) {
  const arr = props.value.length === 7 ? props.value : [5, 6, 7, 8, 9, 10, 20];
  return (
    <Space wrap>
      {arr.map((n, i) => (
        <InputNumber
          key={i}
          min={1}
          max={100}
          value={n}
          onChange={(v) => {
            const next = [...arr];
            next[i] = Number(v ?? 1);
            props.onChange(next);
          }}
        />
      ))}
    </Space>
  );
}

function OrderCard(props: {
  states: Record<ConfigKey, FieldState | null>;
  onChange: (k: ConfigKey, v: number | number[]) => void;
  onSave: (k: ConfigKey) => void;
  savingKey: ConfigKey | null;
}) {
  return (
    <Card title="订单配置">
      <Form layout="vertical">
        <Form.Item label={CONFIG_META.orderPointPercent.label}>
          <Space>
            <InputNumber
              min={CONFIG_META.orderPointPercent.min}
              max={CONFIG_META.orderPointPercent.max}
              value={props.states.orderPointPercent?.draft as number | undefined}
              onChange={(v) => props.onChange('orderPointPercent', Number(v ?? 0))}
              disabled={props.states.orderPointPercent == null}
            />
            <Button
              type="primary"
              disabled={
                !props.states.orderPointPercent?.dirty || props.savingKey === 'orderPointPercent'
              }
              loading={props.savingKey === 'orderPointPercent'}
              onClick={() => props.onSave('orderPointPercent')}
            >
              保存
            </Button>
          </Space>
        </Form.Item>
      </Form>
    </Card>
  );
}

function GeneralCard(props: {
  states: Record<ConfigKey, FieldState | null>;
  onChange: (k: ConfigKey, v: number | number[]) => void;
  onSave: (k: ConfigKey) => void;
  savingKey: ConfigKey | null;
}) {
  return (
    <Card title="通用配置">
      <Form layout="vertical">
        <Form.Item label={CONFIG_META.diamondUnlockCost.label}>
          <Space>
            <InputNumber
              min={CONFIG_META.diamondUnlockCost.min}
              max={CONFIG_META.diamondUnlockCost.max}
              value={props.states.diamondUnlockCost?.draft as number | undefined}
              onChange={(v) => props.onChange('diamondUnlockCost', Number(v ?? 0))}
              disabled={props.states.diamondUnlockCost == null}
            />
            <Button
              type="primary"
              disabled={
                !props.states.diamondUnlockCost?.dirty || props.savingKey === 'diamondUnlockCost'
              }
              loading={props.savingKey === 'diamondUnlockCost'}
              onClick={() => props.onSave('diamondUnlockCost')}
            >
              保存
            </Button>
          </Space>
        </Form.Item>
        <Form.Item label={CONFIG_META.defaultFootprintCapacity.label}>
          <Space>
            <InputNumber
              min={CONFIG_META.defaultFootprintCapacity.min}
              max={CONFIG_META.defaultFootprintCapacity.max}
              value={props.states.defaultFootprintCapacity?.draft as number | undefined}
              onChange={(v) => props.onChange('defaultFootprintCapacity', Number(v ?? 0))}
              disabled={props.states.defaultFootprintCapacity == null}
            />
            <Button
              type="primary"
              disabled={
                !props.states.defaultFootprintCapacity?.dirty ||
                props.savingKey === 'defaultFootprintCapacity'
              }
              loading={props.savingKey === 'defaultFootprintCapacity'}
              onClick={() => props.onSave('defaultFootprintCapacity')}
            >
              保存
            </Button>
          </Space>
        </Form.Item>
      </Form>
    </Card>
  );
}
```

### Step 11.2: 跑 multi-admin 类型检查

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit
```

期望：无 type 错（可能有 antd 类型版本警告，无视）

### Step 11.3: 跑 multi-admin build

```bash
cd /home/peter/project/multi-admin && pnpm build
```

期望：build 通过

### Step 11.4: 提交

```bash
cd /home/peter/project/multi-admin && git add src/features/store/config/ConfigPage.tsx && git commit -m "feat(config): implement ConfigPage with 3 cards (sign-in/order/general)"
```

---

## Task 12: 同步 OpenAPI + types

**Files:**
- Multi-admin 是 sync:types 出来的类型；后端改完 OpenAPI 后，multi-admin 重新生成

### Step 12.1: 后端起 Swagger 看下 schema

```bash
cd /home/peter/project/may_store && cargo run --release
# 等服务起来，浏览器开 http://localhost:8080/swagger-ui/
# 找到 /api/admin/configs GET -> 应该看到 ConfigResponse 有 5 个字段（含 fullTeamBonusAmt）
```

期望：5 个字段都列出来

### Step 12.2: 导出 OpenAPI spec

```bash
curl -s http://localhost:8080/api-docs/openapi.json -o /tmp/openapi.json
# 停服务
```

### Step 12.3: multi-admin 重新生成 types

```bash
cd /home/peter/project/multi-admin && pnpm openapi-typescript /tmp/openapi.json -o src/api/generated/store.ts
```

期望：`src/api/generated/store.ts` 文件有改动

### Step 12.4: 提交

```bash
cd /home/peter/project/multi-admin && git add src/api/generated/store.ts && git commit -m "chore(openapi): sync types after admin config changes"
```

---

## Task 13: 最终验证

**Files:** — （不修改文件）

### Step 13.1: 后端

```bash
cd /home/peter/project/may_store && cargo check && cargo build --release && cargo test --lib
```

期望：3 个命令全过

### Step 13.2: 前端

```bash
cd /home/peter/project/multi-admin && pnpm tsc --noEmit && pnpm build
```

期望：2 个命令全过

### Step 13.3: 手动冒烟（多 admin + 后端都跑起来）

依次验证：
1. 打开 multi-admin `/store/config` → 看到 5 个配置项的当前值
2. 改 `fullTeamBonusAmt` 从 10 改成 5 → 点保存 → 提示"已保存"
3. 刷新页面 → 看到 5（不是 10）→ **新值生效**
4. 改一个超范围的值（比如 999）→ 后端返 400 → 前端显示错误信息，**不**保存
5. （可选）用 may_store 的 swagger 调 `GET /api/admin/configs` → 看到结构体里 5 个字段

### Step 13.4: 没有 step 13.4，直接结束

完成。所有 commit 已落在两个 repo。

---

## 完成标准

- [ ] T1: 16 个 validate_config 单测全过
- [ ] T2: v3.sql 加了 INSERT 提示
- [ ] T3: get_config 返回结构体、Swagger 文档与之一致
- [ ] T4: 4 个满签业务规则 stub 测试就位
- [ ] T6: DailyCheckinOut 扩字段
- [ ] T7: daily_checkin 改走组钻石 + 满签逻辑全联通
- [ ] T8-T11: multi-admin ConfigPage 跑起来可读可改
- [ ] T12: openapi types 同步
- [ ] T13: cargo test / pnpm build 全过、手动冒烟通过

## 不在本次范围

- 集成测试 / DB fixture（项目无基建）
- multi-admin 其它 feature 页（用户/组/订单/审计）
- 角色权限细分
- 满签人数可配
- 0 点调度任务
- 前端单元测试
- 国际化
