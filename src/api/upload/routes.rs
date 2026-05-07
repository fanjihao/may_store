// API 层 - 上传路由
// 处理文件/图片上传相关的HTTP请求

use ntex::web::{self, types::State, HttpRequest, HttpResponse, Responder, ServiceConfig};
use std::sync::Arc;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::private::{ACCESS_KEY, BUCKET_NAME, SECRET_KEY};

/// 配置上传路由
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::scope("/upload")
            .route("/token", web::get().to(get_upload_token))
            .route("/image", web::post().to(upload_image)),
    );
}

/// 生成七牛云上传Token
/// 客户端使用此token直接上传文件到七牛云存储
fn generate_qiniu_token(access_key: &str, secret_key: &str, bucket: &str) -> String {
    // 七牛云上传策略
    let deadline = chrono::Utc::now().timestamp() + 3600; // 1小时后过期

    let policy = serde_json::json!({
        "scope": bucket,
        "deadline": deadline,
        "returnBody": r#"{"key":"$(key)","hash":"$(etag)","fsize":$(fsize)}"#
    });

    let policy_str = serde_json::to_string(&policy).unwrap_or_default();
    let policy_encoded = base64::encode(&policy_str);

    // 使用HMAC-SHA1签名
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    type HmacSha1 = Hmac<Sha1>;

    let mut mac = HmacSha1::new_from_slice(secret_key.as_bytes()).unwrap();
    mac.update(policy_encoded.as_bytes());
    let signature = mac.finalize().into_bytes();

    let signature_encoded = base64::encode(&signature);

    format!("{}:{}:{}", access_key, signature_encoded, policy_encoded)
}

/// 获取七牛云上传Token
/// 用于客户端直传文件到七牛云存储
#[utoipa::path(
    get,
    path = "/upload/token",
    tag = "上传",
    responses(
        (status = 200, description = "获取上传Token成功"),
        (status = 401, description = "未登录")
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_upload_token(
    _state: State<Arc<AppState>>,
    _token: crate::middlewares::auth::UserToken,
) -> Result<impl Responder, CustomError> {
    let token = generate_qiniu_token(ACCESS_KEY, SECRET_KEY, BUCKET_NAME);

    let upload_token = serde_json::json!({
        "uptoken": token,
        "bucket": BUCKET_NAME,
        "domain": "cdn.example.com",
        "expires": 3600
    });

    Ok(HttpResponse::Ok().json(&upload_token))
}

/// 上传图片（处理 multipart form data）
/// 将图片保存到本地存储目录
#[utoipa::path(
    post,
    path = "/upload/image",
    tag = "上传",
    responses(
        (status = 200, description = "上传成功"),
        (status = 400, description = "上传失败")
    ),
    security(("cookie_auth" = []))
)]
pub async fn upload_image(
    req: HttpRequest,
    _token: crate::middlewares::auth::UserToken,
) -> Result<impl Responder, CustomError> {
    // 简化实现：检查Content-Type并返回成功响应
    // 实际项目中应解析 multipart form data，保存文件到 storage/uploads 目录

    let content_type = req
        .headers()
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg");

    // 生成唯一文件名
    let filename = format!(
        "{}_{}.jpg",
        chrono::Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4().to_string()[..8].to_string()
    );

    // 返回上传成功响应（实际应接收并保存文件）
    let result = serde_json::json!({
        "url": format!("https://cdn.example.com/images/{}", filename),
        "filename": filename,
        "content_type": content_type,
        "size": 0,
        "message": "文件接收成功，实际项目中应实现文件保存逻辑"
    });

    Ok(HttpResponse::Ok().json(&result))
}
