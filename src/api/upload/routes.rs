// API - 文件上传路由（七牛云直传）
// FSD v2026-06-03-2 §16.3 compliant - 业务后端不接收文件流，仅颁发七牛 upload token

use ntex::web::{
    self,
    types::{Json, Path, State},
    Responder, ServiceConfig,
};
use qiniu_sdk::objects::{apis::http_client::ResponseErrorKind, ObjectsManager};
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

#[derive(Debug, sqlx::FromRow)]
struct UploadRecord {
    pub user_id: i64,
    pub status: String,
    pub size: Option<i64>,
    pub qiniu_hash: Option<String>,
    pub cdn_url: Option<String>,
    pub content_check_status: String,
    pub business_ref_type: Option<String>,
    pub business_ref_id: Option<i64>,
}

#[derive(Debug)]
struct QiniuObjectMetadata {
    hash: String,
    size: u64,
}

/// 删除文件响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteFileResponse {
    pub status: String,
}

// ========== 工具函数 ==========

const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
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

fn build_cdn_url(cfg: &crate::config::QiniuConfig, file_key: &str) -> String {
    let domain = if cfg.cdn_domain.is_empty() {
        cfg.bucket.as_str()
    } else {
        cfg.cdn_domain.as_str()
    };
    let base_url = if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.trim_end_matches('/').to_string()
    } else {
        format!("https://{}", domain.trim_end_matches('/'))
    };
    format!("{}/{}", base_url, file_key.trim_start_matches('/'))
}

async fn stat_qiniu_object(
    cfg: &crate::config::QiniuConfig,
    file_key: &str,
) -> Result<QiniuObjectMetadata, CustomError> {
    if cfg.access_key.is_empty() || cfg.secret_key.is_empty() {
        return Err(CustomError::upload_token_invalid(
            "七牛 AccessKey/SecretKey 未配置".to_string(),
        ));
    }

    let access_key = cfg.access_key.clone();
    let secret_key = cfg.secret_key.clone();
    let bucket_name = cfg.bucket.clone();
    let object_key = file_key.to_string();
    let error_key = object_key.clone();

    tokio::task::spawn_blocking(move || {
        let credential = Credential::new(access_key, secret_key);
        let manager = ObjectsManager::new(credential);
        let bucket = manager.bucket(bucket_name);
        let response =
            bucket
                .stat_object(&object_key)
                .call()
                .map_err(|error| match error.kind() {
                    ResponseErrorKind::StatusCodeError(status)
                        if status.as_u16() == 404 || status.as_u16() == 612 =>
                    {
                        CustomError::upload_file_not_found(format!(
                            "七牛对象 {} 不存在",
                            object_key
                        ))
                    }
                    _ => CustomError::internal(format!("七牛对象核验失败: {}", error)),
                })?;

        let body = response.into_body();
        let value: &serde_json::Value = body.as_ref();
        let hash = value
            .get("hash")
            .and_then(serde_json::Value::as_str)
            .filter(|hash| !hash.is_empty())
            .ok_or_else(|| CustomError::internal("七牛 stat 响应缺少 hash"))?;
        let size = value
            .get("fsize")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| CustomError::internal("七牛 stat 响应缺少 fsize"))?;

        Ok(QiniuObjectMetadata {
            hash: hash.to_string(),
            size,
        })
    })
    .await
    .map_err(|error| {
        CustomError::internal(format!("七牛对象 {} 核验任务失败: {}", error_key, error))
    })?
}

fn validate_qiniu_object(
    client_hash: &str,
    expected_size: i64,
    object: &QiniuObjectMetadata,
) -> Result<(), CustomError> {
    if client_hash.trim().is_empty() {
        return Err(CustomError::bad_request("七牛 hash 不能为空"));
    }
    if expected_size <= 0 {
        return Err(CustomError::internal("上传记录的预登记大小无效"));
    }
    if object.hash != client_hash {
        return Err(CustomError::upload_content_rejected(
            "七牛对象 hash 与客户端上传响应不一致",
        ));
    }
    if object.size != expected_size as u64 {
        return Err(CustomError::upload_content_rejected(format!(
            "七牛对象大小 {} 与预登记大小 {} 不一致",
            object.size, expected_size
        )));
    }
    Ok(())
}

async fn fetch_upload_record(
    db: &sqlx::Pool<sqlx::Postgres>,
    file_key: &str,
) -> Result<Option<UploadRecord>, CustomError> {
    Ok(sqlx::query_as::<_, UploadRecord>(
        r#"
        SELECT user_id, status, size, qiniu_hash, cdn_url,
               content_check_status::text AS content_check_status,
               business_ref_type::text AS business_ref_type, business_ref_id
        FROM upload_files
        WHERE file_key = $1
        "#,
    )
    .bind(file_key)
    .fetch_optional(db)
    .await?)
}

fn validate_business_ref_type(
    record: &UploadRecord,
    requested: Option<&BusinessRefType>,
) -> Result<String, CustomError> {
    let registered = record
        .business_ref_type
        .as_deref()
        .ok_or_else(|| CustomError::internal("上传记录缺少预登记业务类型"))?;
    if let Some(requested) = requested {
        if requested.as_str() != registered {
            return Err(CustomError::bad_request(
                "确认上传的业务类型与预登记类型不一致",
            ));
        }
    }
    Ok(registered.to_string())
}

fn active_confirm_response(
    record: &UploadRecord,
    input: &ConfirmUploadRequest,
) -> Result<ConfirmUploadResponse, CustomError> {
    if record.status != "ACTIVE" {
        return Err(CustomError::idempotency_conflict(format!(
            "上传记录状态 {} 不允许确认",
            record.status
        )));
    }
    if record.qiniu_hash.as_deref() != Some(input.hash.as_str()) {
        return Err(CustomError::idempotency_conflict(
            "该文件已使用不同 hash 完成确认",
        ));
    }
    if input.business_ref_id.is_some() && record.business_ref_id != input.business_ref_id {
        return Err(CustomError::idempotency_conflict(
            "该文件已绑定不同业务记录",
        ));
    }
    let cdn_url = record
        .cdn_url
        .clone()
        .ok_or_else(|| CustomError::internal("已激活上传记录缺少 CDN URL"))?;

    Ok(ConfirmUploadResponse {
        file_key: input.file_key.clone(),
        cdn_url,
        content_check_status: record.content_check_status.clone(),
    })
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
    let upload_token =
        build_upload_token(&state.qiniu, &file_key, &input.content_type, input.size)?;

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

    let expires_at =
        chrono::Utc::now() + chrono::Duration::seconds(state.qiniu.token_expire_secs as i64);

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

    let expires_at =
        chrono::Utc::now() + chrono::Duration::seconds(state.qiniu.token_expire_secs as i64);

    let mut items = Vec::with_capacity(input.files.len());
    for file in &input.files {
        validate_mime(&file.content_type)?;
        validate_size(file.size)?;
        let file_key = generate_file_key(&file.business_ref_type, &file.filename);
        let upload_token =
            build_upload_token(&state.qiniu, &file_key, &file.content_type, file.size)?;
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

    Ok(ApiResponse::success(UploadTokensResponse {
        uploads: items,
    }))
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

    if input.hash.trim().is_empty() {
        return Err(CustomError::bad_request("七牛 hash 不能为空"));
    }

    let record = fetch_upload_record(db, &input.file_key)
        .await?
        .ok_or_else(|| {
            CustomError::upload_file_not_found(format!("file_key {} 不存在", input.file_key))
        })?;

    if record.user_id != token.user_id {
        return Err(CustomError::upload_permission_denied(
            "无权操作该文件".to_string(),
        ));
    }

    if record.status != "PENDING" && record.status != "ACTIVE" {
        return Err(CustomError::idempotency_conflict(format!(
            "上传记录状态 {} 不允许确认",
            record.status
        )));
    }

    let expected_size = record
        .size
        .ok_or_else(|| CustomError::internal("上传记录缺少预登记大小"))?;
    let business_ref_type = validate_business_ref_type(&record, input.business_ref_type.as_ref())?;
    let object = stat_qiniu_object(&state.qiniu, &input.file_key).await?;
    validate_qiniu_object(&input.hash, expected_size, &object)?;

    // ACTIVE 只作为相同参数重试的幂等成功，不再执行状态转换。
    if record.status == "ACTIVE" {
        return Ok(ApiResponse::success(active_confirm_response(
            &record, &input,
        )?));
    }

    let cdn_url = build_cdn_url(&state.qiniu, &input.file_key);
    let result = sqlx::query(
        r#"
        UPDATE upload_files
        SET status = 'ACTIVE',
            qiniu_hash = $3,
            cdn_url = $4,
            business_ref_type = $5::upload_business_ref_enum,
            business_ref_id = $6,
            updated_at = NOW()
        WHERE file_key = $1 AND user_id = $2 AND status = 'PENDING'
        "#,
    )
    .bind(&input.file_key)
    .bind(token.user_id)
    .bind(&object.hash)
    .bind(&cdn_url)
    .bind(&business_ref_type)
    .bind(input.business_ref_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        // 并发的相同确认可能已经完成；仅在落库值一致时按幂等成功返回。
        let current = fetch_upload_record(db, &input.file_key)
            .await?
            .ok_or_else(|| {
                CustomError::upload_file_not_found(format!("file_key {} 不存在", input.file_key))
            })?;
        if current.user_id != token.user_id {
            return Err(CustomError::upload_permission_denied(
                "无权操作该文件".to_string(),
            ));
        }
        validate_business_ref_type(&current, input.business_ref_type.as_ref())?;
        return Ok(ApiResponse::success(active_confirm_response(
            &current, &input,
        )?));
    }

    Ok(ApiResponse::success(ConfirmUploadResponse {
        file_key: input.file_key,
        cdn_url,
        content_check_status: "PENDING".to_string(),
    }))
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

    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT user_id, status FROM upload_files WHERE file_key = $1")
            .bind(&key)
            .fetch_optional(db)
            .await?;

    let (owner_id, _status) =
        row.ok_or_else(|| CustomError::upload_file_not_found(format!("file_key {} 不存在", key)))?;

    if owner_id != token.user_id {
        return Err(CustomError::upload_permission_denied(
            "非文件所有者".to_string(),
        ));
    }

    // 软删除记录（实际生产应调用七牛 delete API）
    sqlx::query(
        r#"
        UPDATE upload_files
        SET status = 'DELETED', deleted_at = NOW(), updated_at = NOW()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qiniu_object_metadata_must_match_client_hash() {
        let object = QiniuObjectMetadata {
            hash: "server-hash".to_string(),
            size: 42,
        };
        assert!(matches!(
            validate_qiniu_object("client-hash", 42, &object),
            Err(CustomError::UploadContentRejected(_))
        ));
    }

    #[test]
    fn qiniu_object_metadata_must_match_registered_size() {
        let object = QiniuObjectMetadata {
            hash: "same-hash".to_string(),
            size: 43,
        };
        assert!(matches!(
            validate_qiniu_object("same-hash", 42, &object),
            Err(CustomError::UploadContentRejected(_))
        ));
    }

    #[test]
    fn qiniu_hash_cannot_be_empty() {
        let object = QiniuObjectMetadata {
            hash: "server-hash".to_string(),
            size: 42,
        };
        assert!(matches!(
            validate_qiniu_object("  ", 42, &object),
            Err(CustomError::BadRequest(_))
        ));
    }
}
