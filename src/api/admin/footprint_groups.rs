// API - 后台管理 - 足迹公用分组
// 2026-07-06 新增: multi-admin 维护"公用足迹分组"(is_global=true, group_id NULL)
// 后续阶段: 扩展支持组内自建分组(is_global=false, group_id 非空)—— 同一份 API 已兼容

use ntex::web::{
    self,
    types::{Json, Path, Query, State}, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::admin_auth::AdminToken;
use crate::utils::response::ApiResponse;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/admin/footprint-groups")
            .route("", web::get().to(list_groups))
            .route("", web::post().to(create_group))
            .route("/{id}", web::patch().to(update_group))
            .route("/{id}", web::delete().to(delete_group)),
    );
}

// ========== 响应/请求结构 ==========

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FootprintGroupAdminOut {
    pub id: i64,
    pub group_id: Option<i64>,
    pub group_name: String,
    pub group_type: i16,
    pub max_capacity: i32,
    pub current_count: i32,
    pub status: i16,
    pub is_global: bool,
    pub create_time: chrono::DateTime<chrono::Utc>,
    pub update_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFootprintGroupInput {
    pub group_name: String,
    pub group_type: i16,
    pub max_capacity: i32,
    pub is_global: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFootprintGroupInput {
    pub group_name: Option<String>,
    pub group_type: Option<i16>,
    pub max_capacity: Option<i32>,
    pub status: Option<i16>,
}

#[derive(Debug, Deserialize)]
pub struct FootprintGroupQuery {
    /// 0=全部 / 1=global / 2=组内
    pub scope: Option<i16>,
}

// ========== 处理器 ==========

/// 列出足迹分组 (multi-admin 用)
async fn list_groups(
    state: State<Arc<AppState>>,
    _admin: AdminToken,
    query: Query<FootprintGroupQuery>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let scope = query.scope.unwrap_or(0);

    // 0=全部, 1=global, 2=组内
    let where_clause = match scope {
        1 => "WHERE is_global = TRUE",
        2 => "WHERE is_global = FALSE",
        _ => "",
    };

    let sql = format!(
        "SELECT id, group_id, group_name, group_type, max_capacity, current_count, status, is_global, create_time, update_time \
         FROM record_group {where_clause} ORDER BY is_global DESC, id ASC"
    );

    let rows = sqlx::query(&sql).fetch_all(db).await?;
    let items: Vec<FootprintGroupAdminOut> = rows
        .into_iter()
        .map(|r| FootprintGroupAdminOut {
            id: r.get("id"),
            group_id: r.get("group_id"),
            group_name: r.get("group_name"),
            group_type: r.get("group_type"),
            max_capacity: r.get("max_capacity"),
            current_count: r.get("current_count"),
            status: r.get("status"),
            is_global: r.get("is_global"),
            create_time: r.get("create_time"),
            update_time: r.get("update_time"),
        })
        .collect();
    Ok(ApiResponse::success(items))
}

/// 创建公用足迹分组
async fn create_group(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    body: Json<CreateFootprintGroupInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let input = body.into_inner();

    if input.group_name.trim().is_empty() {
        return Err(CustomError::BadRequest("分组名不能为空".into()));
    }
    if input.max_capacity < 1 {
        return Err(CustomError::BadRequest("容量必须 ≥ 1".into()));
    }

    // is_global=true → group_id=NULL; is_global=false → 需要后续由组端 API 传入 group_id
    // 当前阶段 multi-admin 只用 global=true 路径, is_global=false 由 admin 校验拒绝
    if !input.is_global {
        return Err(CustomError::BadRequest(
            "当前阶段只支持创建公用分组 (is_global=true)".into(),
        ));
    }

    let row = sqlx::query(
        r#"INSERT INTO record_group (group_id, group_name, group_type, max_capacity, is_global)
           VALUES (NULL, $1, $2, $3, TRUE)
           RETURNING id, group_id, group_name, group_type, max_capacity, current_count, status, is_global, create_time, update_time"#,
    )
    .bind(&input.group_name)
    .bind(input.group_type)
    .bind(input.max_capacity)
    .fetch_one(db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.constraint().is_some() => {
            CustomError::BadRequest("公用分组名重复".into())
        }
        other => CustomError::InternalServerError(format!("数据库错误: {other}")),
    })?;

    let out = FootprintGroupAdminOut {
        id: row.get("id"),
        group_id: row.get("group_id"),
        group_name: row.get("group_name"),
        group_type: row.get("group_type"),
        max_capacity: row.get("max_capacity"),
        current_count: row.get("current_count"),
        status: row.get("status"),
        is_global: row.get("is_global"),
        create_time: row.get("create_time"),
        update_time: row.get("update_time"),
    };

    // 写审计
    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, detail)
           VALUES ($1, 'ADMIN', 'FOOTPRINT_GROUP_CREATE', 'RECORD_GROUP', $2)"#,
    )
    .bind(admin.user_id)
    .bind(serde_json::json!({ "id": out.id, "name": out.group_name }))
    .execute(db)
    .await;

    Ok(ApiResponse::success(out))
}

/// 更新分组 (改名字/类型/容量/启停)
async fn update_group(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
    body: Json<UpdateFootprintGroupInput>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let id = path.into_inner();
    let input = body.into_inner();

    let row = sqlx::query(
        r#"UPDATE record_group
           SET group_name   = COALESCE($2, group_name),
               group_type   = COALESCE($3, group_type),
               max_capacity = COALESCE($4, max_capacity),
               status       = COALESCE($5, status),
               update_time  = NOW()
           WHERE id = $1
           RETURNING id, group_id, group_name, group_type, max_capacity, current_count, status, is_global, create_time, update_time"#,
    )
    .bind(id)
    .bind(&input.group_name)
    .bind(input.group_type)
    .bind(input.max_capacity)
    .bind(input.status)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| CustomError::NotFound("分组不存在".into()))?;

    let out = FootprintGroupAdminOut {
        id: row.get("id"),
        group_id: row.get("group_id"),
        group_name: row.get("group_name"),
        group_type: row.get("group_type"),
        max_capacity: row.get("max_capacity"),
        current_count: row.get("current_count"),
        status: row.get("status"),
        is_global: row.get("is_global"),
        create_time: row.get("create_time"),
        update_time: row.get("update_time"),
    };

    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, detail)
           VALUES ($1, 'ADMIN', 'FOOTPRINT_GROUP_UPDATE', 'RECORD_GROUP', $2)"#,
    )
    .bind(admin.user_id)
    .bind(serde_json::json!({ "id": id, "changes": input }))
    .execute(db)
    .await;

    Ok(ApiResponse::success(out))
}

/// 删除分组 (软删除: 改 status=0, 实际不删行, 保留 footprints.record_group_id 引用完整性)
async fn delete_group(
    state: State<Arc<AppState>>,
    admin: AdminToken,
    path: Path<i64>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let id = path.into_inner();

    let res = sqlx::query("UPDATE record_group SET status = 0, update_time = NOW() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;

    if res.rows_affected() == 0 {
        return Err(CustomError::NotFound("分组不存在".into()));
    }

    let _ = sqlx::query(
        r#"INSERT INTO audit_logs (operator_id, operator_type, action_type, target_type, detail)
           VALUES ($1, 'ADMIN', 'FOOTPRINT_GROUP_DELETE', 'RECORD_GROUP', $2)"#,
    )
    .bind(admin.user_id)
    .bind(serde_json::json!({ "id": id }))
    .execute(db)
    .await;

    Ok(ApiResponse::success(serde_json::json!({ "status": "ok", "id": id })))
}