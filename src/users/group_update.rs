use std::sync::Arc;

use crate::{
    errors::CustomError,
    models::users::UserToken,
    AppState,
};
use ntex::web::{
    types::{Json, Path, State},
    Responder,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct GroupUpdateInput {
    pub group_name: String,
}

#[utoipa::path(
    put,
    path = "/groups/{group_id}",
    tag = "用户",
    summary = "修改关联组名称",
    request_body = GroupUpdateInput,
    params(
        ("group_id" = i64, Path, description = "关联组ID")
    ),
    responses(
        (status = 200, description = "修改成功"),
        (status = 400, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn update_group(
    token: UserToken,
    state: State<Arc<AppState>>,
    path: Path<i64>,
    body: Json<GroupUpdateInput>,
) -> Result<impl Responder, CustomError> {
    let group_id = path.into_inner();
    let db = &state.db_pool;

    // 检查权限：必须是该组成员
    let is_member = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM association_group_members WHERE group_id=$1 AND user_id=$2)"
    )
    .bind(group_id)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;

    if !is_member {
        return Err(CustomError::BadRequest("你不是该组成员，无法修改".into()));
    }

    sqlx::query("UPDATE association_groups SET group_name=$1, updated_at=NOW() WHERE group_id=$2")
        .bind(&body.group_name)
        .bind(group_id)
        .execute(db)
        .await?;

    Ok(ntex::web::HttpResponse::Ok().json(&serde_json::json!({ "status": "ok" })))
}
