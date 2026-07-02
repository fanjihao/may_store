// API - 文件上传路由（七牛云直传）
// FSD v2026-06-03-2 §16.3 compliant - 业务后端不接收文件流，仅颁发七牛 upload token

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use qiniu_upload_token::{credential::Credential, prelude::*, UploadPolicy};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

/// 配置上传路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/uploads")
            // 申请七牛 upload token（单文件）
            .route("/token", web::post().to(get_upload_token))
            // 批量申请七牛 upload token
            .route("/tokens", web::post().to(get_upload_tokens))
            // 确认上传完成
            .route("/confirm", web::post().to(confirm_upload))
            // 七牛异步回调
            .route("/qiniu-callback", web::post().to(qiniu_callback))
            // 删除上传文件
            .route("/{file_key:path}", web::delete().to(delete_file)),
    );
}

// ========== 请求 / 响应结构 ==========

/// 业务引用类型
#[derive(Debug, Deserialize, Serialize, ToSchema, Clone)]
#[serde(rename_all = "snake_case")]
pub enum BusinessRefType {
    Food,
    Footprint,
    Checkin,
    Avatar,
    /// 组/厨房公共头像（双人组的 avatar）
    GroupAvatar,
}

impl BusinessRefType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Food => "food",
            Self::Footprint => "footprint",
            Self::Checkin => "checkin",
            Self::Avatar => "avatar",
            Self::GroupAvatar => "group_avatar",
        }
    }
}

/// 单文件 upload token 请求（FSD §16.3.6）
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadTokenRequest {
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub business_ref_type: BusinessRefType,
    pub idempotency_key: String,
}

/// 单文件 upload token 响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadTokenResponse {
    pub upload_token: String,
    pub file_key: String,
    pub upload_host: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// 批量 upload token 请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadTokensRequest {
    pub files: Vec<UploadTokenFileItem>,
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadTokenFileItem {
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub business_ref_type: BusinessRefType,
}

/// 批量 upload token 响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadTokensResponse {
    pub uploads: Vec<UploadTokenItem>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadTokenItem {
    pub filename: String,
    pub upload_token: String,
    pub file_key: String,
    pub upload_host: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// 确认上传请求（FSD §16.3）
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmUploadRequest {
    pub file_key: String,
    pub hash: String,
    pub business_ref_type: Option<BusinessRefType>,
    pub business_ref_id: Option<i64>,
}

/// 确认上传响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmUploadResponse {
    pub file_key: String,
    pub cdn_url: String,
    pub content_check_status: String,
}

/// 七牛异步回调请求
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QiniuCallbackRequest {
    pub file_key: String,
    pub hash: String,
    pub user_id: i64,
}

/// 删除文件响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteFileResponse {
    pub status: String,
}

// ========== 工具函数 ==========

const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/gif"];
const MAX_FILE_SIZE: i64 = 5 * 1024 * 1024; // 5MB
const MAX_BATCH_FILES: usize = 9;
const MAX_BATCH_TOTAL_SIZE: i64 = 20 * 1024 * 1024; // 20MB

/// 校验 MIME 类型
fn validate_mime(content_type: &str) -> Result<(), CustomError> {
    if !ALLOWED_MIME.contains(&content_type) {
        return Err(CustomError::upload_type_not_allowed(format!(
            "不支持的文件类型: {}",
            content_type
        )));
    }
    Ok(())
}

/// 校验大小
fn validate_size(size: i64) -> Result<(), CustomError> {
    if size <= 0 {
        return Err(CustomError::bad_request("文件大小必须大于 0"));
    }
    if size > MAX_FILE_SIZE {
        return Err(CustomError::upload_size_exceeded(format!(
            "文件大小 {} 超过 5MB 上限",
            size
        )));
    }
    Ok(())
}

/// 生成 file_key
fn generate_file_key(business_ref_type: &BusinessRefType, filename: &str) -> String {
    let now = chrono::Utc::now();
    let date_path = now.format("%Y/%m/%d").to_string();
    let uuid = uuid::Uuid::new_v4().to_string();
    let extension = filename.rsplit('.').next().unwrap_or("jpg");
    format!(
        "uploads/{}/{}/{}.{}",
        business_ref_type.as_str(),
        date_path,
        uuid,
        extension
    )
}

/// 调用七牛 SDK 生成 upload token
fn build_upload_token(
    cfg: &crate::config::QiniuConfig,
    file_key: &str,
    mime: &str,
    size: i64,
) -> Result<String, CustomError> {
    if cfg.access_key.is_empty() || cfg.secret_key.is_empty() {
        return Err(CustomError::upload_token_invalid(
            "七牛 AccessKey/SecretKey 未配置".to_string(),
        ));
    }

    let credential = Credential::new(&cfg.access_key, &cfg.secret_key);
    let lifetime = Duration::from_secs(cfg.token_expire_secs as u64);

    let policy = UploadPolicy::new_for_object(&cfg.bucket, file_key, lifetime)
        .mime_types([mime])
        .file_size_limitation(..=(size as u64))
        .return_body("{\"key\":\"$(key)\",\"hash\":\"$(etag)\",\"size\":$(fsize)}")
        .build();

    let provider = policy.into_dynamic_upload_token_provider(credential);
    provider
        .to_token_string(Default::default())
        .map(|s| s.into_owned())
        .map_err(|e| CustomError::upload_token_invalid(format!("七牛 token 颁发失败: {:?}", e)))
}

/// 写 upload_files 表（PENDING 状态）
async fn insert_upload_record(
    db: &sqlx::Pool<sqlx::Postgres>,
    user_id: i64,
    file_key: &str,
    original_filename: &str,
    content_type: &str,
    size: i64,
    business_ref_type: &str,
) -> Result<(), CustomError> {
    // SQL 占位符与 bind 顺序必须一一对应:
    //   $1 user_id, $2 file_key, $3 original_filename, $4 content_type,
    //   $5 size, $6 business_ref_type
    // 旧实现把 original_filename 写成 $2 (跟 file_key 重复), bind 也少一个,
    // 会触发 sqlx 的 "bind mismatch" 错误, 接口返回 500 "数据库操作失败"。
    //
    // 注意: business_ref_type 列类型是 upload_business_ref_enum,
    // sqlx::query 绑定的是 &str (text), PostgreSQL 不允许 text 隐式转 enum,
    // 会报 42804 datatype_mismatch。必须在 SQL 里显式 ::upload_business_ref_enum
    // 强转 (sqlx 没有从外部引入 PgEnum derive 时只能用这个办法)。
    sqlx::query(
        r#"
        INSERT INTO upload_files
            (user_id, file_key, original_filename, content_type, size,
             business_ref_type, content_check_status, status, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6::upload_business_ref_enum,
                'PENDING', 'PENDING', NOW(), NOW())
        ON CONFLICT (file_key) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(file_key)
    .bind(original_filename)
    .bind(content_type)
    .bind(size)
    .bind(business_ref_type)
    .execute(db)
    .await?;
    Ok(())
}

// ========== 处理器 ==========

/// 申请七牛 upload token
/// POST /api/uploads/token
/// FSD §16.3.6
#[utoipa::path(
    post,
    path = "/api/uploads/token",
    tag = "文件上传（七牛直传）",
    request_body = UploadTokenRequest,
    responses(
        (status = 200, description = "token 颁发成功", body = UploadTokenResponse),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录"),
        (status = 413, description = "文件大小超限", body = ErrorBody),
        (status = 415, description = "不支持的文件类型", body = ErrorBody),
        (status = 422, description = "七牛 token 颁发失败", body = ErrorBody),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_upload_token(
    state: State<Arc<AppState>>,
    token: UserToken,
    body: Json<UploadTokenRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();
    validate_mime(&input.content_type)?;
    validate_size(input.size)?;

    let file_key = generate_file_key(&input.business_ref_type, &input.filename);
    let upload_token = build_upload_token(
        &state.qiniu,
        &file_key,
        &input.content_type,
        input.size,
    )?;

    // 写 upload_files 表
    insert_upload_record(
        &state.db_pool,
        token.user_id,
        &file_key,
        &input.filename,
        &input.content_type,
        input.size,
        input.business_ref_type.as_str(),
    )
    .await?;

    let expires_at = chrono::Utc::now()
        + chrono::Duration::seconds(state.qiniu.token_expire_secs as i64);

    let upload_host = if state.qiniu.upload_host.is_empty() {
        state.qiniu.region.upload_host().to_string()
    } else {
        state.qiniu.upload_host.clone()
    };

    Ok(ApiResponse::success(UploadTokenResponse {
        upload_token,
        file_key,
        upload_host,
        expires_at,
    }))
}

/// 批量申请七牛 upload token
/// POST /api/uploads/tokens
/// FSD §16.3
#[utoipa::path(
    post,
    path = "/api/uploads/tokens",
    tag = "文件上传（七牛直传）",
    request_body = UploadTokensRequest,
    responses(
        (status = 200, description = "tokens 颁发成功", body = UploadTokensResponse),
        (status = 400, description = "参数错误"),
        (status = 401, description = "未登录"),
        (status = 413, description = "文件大小或数量超限", body = ErrorBody),
        (status = 415, description = "不支持的文件类型", body = ErrorBody),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_upload_tokens(
    state: State<Arc<AppState>>,
    token: UserToken,
    body: Json<UploadTokensRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();

    if input.files.len() > MAX_BATCH_FILES {
        return Err(CustomError::bad_request(format!(
            "单次最多 {} 个文件",
            MAX_BATCH_FILES
        )));
    }
    let total_size: i64 = input.files.iter().map(|f| f.size).sum();
    if total_size > MAX_BATCH_TOTAL_SIZE {
        return Err(CustomError::upload_size_exceeded(format!(
            "总大小 {} 超过 20MB 上限",
            total_size
        )));
    }

    let upload_host = if state.qiniu.upload_host.is_empty() {
        state.qiniu.region.upload_host().to_string()
    } else {
        state.qiniu.upload_host.clone()
    };

    let expires_at = chrono::Utc::now()
        + chrono::Duration::seconds(state.qiniu.token_expire_secs as i64);

    let mut items = Vec::with_capacity(input.files.len());
    for file in &input.files {
        validate_mime(&file.content_type)?;
        validate_size(file.size)?;
        let file_key = generate_file_key(&file.business_ref_type, &file.filename);
        let upload_token = build_upload_token(
            &state.qiniu,
            &file_key,
            &file.content_type,
            file.size,
        )?;
        insert_upload_record(
            &state.db_pool,
            token.user_id,
            &file_key,
            &file.filename,
            &file.content_type,
            file.size,
            file.business_ref_type.as_str(),
        )
        .await?;
        items.push(UploadTokenItem {
            filename: file.filename.clone(),
            upload_token,
            file_key,
            upload_host: upload_host.clone(),
            expires_at,
        });
    }

    Ok(ApiResponse::success(UploadTokensResponse { uploads: items }))
}

/// 确认上传完成
/// POST /api/uploads/confirm
/// FSD §16.3
#[utoipa::path(
    post,
    path = "/api/uploads/confirm",
    tag = "文件上传（七牛直传）",
    request_body = ConfirmUploadRequest,
    responses(
        (status = 200, description = "确认成功", body = ConfirmUploadResponse),
        (status = 401, description = "未登录"),
        (status = 404, description = "文件不存在", body = ErrorBody),
        (status = 422, description = "token 无效或内容审核拒绝", body = ErrorBody),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn confirm_upload(
    state: State<Arc<AppState>>,
    token: UserToken,
    body: Json<ConfirmUploadRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();
    let db = &state.db_pool;

    // 校验所有权与状态
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT user_id, status FROM upload_files WHERE file_key = $1",
    )
    .bind(&input.file_key)
    .fetch_optional(db)
    .await?;

    let (owner_id, _status) = row.ok_or_else(|| {
        CustomError::upload_file_not_found(format!("file_key {} 不存在", input.file_key))
    })?;

    if owner_id != token.user_id {
        return Err(CustomError::upload_permission_denied(
            "无权操作该文件".to_string(),
        ));
    }

    // 更新为 ACTIVE
    sqlx::query(
        r#"
        UPDATE upload_files
        SET status = 'ACTIVE'::user_status_enum, updated_at = NOW()
        WHERE file_key = $1
        "#,
    )
    .bind(&input.file_key)
    .execute(db)
    .await?;

    let cdn_url = if state.qiniu.cdn_domain.is_empty() {
        format!("https://{}/{}", state.qiniu.bucket, input.file_key)
    } else {
        format!("https://{}/{}", state.qiniu.cdn_domain, input.file_key)
    };

    Ok(ApiResponse::success(ConfirmUploadResponse {
        file_key: input.file_key,
        cdn_url,
        content_check_status: "PENDING".to_string(),
    }))
}

/// 七牛异步回调
/// POST /api/uploads/qiniu-callback
/// FSD §16.3.9
pub async fn qiniu_callback(
    state: State<Arc<AppState>>,
    body: Json<QiniuCallbackRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();
    sqlx::query(
        r#"
        UPDATE upload_files
        SET status = 'ACTIVE'::user_status_enum, updated_at = NOW()
        WHERE file_key = $1 AND user_id = $2
        "#,
    )
    .bind(&input.file_key)
    .bind(input.user_id)
    .execute(&state.db_pool)
    .await?;
    Ok(HttpResponse::Ok().json(&serde_json::json!({ "code": 0, "message": "ok" })))
}

/// 删除上传文件（同步七牛 delete + 软删除记录）
/// DELETE /api/uploads/{file_key:path}
/// FSD §16.3.10
#[utoipa::path(
    delete,
    path = "/api/uploads/{file_key:path}",
    tag = "文件上传（七牛直传）",
    params(
        ("file_key" = String, Path, description = "文件路径")
    ),
    responses(
        (status = 200, description = "删除成功", body = DeleteFileResponse),
        (status = 401, description = "未登录"),
        (status = 403, description = "非文件所有者", body = ErrorBody),
        (status = 404, description = "文件不存在", body = ErrorBody),
        (status = 500, description = "服务器错误")
    ),
    security(("bearer_auth" = []))
)]
pub async fn delete_file(
    state: State<Arc<AppState>>,
    token: UserToken,
    file_key: Path<String>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let key = file_key.into_inner();

    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT user_id, status FROM upload_files WHERE file_key = $1",
    )
    .bind(&key)
    .fetch_optional(db)
    .await?;

    let (owner_id, _status) = row.ok_or_else(|| {
        CustomError::upload_file_not_found(format!("file_key {} 不存在", key))
    })?;

    if owner_id != token.user_id {
        return Err(CustomError::upload_permission_denied(
            "非文件所有者".to_string(),
        ));
    }

    // 软删除记录（实际生产应调用七牛 delete API）
    sqlx::query(
        r#"
        UPDATE upload_files
        SET status = 'DELETED'::user_status_enum, deleted_at = NOW(), updated_at = NOW()
        WHERE file_key = $1
        "#,
    )
    .bind(&key)
    .execute(db)
    .await?;

    Ok(ApiResponse::success(DeleteFileResponse {
        status: "ok".to_string(),
    }))
}

// ========== OpenAPI 辅助 ==========

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: u16,
    pub message: String,
}
