use std::sync::Arc;

use crate::models::users::IsRegisterResponse;
use crate::users::verify_password;
use crate::{
    errors::CustomError,
    models::users::{
        LoginInput, LoginMethodEnum, LoginResponse, UserPublic, UserRecord, UserToken,
        UserTokenClaims,
    },
    utils::TOKEN_SECRET_KEY,
    AppState,
};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use ntex::web::{
    types::{Json, Query, State},
    Responder,
};
// removed unused imports

use serde::Deserialize;

// 已废弃的按参数查询用户方式，改为仅获取当前登录用户；旧结构移除。

#[utoipa::path(
    post,
    path = "/login",
    tag = "用户",
    summary = "账号密码登录，返回 Token 与用户信息",
    request_body = LoginInput,
    responses(
        (status = 200, body = LoginResponse),
        (status = 400, body = CustomError)
    )
)]
pub async fn login(
    user: Json<LoginInput>,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let account = user.username.clone();

    let record = sqlx::query_as::<_, UserRecord>(
        r#"SELECT u.user_id, u.username, u.email, u.role, u.love_point, u.avatar, u.phone, u.associate_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.phone_verified, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
           (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
           FROM users u WHERE u.username = $1"#
    )
        .bind(&account)
        .fetch_optional(db_pool)
        .await?;
    let record = match record {
        Some(r) => r,
        None => return Err(CustomError::BadRequest("账号不存在".into())),
    };

    let stored = record.password_hash.clone().unwrap_or_default();
    if let Some(ref pwd) = user.password {
        if !verify_password(pwd, &stored).unwrap_or(false) {
            return Err(CustomError::BadRequest("账号或密码错误".into()));
        }
    } else {
        return Err(CustomError::BadRequest("缺少密码".into()));
    }

    // 更新 last_login_at & login_method
    sqlx::query("UPDATE users SET last_login_at = $2, login_method = $3 WHERE user_id = $1")
        .bind(record.user_id)
        .bind(Utc::now())
        .bind(LoginMethodEnum::PASSWORD)
        .execute(db_pool)
        .await?;

    let public: UserPublic = record.clone().into();
    let exp = chrono::Local::now().timestamp() + 3600 * 24 * 7;
    let claims = UserTokenClaims {
        user_id: record.user_id,
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(TOKEN_SECRET_KEY),
    )
    .map_err(|e| CustomError::InternalError(e.to_string().into()))?;

    // 缓存用户公开信息
    let _ = state.redis_cache.set_user_public(&public, 3600).await;

    Ok(Json(LoginResponse {
        token,
        user: public,
    }))
}
#[utoipa::path(
    get,
    path = "/users",
    operation_id = "get_user_info",
    tag = "用户",
    summary = "获取当前登录用户信息",
    responses(
        (status = 200, body = UserPublic),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_user_info(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<Json<UserPublic>, CustomError> {
    if let Some(u) = token.user {
        return Ok(Json(u));
    }
    // 兜底查询
    let db = &state.db_pool;
    let rec = sqlx::query_as::<_, UserRecord>(r#"
        SELECT u.user_id, u.username, u.email, u.role, u.love_point, u.avatar, u.phone, u.associate_id, u.status, u.created_at, u.updated_at, u.password_hash, u.password_algo, u.gender, u.birthday, u.phone_verified, u.login_method, u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
               (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
        FROM users u WHERE u.user_id = $1
    "#)
    .bind(token.user_id)
    .fetch_one(db)
    .await?;
    Ok(Json(rec.into()))
}
#[derive(Debug, Deserialize)]
pub struct IsRegisterQuery {
    pub username: String,
}

#[utoipa::path(
    get,
    path = "/users/is-register",
    operation_id = "is_register",
    tag = "用户",
    summary = "判断用户名是否已注册",
    params(("username" = String, Query, description = "用户名")),
    responses((status = 200, body = IsRegisterResponse), (status = 400, body = CustomError))
)]
pub async fn is_register(
    q: Query<IsRegisterQuery>,
    state: State<Arc<AppState>>,
) -> Result<Json<IsRegisterResponse>, CustomError> {
    if q.username.trim().is_empty() {
        return Err(CustomError::BadRequest("用户名不能为空".into()));
    }
    let db = &state.db_pool;
    let exists = sqlx::query_scalar::<_, i64>("SELECT user_id FROM users WHERE username = $1")
        .bind(&q.username)
        .fetch_optional(db)
        .await?;
    Ok(Json(IsRegisterResponse {
        registered: exists.is_some(),
    }))
}
