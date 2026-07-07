// API 层 - 后台组等级配置管理
//
// 端点:
//   GET  /api/admin/group-levels       — 拉所有等级 (按 level ASC)
//   PUT  /api/admin/group-levels/{n}  — 改单个等级的 required_exp
// 鉴权: AdminToken (需要是 active admin)
//
// 备注: 改 required_exp 只影响"未来的等级判定" (get_group 实时算, 不需要回填历史 level)

use ntex::web::{
    self,
    types::{Json, Path, State},
    Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::admin_auth::AdminToken;
use crate::utils::response::ApiResponse;

/// 配置路由(在 admin::routes::configure 里被调)
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/admin/group-levels")
            .route("", web::get().to(list_group_levels))
            .route("/{level}", web::put().to(update_group_level)),
    );
}

/// 改单个等级的请求体
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupLevelInput {
    pub required_exp: i64,
}

/// 等级列表响应 (GET 端点)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevelListOut {
    pub levels: Vec<crate::domain::group::GroupLevelConfig>,
}

/// 改完返回的响应 (PUT 端点)
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupLevelUpdateOut {
    pub level: i32,
    pub required_exp: i64,
}

/// 拉所有等级 (按 level ASC)
#[utoipa::path(
    get,
    path = "/api/admin/group-levels",
    tag = "后台管理",
    responses(
        (status = 200, description = "获取成功", body = GroupLevelListOut),
        (status = 401, description = "未登录"),
        (status = 403, description = "非 admin 角色")
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_group_levels(
    _admin: AdminToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let levels =
        crate::application::group_level_service::GroupLevelService::list_levels(&state.db_pool)
            .await
            .map_err(|e| CustomError::internal(format!("拉等级表失败: {}", e)))?;
    Ok(ApiResponse::success(GroupLevelListOut { levels }))
}

/// 改单个等级的 required_exp
#[utoipa::path(
    put,
    path = "/api/admin/group-levels/{level}",
    tag = "后台管理",
    params(("level" = i32, Path, description = "等级 Lv N")),
    request_body = UpdateGroupLevelInput,
    responses(
        (status = 200, description = "更新成功", body = GroupLevelUpdateOut),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录"),
        (status = 403, description = "非 admin 角色"),
        (status = 404, description = "该等级不存在")
    ),
    security(("bearer_auth" = []))
)]
pub async fn update_group_level(
    _admin: AdminToken,
    state: State<Arc<AppState>>,
    path: Path<i32>,
    body: Json<UpdateGroupLevelInput>,
) -> Result<impl Responder, CustomError> {
    let level = path.into_inner();
    let input = body.into_inner();

    // 校验
    if level < 1 {
        return Err(CustomError::bad_request("level 必须 >= 1"));
    }
    if input.required_exp < 0 {
        return Err(CustomError::bad_request(
            "required_exp 不能为负数",
        ));
    }

    // 校验: 这一级的 required_exp 必须 >= 上一级 (保证单调递增)
    // —— 允许 admin 设成跟上一级相同, 但不能比上一级小, 否则会出现"高等级经验少"的怪事
    if level > 1 {
        let prev_exp: Option<i64> = sqlx::query_scalar(
            "SELECT required_exp FROM group_level_configs WHERE level = $1",
        )
        .bind(level - 1)
        .fetch_optional(&state.db_pool)
        .await?;
        if let Some(prev) = prev_exp {
            if input.required_exp < prev {
                return Err(CustomError::bad_request(format!(
                    "Lv {} 的 required_exp ({}) 不能小于 Lv {} 的 ({})",
                    level, input.required_exp, level - 1, prev
                )));
            }
        }
    }

    // upsert: 如果等级不存在就插入 (允许 admin 加新等级)
    sqlx::query(
        r#"INSERT INTO group_level_configs (level, required_exp, updated_at)
           VALUES ($1, $2, NOW())
           ON CONFLICT (level) DO UPDATE
           SET required_exp = EXCLUDED.required_exp, updated_at = NOW()"#,
    )
    .bind(level)
    .bind(input.required_exp)
    .execute(&state.db_pool)
    .await?;

    Ok(ApiResponse::success(GroupLevelUpdateOut {
        level,
        required_exp: input.required_exp,
    }))
}
