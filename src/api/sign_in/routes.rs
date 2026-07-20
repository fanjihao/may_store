// API - 签到路由
// FSD.latest.md compliant - 签到、连续签到、组钻石奖励

use ntex::web::{self, types::State, Responder, ServiceConfig};
use serde::Deserialize;
use serde::Serialize;
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::application::sign_in_service::SignService;
use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::middlewares::require_group::RequireGroup;
use crate::middlewares::target_group::require_active_target_group_member;
use crate::utils::response::ApiResponse;

/// 配置签到路由
pub fn configure(cfg: &mut ServiceConfig) {
    // 不用 web::scope —— 避免圈住路径
    cfg.service(web::resource("/api/groups/{group_id}/sign-in").route(web::post().to(sign_in)));
    cfg.service(
        web::resource("/api/groups/{group_id}/sign-in/status").route(web::get().to(sign_in_status)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/sign-in/calendar")
            .route(web::get().to(get_sign_in_calendar)),
    );
    cfg.service(
        web::resource("/api/groups/{group_id}/sign-ins").route(web::get().to(get_sign_ins)),
    );
}

/// 算"当前连续签到天数"——前端在没签到今天时也能看到 streak 还在
///
/// 规则(按业务诉求"昨天签了算 1 连续"):
/// - `last_sign_date` 是今天 → 用 `last_consecutive_days`(今天刚签过)
/// - `last_sign_date` 是昨天 → 用 `last_consecutive_days`(streak 还在,等今天签)
/// - `last_sign_date` 比昨天更早 OR 从没签过 → 0(streak 已断)
pub fn compute_current_consecutive_days(
    today: chrono::NaiveDate,
    last_sign_date: Option<chrono::NaiveDate>,
    last_consecutive_days: Option<i32>,
) -> i32 {
    match last_sign_date {
        None => 0,
        Some(d) => {
            let yesterday = today.pred_opt().unwrap();
            if d == today || d == yesterday {
                last_consecutive_days.unwrap_or(0)
            } else {
                0
            }
        }
    }
}

// ========== 响应结构 ==========

/// 签到信息响应
#[derive(Debug, Serialize, ToSchema)]
pub struct SignInfoResponseWrapper {
    pub today_signed: bool,
    pub consecutive_days: i32,
    pub total_sign_days: i32,
    pub today_diamonds: i32,
    pub last_sign_date: Option<String>,
}

/// 每日签到响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DailyCheckinResponse {
    pub diamond_reward: i32,
    pub consecutive_days: i32,
    pub total_diamonds: i32,
    /// 本次签到是否拿到满签奖励
    pub full_team_bonus: bool,
    /// 满签奖励金额（0 表示没拿到）
    pub full_team_bonus_amt: i32,
}

/// 签到记录项
#[derive(Debug, Serialize, ToSchema)]
pub struct SignRecordItem {
    pub sign_id: i64,
    pub user_id: i64,
    pub nickname: Option<String>,
    pub sign_date: String,
    pub consecutive_days: i32,
    pub diamond_reward: i32,
}

/// 组内双方签到状态响应
#[derive(Debug, Serialize, ToSchema)]
pub struct SignInStatusResponse {
    pub date: String,
    pub members: Vec<MemberSignStatus>,
    pub full_team_today: bool,
    /// 当前用户今日是否已签到
    pub today_signed: bool,
    /// 当前用户连续签到天数
    pub consecutive_days: i32,
    /// 当前用户累计签到天数
    pub total_sign_days: i32,
    /// 当前用户今日签到获得的钻石数(未签到时为 0)
    pub today_diamonds: i32,
    /// 7 天连续签到奖励配置(全局,管理员可配)
    /// 数组下标 1~7 对应连续第 N 天的奖励钻石
    /// 兜底值见 SignService::load_sign_rewards
    pub daily_checkin_rewards: Vec<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberSignStatus {
    pub user_id: i64,
    pub signed: bool,
    pub consecutive_days: i32,
}

// ===== 2026-07-08 月历签到响应 (前端日历视图用) =====

/// 组成员基本信息 (日历视图的"两半"标识)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarMember {
    pub user_id: i64,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
}

/// 单人单日签到信息
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSigner {
    pub user_id: i64,
    /// 签到时间(用于前端判断"第一个签到的"和"第二个签到的")
    pub signed_at: String,
}

/// 单日签到状态 (一天内可能 0/1/2 人签了)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub date: String,
    pub signers: Vec<CalendarSigner>,
}

/// 月历签到响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInCalendarResponse {
    /// 传入的月份 (YYYY-MM)
    pub month: String,
    pub members: Vec<CalendarMember>,
    /// 月内每一天 (无签到的日子也会返回, signers 为空数组)
    pub days: Vec<CalendarDay>,
}

/// 月历查询参数
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInCalendarQuery {
    /// 年月, 格式 YYYY-MM; 不传默认当前月
    pub month: Option<String>,
}

// ========== 处理器 ==========

/// 当日签到
/// POST /api/groups/{group_id}/sign-in
#[utoipa::path(
    post,
    path = "/api/groups/{group_id}/sign-in",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "签到成功", body = DailyCheckinResponse),
        (status = 400, description = "今日已签到"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn sign_in(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let group_id = *path;
    require_active_target_group_member(&state.db_pool, token.user_id, group_id).await?;
    let app_state = (*state).clone();
    let result = SignService::daily_checkin(token, group_id, &app_state).await?;
    Ok(ApiResponse::success(DailyCheckinResponse {
        diamond_reward: result.diamond_reward,
        consecutive_days: result.consecutive_days,
        total_diamonds: result.total_diamonds,
        full_team_bonus: result.full_team_bonus,
        full_team_bonus_amt: result.full_team_bonus_amt,
    }))
}

/// 获取组内双方签到状态
/// GET /api/groups/{group_id}/sign-in/status
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/sign-in/status",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID")
    ),
    responses(
        (status = 200, description = "获取成功", body = SignInStatusResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn sign_in_status(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: ntex::web::types::Path<i64>,
) -> Result<impl Responder, CustomError> {
    let group_id = *path;
    let db = &state.db_pool;

    require_active_target_group_member(db, token.user_id, group_id).await?;

    // 获取今天日期
    let today = chrono::Utc::now().date_naive();

    // 获取组成员及其今天的签到状态
    let members = sqlx::query(
        r#"SELECT agm.user_id, sr.sign_date, sr.consecutive_days
           FROM association_group_members agm
           LEFT JOIN sign_in_records sr ON sr.user_id = agm.user_id AND sr.group_id = agm.group_id AND sr.sign_date = $2
           WHERE agm.group_id = $1 AND agm.member_status = 'ACTIVE'::group_member_status_enum"#
    )
    .bind(group_id)
    .bind(today)
    .fetch_all(db)
    .await?;

    // 额外查一次"当前用户最近一次签到记录" —— 用它算 streak 是否还在
    // 原因: 原来的 LEFT JOIN 只匹配 sr.sign_date = today, 今天没签就拿不到 streak
    let last_sign_row: Option<(chrono::NaiveDate, i32)> = sqlx::query_as(
        "SELECT sign_date, consecutive_days FROM sign_in_records
         WHERE group_id = $1 AND user_id = $2
         ORDER BY sign_date DESC LIMIT 1",
    )
    .bind(group_id)
    .bind(token.user_id)
    .fetch_optional(db)
    .await?;
    let (last_sign_date, last_consecutive_days) = match last_sign_row {
        Some((d, cd)) => (Some(d), Some(cd)),
        None => (None, None),
    };

    let mut member_statuses = Vec::new();
    let mut all_signed = true;
    for r in members {
        let user_id: i64 = r.get("user_id");
        let sign_date: Option<chrono::NaiveDate> = r.get("sign_date");
        let consecutive_days: Option<i32> = r.get("consecutive_days");
        let signed = sign_date.is_some();
        if !signed {
            all_signed = false;
        }
        member_statuses.push(MemberSignStatus {
            user_id,
            signed,
            consecutive_days: consecutive_days.unwrap_or(0),
        });
    }

    // 加载 7 天奖励配置
    let daily_checkin_rewards = SignService::load_sign_rewards(db).await;

    // 找出当前用户
    let my_member = member_statuses.iter().find(|m| m.user_id == token.user_id);

    // 当前用户今日签到状态
    let today_signed = my_member.map(|m| m.signed).unwrap_or(false);
    // 连续天数用 compute_current_consecutive_days 算 —— 关键修复:
    // 今天没签但昨天签了 → 仍然返回"streak 还在"
    let consecutive_days =
        compute_current_consecutive_days(today, last_sign_date, last_consecutive_days);

    // 累计签到天数(从 sign_in_records 查 COUNT)
    let total_sign_days: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sign_in_records WHERE user_id = $1 AND group_id = $2",
    )
    .bind(token.user_id)
    .bind(group_id)
    .fetch_one(db)
    .await?;

    // 今日签到获得的钻石数(从 daily_checkin_rewards 数组按 consecutive_days 索引取)
    // 数组下标 1~7 对应连续第 N 天的奖励,索引为 consecutive_days - 1
    let today_diamonds = if today_signed && consecutive_days > 0 {
        let idx = (consecutive_days as usize).saturating_sub(1);
        daily_checkin_rewards.get(idx).copied().unwrap_or(0)
    } else {
        0
    };

    Ok(ApiResponse::success(SignInStatusResponse {
        date: today.to_string(),
        members: member_statuses,
        full_team_today: all_signed,
        today_signed,
        consecutive_days,
        total_sign_days: total_sign_days as i32,
        today_diamonds,
        daily_checkin_rewards,
    }))
}

/// 获取月历签到状态 (2026-07-08 新增)
///
/// GET /api/groups/{group_id}/sign-in/calendar?month=2026-07
///
/// 返回指定月份内, 每组成员每天的签到情况.
/// - `members`: 组里所有 ACTIVE 成员 (顺序固定, 前端用来标"两半"是谁)
/// - `days`: 月内每一天的签到情况, signers 按签到时间升序 (signers[0] = 第一个签到的, signers[1] = 第二个)
///   前端用这个顺序画"对角线分两半": 上半 = signers[0], 下半 = signers[1]
/// - `month` 不传 → 默认当前月
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/sign-in/calendar",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ("month" = Option<String>, Query, description = "年月 YYYY-MM, 不传默认当前月")
    ),
    responses(
        (status = 200, description = "获取成功", body = SignInCalendarResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_sign_in_calendar(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: ntex::web::types::Path<i64>,
    query: ntex::web::types::Query<SignInCalendarQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = *path;
    let db = &state.db_pool;

    require_active_target_group_member(db, token.user_id, group_id).await?;

    // 解析 month 参数, 不传或格式错 → 默认当前月
    let (year, month) = match query.month.as_deref() {
        Some(s) if s.len() == 7 && s.chars().nth(4) == Some('-') => {
            let y: i32 = s[..4]
                .parse()
                .map_err(|_| CustomError::BadRequest("month 格式应为 YYYY-MM".into()))?;
            let m: u32 = s[5..]
                .parse()
                .map_err(|_| CustomError::BadRequest("month 格式应为 YYYY-MM".into()))?;
            if !(1..=12).contains(&m) {
                return Err(CustomError::BadRequest("month 月份必须在 1-12".into()));
            }
            (y, m)
        }
        _ => {
            let now = chrono::Utc::now();
            (
                now.format("%Y").to_string().parse().unwrap(),
                now.format("%m").to_string().parse().unwrap(),
            )
        }
    };

    // 月份起止日期 (用 chrono's NaiveDate 算, 跨年也能算)
    let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| CustomError::BadRequest("无效的月份".into()))?;
    let end = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(|| CustomError::BadRequest("无效的月份".into()))?;

    // 1) 查组成员基本信息 (固定顺序: 按 user_id 升序)
    let member_rows: Vec<(i64, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT u.user_id, u.nick_name, u.avatar
           FROM association_group_members agm
           JOIN users u ON u.user_id = agm.user_id
           WHERE agm.group_id = $1 AND agm.member_status = 'ACTIVE'::group_member_status_enum
           ORDER BY agm.user_id ASC"#,
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;

    let members: Vec<CalendarMember> = member_rows
        .into_iter()
        .map(|(uid, nick, ava)| CalendarMember {
            user_id: uid,
            nick_name: nick,
            avatar: ava,
        })
        .collect();

    // 2) 查月份内的签到记录 (按 sign_date, created_at 排序, 前端用 created_at 判"先/后")
    let sign_rows: Vec<(i64, chrono::NaiveDate, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"SELECT user_id, sign_date, created_at
           FROM sign_in_records
           WHERE group_id = $1 AND sign_date >= $2 AND sign_date < $3
           ORDER BY sign_date ASC, created_at ASC"#,
    )
    .bind(group_id)
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    // 3) 按日期 group 起来 (前端要的"按天分")
    let mut days_map: std::collections::BTreeMap<chrono::NaiveDate, Vec<CalendarSigner>> =
        std::collections::BTreeMap::new();
    for (uid, sign_date, signed_at) in sign_rows {
        days_map.entry(sign_date).or_default().push(CalendarSigner {
            user_id: uid,
            signed_at: signed_at.to_rfc3339(),
        });
    }

    // 4) 生成"月内每一天"的列表 (没签到的日子 signers=[])
    // 用 BTreeMap 已经是日期升序, 顺序 OK
    let days: Vec<CalendarDay> = days_map
        .into_iter()
        .map(|(date, signers)| CalendarDay {
            date: date.to_string(),
            signers,
        })
        .collect();

    Ok(ApiResponse::success(SignInCalendarResponse {
        month: format!("{:04}-{:02}", year, month),
        members,
        days,
    }))
}

/// 获取签到记录
/// GET /api/groups/{group_id}/sign-ins
#[utoipa::path(
    get,
    path = "/api/groups/{group_id}/sign-ins",
    tag = "签到",
    params(
        ("group_id" = i64, Path, description = "组ID"),
        ("year_month" = Option<String>, Query, description = "年月，如2026-06")
    ),
    responses(
        (status = 200, description = "获取成功"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非组成员"),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_sign_ins(
    state: State<Arc<AppState>>,
    token: UserToken,
    _require: RequireGroup,
    path: ntex::web::types::Path<i64>,
    query: ntex::web::types::Query<SignInsQuery>,
) -> Result<impl Responder, CustomError> {
    let group_id = *path;
    let db = &state.db_pool;

    require_active_target_group_member(db, token.user_id, group_id).await?;

    let (start, next_month_start) = match query.year_month.as_deref() {
        Some(value) => {
            let (start, next_month_start) = parse_year_month(value)?;
            (Some(start), Some(next_month_start))
        }
        None => (None, None),
    };

    let records = sqlx::query(
        r#"SELECT sr.id, sr.user_id, sr.sign_date, sr.consecutive_days, sr.diamond_reward, u.nick_name
           FROM sign_in_records sr
           JOIN users u ON u.user_id = sr.user_id
           WHERE sr.group_id = $1
             AND ($2::date IS NULL OR (sr.sign_date >= $2 AND sr.sign_date < $3))
           ORDER BY sr.sign_date DESC
           LIMIT 31"#,
    )
    .bind(group_id)
    .bind(start)
    .bind(next_month_start)
    .fetch_all(db)
    .await?;

    let items: Vec<serde_json::Value> = records
        .iter()
        .map(|r| {
            serde_json::json!({
                "signId": r.get::<i64, _>("id"),
                "userId": r.get::<i64, _>("user_id"),
                "nickname": r.get::<Option<String>, _>("nick_name"),
                "signDate": r.get::<chrono::NaiveDate, _>("sign_date").to_string(),
                "consecutiveDays": r.get::<i32, _>("consecutive_days"),
                "diamondsEarned": r.get::<i32, _>("diamond_reward")
            })
        })
        .collect();

    Ok(ApiResponse::success(serde_json::json!({
        "records": items,
        "totalCount": items.len()
    })))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SignInsQuery {
    pub year_month: Option<String>,
}

fn parse_year_month(value: &str) -> Result<(chrono::NaiveDate, chrono::NaiveDate), CustomError> {
    let bytes = value.as_bytes();
    if bytes.len() != 7
        || bytes[4] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..].iter().all(u8::is_ascii_digit)
    {
        return Err(CustomError::BadRequest(
            "year_month 格式应为 YYYY-MM".into(),
        ));
    }

    let year = value[..4]
        .parse::<i32>()
        .map_err(|_| CustomError::BadRequest("year_month 格式应为 YYYY-MM".into()))?;
    let month = value[5..]
        .parse::<u32>()
        .map_err(|_| CustomError::BadRequest("year_month 格式应为 YYYY-MM".into()))?;
    let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| CustomError::BadRequest("year_month 不是有效月份".into()))?;
    let (next_year, next_month) = if month == 12 {
        (
            year.checked_add(1)
                .ok_or_else(|| CustomError::BadRequest("year_month 超出支持范围".into()))?,
            1,
        )
    } else {
        (year, month + 1)
    };
    let next_month_start = chrono::NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .ok_or_else(|| CustomError::BadRequest("year_month 超出支持范围".into()))?;

    Ok((start, next_month_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_month_builds_half_open_month_range() {
        let (start, next_month_start) = parse_year_month("2026-01").unwrap();
        assert_eq!(start, chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(
            next_month_start,
            chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()
        );
    }

    #[test]
    fn year_month_handles_december_year_boundary() {
        let (start, next_month_start) = parse_year_month("2026-12").unwrap();
        assert_eq!(start, chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(
            next_month_start,
            chrono::NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
        );
    }

    #[test]
    fn year_month_handles_leap_february_boundary() {
        let (start, next_month_start) = parse_year_month("2028-02").unwrap();
        assert_eq!(start, chrono::NaiveDate::from_ymd_opt(2028, 2, 1).unwrap());
        assert_eq!(
            next_month_start,
            chrono::NaiveDate::from_ymd_opt(2028, 3, 1).unwrap()
        );
    }

    #[test]
    fn year_month_rejects_non_strict_or_invalid_values() {
        for value in [
            "2026-6",
            "26-06",
            "2026/06",
            "2026-00",
            "2026-13",
            "2026-06' OR 1=1",
        ] {
            assert!(matches!(
                parse_year_month(value),
                Err(CustomError::BadRequest(_))
            ));
        }
    }

    #[test]
    fn sign_in_status_response_contains_rewards_field() {
        // 验证响应结构体序列化后包含 daily_checkin_rewards 字段
        let resp = SignInStatusResponse {
            date: "2026-06-23".to_string(),
            members: vec![],
            full_team_today: false,
            today_signed: false,
            consecutive_days: 0,
            total_sign_days: 0,
            today_diamonds: 0,
            daily_checkin_rewards: vec![5, 6, 7, 8, 9, 10, 20],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("daily_checkin_rewards").is_some());
        let arr = json
            .get("daily_checkin_rewards")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(arr.len(), 7);
        assert_eq!(arr[0].as_i64().unwrap(), 5);
        assert_eq!(arr[6].as_i64().unwrap(), 20);
    }

    #[test]
    fn sign_in_status_response_serializes_all_four_new_top_level_fields() {
        // 验证响应序列化后包含 4 个新顶层字段: today_signed / consecutive_days / total_sign_days / today_diamonds
        let resp = SignInStatusResponse {
            date: "2026-06-23".to_string(),
            members: vec![],
            full_team_today: false,
            today_signed: true,
            consecutive_days: 3,
            total_sign_days: 15,
            today_diamonds: 7,
            daily_checkin_rewards: vec![5, 6, 7, 8, 9, 10, 20],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json.get("today_signed").unwrap().as_bool().unwrap(), true);
        assert_eq!(json.get("consecutive_days").unwrap().as_i64().unwrap(), 3);
        assert_eq!(json.get("total_sign_days").unwrap().as_i64().unwrap(), 15);
        assert_eq!(json.get("today_diamonds").unwrap().as_i64().unwrap(), 7);
    }

    #[test]
    fn compute_current_consecutive_days_today_signed() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        // 今天签了, consecutive_days=3 → 保留 3
        assert_eq!(
            compute_current_consecutive_days(today, Some(today), Some(3)),
            3
        );
    }

    #[test]
    fn compute_current_consecutive_days_yesterday_only() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let yesterday = chrono::NaiveDate::from_ymd_opt(2026, 6, 24).unwrap();
        // 昨天签了,今天没签,streak 还在 → 保留 1
        assert_eq!(
            compute_current_consecutive_days(today, Some(yesterday), Some(1)),
            1
        );
    }

    #[test]
    fn compute_current_consecutive_days_broken() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let three_days_ago = chrono::NaiveDate::from_ymd_opt(2026, 6, 22).unwrap();
        // 3 天前签的,streak 断了 → 0
        assert_eq!(
            compute_current_consecutive_days(today, Some(three_days_ago), Some(5)),
            0
        );
    }

    #[test]
    fn compute_current_consecutive_days_never_signed() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        // 从没签过 → 0
        assert_eq!(compute_current_consecutive_days(today, None, None), 0);
    }
}
