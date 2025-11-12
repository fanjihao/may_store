use qiniu_upload_manager::apis::credential::Credential;
use qiniu_upload_token::{FileType, UploadPolicy, UploadTokenProvider};
use std::time::Duration;

use crate::{
    errors::CustomError,
    models::users::UserToken,
    utils::{ACCESS_KEY, BUCKET_NAME, SECRET_KEY},
};

pub async fn get_qiniu_token(_: UserToken) -> Result<String, CustomError> {
    let access_key = ACCESS_KEY;
    let secret_key = SECRET_KEY;
    let upload_policy = UploadPolicy::new_for_bucket(BUCKET_NAME, Duration::from_secs(3600))
        .file_type(FileType::InfrequentAccess)
        .build();
    let credential = Credential::new(access_key, secret_key);
    let provider = upload_policy.into_dynamic_upload_token_provider(credential);
    let token_string = provider.async_to_token_string(Default::default()).await?;

    Ok(token_string.to_string())
}
