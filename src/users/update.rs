use std::sync::Arc;

use ntex::web::{
    types::{Json, State},
    Responder,
};

use crate::users::{hash_password, verify_password};
use crate::{
    errors::CustomError,
    models::users::{GenderEnum, UserPublic, UserRecord, UserToken},
    AppState,
};
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProfileUpdateInput {
    pub username: String,
    pub avatar: Option<String>,
    pub gender: Option<GenderEnum>,
    pub birthday: Option<chrono::NaiveDate>,
    pub new_password: Option<String>,
    pub old_password: Option<String>,
}

#[utoipa::path(
    post,
    path = "/users",
    operation_id = "wx_change_info",
    tag = "用户",
    request_body = ProfileUpdateInput,
    responses((status = 200, body = UserPublic), (status = 400, body = CustomError)),
    security(("cookie_auth" = []))
)]
pub async fn change_info(
    _: UserToken,
    data: Json<ProfileUpdateInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;

    // 先读取用户
    let rec = sqlx::query_as::<_, UserRecord>(
        "SELECT user_id, username, email, role, love_point, avatar, phone, associate_id, status, created_at, updated_at, gender, birthday, phone_verified, login_method, last_login_at, password_hash, password_algo, password_updated_at, is_temp_password, push_id, last_role_switch_at FROM users WHERE username = $1"
    )
        .bind(&data.username)
        .fetch_optional(db_pool)
        .await?;
    let rec = match rec {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("账号不存在".into())),
    };

    // 修改资料
    if data.avatar.is_some() || data.gender.is_some() || data.birthday.is_some() {
        sqlx::query("UPDATE users SET avatar = COALESCE($2, avatar), gender = COALESCE($3, gender), birthday = COALESCE($4, birthday) WHERE username = $1")
            .bind(&data.username)
            .bind(&data.avatar)
            .bind(&data.gender)
            .bind(&data.birthday)
            .execute(db_pool)
            .await?;
    }

    // 修改密码
    if let Some(new_pwd) = &data.new_password {
        // 校验旧密码（如果提供）
        if let Some(old_pwd) = &data.old_password {
            if let Some(stored) = &rec.password_hash {
                if !verify_password(old_pwd, stored).unwrap_or(false) {
                    return Err(CustomError::BadRequest("旧密码错误".into()));
                }
            } else {
                return Err(CustomError::BadRequest("无旧密码记录".into()));
            }
        }
        let (hash, algo) =
            hash_password(new_pwd).map_err(|e| CustomError::InternalError(e.into()))?;
        sqlx::query("UPDATE users SET password_hash = $2, password_algo = $3, password_updated_at = $4, is_temp_password = FALSE WHERE username = $1")
            .bind(&data.username)
            .bind(&hash)
            .bind(&algo)
            .bind(Utc::now())
            .execute(db_pool)
            .await?;
    }

    // 重新取更新后的公开信息
    let updated = sqlx::query_as::<_, UserRecord>(
        "SELECT user_id, username, email, role, love_point, avatar, phone, associate_id, status, created_at, updated_at, gender, birthday, phone_verified, login_method, last_login_at, password_hash, password_algo, password_updated_at, is_temp_password, push_id, last_role_switch_at FROM users WHERE username = $1"
    )
        .bind(&data.username)
        .fetch_one(db_pool)
        .await?;

    Ok(Json(UserPublic::from(updated)))
}
