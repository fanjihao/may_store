use super::service::UploadService;
use crate::{errors::CustomError, middlewares::auth::UserToken};

#[utoipa::path(
    get,
    path = "/upload-token",
    tag = "上传",
    summary = "获取七牛云上传凭证",
    responses(
        (status = 200, body = String, description = "上传凭证"),
        (status = 400, body = CustomError),
        (status = 401, body = CustomError)
    ),
    security(("cookie_auth" = []))
)]
pub async fn get_qiniu_token(_: UserToken) -> Result<String, CustomError> {
    UploadService::generate_qiniu_token().await
}
