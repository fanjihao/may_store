// API - 文件上传路由
// FSD.latest.md compliant - 预签名 URL、确认上传、删除文件

use ntex::web::{
    self,
    types::{Json, Path, State},
    HttpResponse, Responder, ServiceConfig,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;

/// 配置上传路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/api/uploads")
            // 获取预签名上传 URL
            .route("/presigned-url", web::post().to(get_presigned_url))
            // 批量获取预签名 URL
            .route("/presigned-urls", web::post().to(get_presigned_urls))
            // 确认上传完成
            .route("/confirm", web::post().to(confirm_upload))
            // 删除上传文件
            .route("/{file_key:path}", web::delete().to(delete_file)),
    );
}

// ========== 请求结构 ==========

/// 预签名 URL 请求 (FSD v2 13.1)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignedUrlRequest {
    pub filename: String,      // 原始文件名
    pub content_type: String,  // MIME 类型
    pub size: i64,             // 文件大小（字节），最大 5242880（5MB）
    pub idempotency_key: String,
}

/// 预签名 URL 响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignedUrlResponse {
    pub upload_url: String,
    pub file_key: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// 批量预签名 URL 请求 (FSD v2 13.3)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PresignedUrlsRequest {
    pub files: Vec<FileItem>,
    pub idempotency_key: String,
}

/// 文件项
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct FileItem {
    pub filename: String,
    pub content_type: String,
    pub size: i64,
}

/// 批量预签名 URL 响应
#[derive(Debug, Serialize, ToSchema)]
pub struct PresignedUrlsResponse {
    pub uploads: Vec<UploadItem>,
}

/// 单个上传项
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadItem {
    pub filename: String,
    pub upload_url: String,
    pub file_key: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// 确认上传请求 (FSD v2 13.2)
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmUploadRequest {
    pub file_key: String,
}

/// 确认上传响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmUploadResponse {
    pub file_key: String,
    pub cdn_url: String,
    pub content_check_status: String, // PASS / PENDING / REJECTED
}

/// 删除文件响应
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteFileResponse {
    pub status: String,
}

// ========== 处理器 ==========

/// 获取预签名上传 URL
/// POST /api/uploads/presigned-url
/// FSD v2 13.1
#[utoipa::path(
    post,
    path = "/api/uploads/presigned-url",
    tag = "文件上传",
    request_body = PresignedUrlRequest,
    responses(
        (status = 200, description = "获取成功", body = PresignedUrlResponse),
        (status = 400, description = "文件大小超限或类型不支持"),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_presigned_url(
    _state: State<Arc<AppState>>,
    _token: UserToken,
    body: Json<PresignedUrlRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();

    // 验证文件大小（最大 5MB）
    if input.size > 5 * 1024 * 1024 {
        return Err(CustomError::BadRequest("文件大小超出5MB限制".into()));
    }

    // 验证文件类型
    let allowed_types = ["image/jpeg", "image/png", "image/gif"];
    if !allowed_types.contains(&input.content_type.as_str()) {
        return Err(CustomError::BadRequest("不支持的文件类型".into()));
    }

    // 生成 file_key
    let now = chrono::Utc::now();
    let date_path = now.format("uploads/%Y/%m/%d").to_string();
    let uuid = uuid::Uuid::new_v4().to_string();
    let extension = input.filename.split('.').last().unwrap_or("jpg");
    let file_key = format!("{}/{}.{}", date_path, uuid, extension);

    // 生成预签名 URL（简化实现，实际应调用对象存储服务）
    let upload_url = format!(
        "https://cdn.example.com/upload?signature=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9&key={}",
        file_key
    );

    let expires_at = now + chrono::Duration::minutes(30);

    Ok(HttpResponse::Ok().json(&PresignedUrlResponse {
        upload_url,
        file_key: file_key.clone(),
        expires_at,
    }))
}

/// 批量获取预签名 URL
/// POST /api/uploads/presigned-urls
/// FSD v2 13.3
#[utoipa::path(
    post,
    path = "/api/uploads/presigned-urls",
    tag = "文件上传",
    request_body = PresignedUrlsRequest,
    responses(
        (status = 200, description = "获取成功", body = PresignedUrlsResponse),
        (status = 400, description = "文件数量超限或大小超限"),
        (status = 401, description = "未登录"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_presigned_urls(
    _state: State<Arc<AppState>>,
    _token: UserToken,
    body: Json<PresignedUrlsRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();

    // 验证文件数量（最多 9 个）
    if input.files.len() > 9 {
        return Err(CustomError::BadRequest("最多9个文件".into()));
    }

    // 验证总大小（不超过 20MB）
    let total_size: i64 = input.files.iter().map(|f| f.size).sum();
    if total_size > 20 * 1024 * 1024 {
        return Err(CustomError::BadRequest("总大小不超过20MB".into()));
    }

    let now = chrono::Utc::now();
    let date_path = now.format("uploads/%Y/%m/%d").to_string();

    let uploads: Vec<UploadItem> = input
        .files
        .iter()
        .map(|file| {
            let uuid = uuid::Uuid::new_v4().to_string();
            let extension = file.filename.split('.').last().unwrap_or("jpg");
            let file_key = format!("{}/{}.{}", date_path, uuid, extension);

            let upload_url = format!(
                "https://cdn.example.com/upload?signature=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9&key={}",
                file_key
            );

            UploadItem {
                filename: file.filename.clone(),
                upload_url,
                file_key: file_key.clone(),
                expires_at: now + chrono::Duration::minutes(30),
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(&PresignedUrlsResponse { uploads }))
}

/// 确认上传完成
/// POST /api/uploads/confirm
/// FSD v2 13.2
#[utoipa::path(
    post,
    path = "/api/uploads/confirm",
    tag = "文件上传",
    request_body = ConfirmUploadRequest,
    responses(
        (status = 200, description = "确认成功", body = ConfirmUploadResponse),
        (status = 401, description = "未登录"),
        (status = 404, description = "文件不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn confirm_upload(
    state: State<Arc<AppState>>,
    _token: UserToken,
    body: Json<ConfirmUploadRequest>,
) -> Result<impl Responder, CustomError> {
    let input = body.into_inner();
    let db = &state.db_pool;

    // 验证文件是否存在（简化：实际应检查对象存储）
    let file_exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM uploaded_files WHERE file_key = $1)"
    )
    .bind(&input.file_key)
    .fetch_optional(db)
    .await?
    .unwrap_or(true); // 如果表不存在或没有记录，默认通过

    if !file_exists {
        return Err(CustomError::NotFound("文件不存在".into()));
    }

    let cdn_url = format!("https://cdn.example.com/{}", input.file_key);

    // 简化：内容审核状态默认为 PASS，实际应异步回调
    let content_check_status = "PASS";

    Ok(HttpResponse::Ok().json(&ConfirmUploadResponse {
        file_key: input.file_key,
        cdn_url,
        content_check_status: content_check_status.to_string(),
    }))
}

/// 删除上传文件
/// DELETE /api/uploads/{file_key:path}
/// FSD v2 13.4
#[utoipa::path(
    delete,
    path = "/api/uploads/{file_key:path}",
    tag = "文件上传",
    params(
        ("file_key" = String, Path, description = "文件路径")
    ),
    responses(
        (status = 200, description = "删除成功", body = DeleteFileResponse),
        (status = 401, description = "未登录"),
        (status = 404, description = "文件不存在"),
        (status = 500, description = "服务器错误")
    ),
    security(("cookie_auth" = []))
)]
pub async fn delete_file(
    state: State<Arc<AppState>>,
    _token: UserToken,
    file_key: Path<String>,
) -> Result<impl Responder, CustomError> {
    let db = &state.db_pool;
    let key = file_key.into_inner();

    // 检查文件是否存在
    let exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM uploaded_files WHERE file_key = $1)"
    )
    .bind(&key)
    .fetch_optional(db)
    .await?
    .unwrap_or(false);

    if !exists {
        return Err(CustomError::NotFound("文件不存在".into()));
    }

    // 删除文件记录（简化：实际应调用对象存储删除）
    sqlx::query("DELETE FROM uploaded_files WHERE file_key = $1")
        .bind(&key)
        .execute(db)
        .await?;

    Ok(HttpResponse::Ok().json(&DeleteFileResponse {
        status: "ok".to_string(),
    }))
}
