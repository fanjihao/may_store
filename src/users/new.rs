use std::sync::Arc;

use ntex::web::{ types::{ Json, State }, Responder, HttpResponse };

use crate::users::hash_password;
use crate::{
    errors::CustomError,
    models::users::{ GenderEnum, LoginMethodEnum, RegisterInput, UserRoleEnum },
    AppState,
};
use sqlx::Row; // 仅用于读取 COUNT(*) 结果行

#[utoipa::path(
    post,
    path = "/register",
    request_body = RegisterInput,
    tag = "用户",
    responses(
        (status = 201, description = "注册成功，无响应体"),
        (status = 400, body = CustomError)
    )
)]
pub async fn register(
    data: Json<RegisterInput>,
    state: State<Arc<AppState>>
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    if data.username.is_empty() || data.password.is_empty() {
        return Err(CustomError::BadRequest("缺少账号或密码".into()));
    }

    // 检查是否已存在
    let exists_row = sqlx
        ::query("SELECT COUNT(*) FROM users WHERE username = $1")
        .bind(&data.username)
        .fetch_one(db_pool).await?;
    let exists: i64 = exists_row.get(0);
    if exists > 0 {
        return Err(CustomError::BadRequest("账号已存在".into()));
    }

    let (pwd_hash, algo) = hash_password(&data.password).map_err(|e|
        CustomError::InternalError(e.into())
    )?;

    // 仅执行插入，不再返回用户信息；执行结果不需要获取行，避免无 RETURNING 时的错误
    sqlx
        ::query(
            r#"INSERT INTO users (
                username, nick_name, open_id, password_hash, password_algo, gender, birthday, username_change, login_method, role, love_point, status, is_temp_password
            ) VALUES (
                $1, $2, $3, $4, $5, $6, NULL, FALSE, $7, $8, 0, 1, FALSE
            )"#
        )
        .bind(&data.username)
        .bind(&data.username) // 默认昵称同用户名
        .bind(&data.open_id)
        .bind(&pwd_hash)
        .bind(&algo)
        .bind(GenderEnum::UNKNOWN)
        .bind(LoginMethodEnum::PASSWORD)
        .bind(UserRoleEnum::ORDERING)
        .execute(db_pool).await?;

    Ok(HttpResponse::Created().finish())
}
