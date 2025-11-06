use std::sync::Arc;

use ntex::web::{
    types::{Json, State},
    Responder,
};

use crate::users::hash_password;
use crate::{
    errors::CustomError,
    models::users::{
        GenderEnum, LoginMethodEnum, RegisterInput, UserPublic, UserRecord, UserRoleEnum,
    },
    AppState,
};
use sqlx::Row; // bring Row trait for exists_row.get(0)

#[utoipa::path(
    post,
    path = "/register",
    request_body = RegisterInput,
    tag = "用户",
    responses(
        (status = 201, body = UserPublic),
        (status = 400, body = CustomError)
    )
)]
pub async fn register(
    data: Json<RegisterInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    if data.username.is_empty() || data.password.is_empty() {
        return Err(CustomError::BadRequest("缺少账号或密码".into()));
    }

    // 检查是否已存在
    let exists_row = sqlx::query("SELECT COUNT(*) FROM users WHERE username = $1")
        .bind(&data.username)
        .fetch_one(db_pool)
        .await?;
    let exists: i64 = exists_row.get(0);
    if exists > 0 {
        return Err(CustomError::BadRequest("账号已存在".into()));
    }

    let (pwd_hash, algo) =
        hash_password(&data.password).map_err(|e| CustomError::InternalError(e.into()))?;

    // 插入并返回所需列
    let inserted = sqlx::query_as::<_, UserRecord>(
        "INSERT INTO users (username, password_hash, password_algo, gender, birthday, phone_verified, login_method, role, love_point, status, is_temp_password)
         VALUES ($1, $2, $3, $4, NULL, FALSE, $5, $6, 0, 1, FALSE)
         RETURNING user_id, username, email, role, love_point, avatar, phone, associate_id, status, created_at, updated_at, password_hash, password_algo, gender, birthday, phone_verified, login_method, last_login_at, password_updated_at, is_temp_password, push_id, last_role_switch_at"
    )
        .bind(&data.username)
        .bind(&pwd_hash)
        .bind(&algo)
        .bind(GenderEnum::UNKNOWN)
        .bind(LoginMethodEnum::PASSWORD)
        .bind(UserRoleEnum::ORDERING)
        .fetch_one(db_pool)
        .await?;

    Ok(Json(UserPublic::from(inserted)))
}
