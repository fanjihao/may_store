use crate::{errors::CustomError, utils::TOKEN_SECRET_KEY, AppState};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use ntex::{
    http::Payload,
    web::{ErrorRenderer, FromRequest, HttpRequest},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use std::{future::Future, sync::Arc};
use utoipa::ToSchema;

// ========== 枚举类型 ==========
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type)]
#[sqlx(type_name = "user_role_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UserRoleEnum {
    ORDERING,
    RECEIVING,
    ADMIN,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type)]
#[sqlx(type_name = "gender_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GenderEnum {
    MALE,
    FEMALE,
    OTHER,
    UNKNOWN,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, Type)]
#[sqlx(type_name = "login_method_enum", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LoginMethodEnum {
    PASSWORD,
    #[serde(rename = "PHONE_CODE")]
    PhoneCode,
    OAUTH,
    MIXED,
    WEIXIN
}

// ========== 输入 DTO ==========
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginInput {
    pub username: String,
    pub password: Option<String>,
    pub phone_code: Option<String>,
    pub weixin_code: Option<String>,
    pub login_method: LoginMethodEnum,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar: Option<String>,
    pub open_id: Option<String>,
    pub gender: Option<GenderEnum>,
    pub birthday: Option<chrono::NaiveDate>,
}
// ========== 输出 DTO ==========
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserPublic {
    pub user_id: i64,
    pub username: String,
    pub email: Option<String>,
    pub nick_name: Option<String>,
    pub role: UserRoleEnum,
    pub love_point: i32,
    pub avatar: Option<String>,
    pub phone: Option<String>,
    pub open_id: Option<String>,
    pub status: i16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    // 扩展字段
    pub gender: GenderEnum,
    pub birthday: Option<chrono::NaiveDate>,
    pub username_change: bool,
    pub login_method: LoginMethodEnum,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_temp_password: bool,
    pub push_id: Option<String>,
    pub last_role_switch_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 用户所在的活跃组ID（若用户不在任何组则为 null）
    pub group_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserPublic,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IsRegisterResponse {
    pub registered: bool,
}

// ========== 数据库映射结构 ==========
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserRecord {
    pub user_id: i64,
    pub username: String,
    pub email: Option<String>,
    pub nick_name: Option<String>,
    pub role: UserRoleEnum,
    pub love_point: i32,
    pub avatar: Option<String>,
    pub phone: Option<String>,
    pub open_id: Option<String>,
    pub status: i16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    // 扩展字段
    pub password_hash: Option<String>,
    pub password_algo: Option<String>,
    pub gender: GenderEnum,
    pub birthday: Option<chrono::NaiveDate>,
    pub username_change: bool,
    pub login_method: LoginMethodEnum,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
    pub password_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_temp_password: bool,
    pub push_id: Option<String>,
    pub last_role_switch_at: Option<chrono::DateTime<chrono::Utc>>,
    pub group_id: Option<i64>,
}

impl From<UserRecord> for UserPublic {
    fn from(record: UserRecord) -> Self {
        UserPublic {
            user_id: record.user_id,
            username: record.username,
            email: record.email,
            nick_name: record.nick_name,
            role: record.role,
            love_point: record.love_point,
            avatar: record.avatar,
            phone: record.phone,
            open_id: record.open_id,
            status: record.status,
            created_at: record.created_at,
            updated_at: record.updated_at,
            gender: record.gender,
            birthday: record.birthday,
            username_change: record.username_change,
            login_method: record.login_method,
            last_login_at: record.last_login_at,
            is_temp_password: record.is_temp_password,
            push_id: record.push_id,
            last_role_switch_at: record.last_role_switch_at,
            group_id: record.group_id,
        }
    }
}

// ========== Token Claims ==========
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTokenClaims {
    pub exp: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserToken {
    pub exp: i64,
    pub user_id: i64,
    pub user: Option<UserPublic>,
}

impl<E: ErrorRenderer> FromRequest<E> for UserToken {
    type Error = CustomError;

    fn from_request(
        req: &HttpRequest,
        _: &mut Payload,
    ) -> impl Future<Output = Result<Self, Self::Error>> {
        let state = req.app_state::<Arc<AppState>>().expect("app state").clone();
        let redis_cache = state.redis_cache.clone();
        let auth_header = req.headers().get("Authorization").cloned();

        // println!("Authenticating request for path: {:#?}", auth_header);
        async move {
            let mut raw = auth_header
                .ok_or_else(|| CustomError::AuthFailed("No login authorization".into()))?
                .to_str()
                .map_err(|_| CustomError::AuthFailed("Invalid header".into()))?
                .to_string();
            // 支持 'Bearer <token>' 前缀
            if let Some(stripped) = raw.strip_prefix("Bearer ") {
                raw = stripped.trim().to_string();
            }

            let decoding_key = DecodingKey::from_secret(TOKEN_SECRET_KEY);
            let validation = Validation::new(Algorithm::HS256);
            let data =
                decode::<UserTokenClaims>(&raw, &decoding_key, &validation).map_err(|e| {
                    CustomError::AuthFailed(format!("decode token error: {}", e).into())
                })?;
            let uid = data.claims.user_id;

            // 从缓存或数据库获取用户信息
            let mut public: Option<UserPublic> =
                redis_cache.get_user_public(&uid).await.ok().flatten();
            if public.is_none() {
                let db = &state.db_pool;
                if let Ok(record) = sqlx::query_as::<_, UserRecord>(
                    r#"
                    SELECT u.user_id, u.username, u.email, u.nick_name, u.role, u.love_point, u.avatar, u.phone,
                           u.open_id, u.status, u.created_at, u.updated_at, u.password_hash,
                           u.password_algo, u.gender, u.birthday, u.username_change, u.login_method,
                           u.last_login_at, u.password_updated_at, u.is_temp_password, u.push_id, u.last_role_switch_at,
                           (SELECT agm.group_id FROM association_group_members agm JOIN association_groups g ON g.group_id=agm.group_id AND g.status=1 WHERE agm.user_id=u.user_id ORDER BY agm.is_primary DESC, agm.group_id ASC LIMIT 1) AS group_id
                    FROM users u WHERE u.user_id=$1 AND u.status=1
                    "#
                )
                .bind(uid)
                .fetch_one(db)
                .await
                {
                    public = Some(record.into());
                    if let Some(ref p) = public {
                        let _ = redis_cache.set_user_public(p, 3600).await;
                    }
                }
            }

            if let Some(ref p) = public {
                // 插入一个克隆，避免生命周期问题
                req.extensions_mut().insert(p.clone());
            }

            Ok(UserToken {
                exp: data.claims.exp,
                user_id: uid,
                user: public.clone(),
            })
        }
    }
}
