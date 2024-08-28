use std::{sync::Arc, time::Duration};

use ntex::{
    util::{stream_recv, BytesMut},
    web::{
        types::{Json, Payload, State},
        HttpRequest, Responder,
    },
};
use qiniu_upload_manager::{
    apis::credential::Credential, AutoUploader, AutoUploaderObjectParams, UploadManager,
    UploadTokenSigner,
};
use qiniu_upload_token::{FileType, UploadPolicy, UploadTokenProvider};
use tokio::fs;

use crate::{
    errors::CustomError,
    models::users::UserToken,
    utils::{ACCESS_KEY, BUCKET_NAME, DOMAIN_NAME, SECRET_KEY},
    AppState,
};

pub async fn upload_file(
    mut payload: Payload,
    state: State<Arc<AppState>>,
    req: HttpRequest,
) -> Result<impl Responder, CustomError> {
    let db_pool = &state.clone().db_pool;
    let mut bytes: BytesMut = BytesMut::new();

    let content_disposition = req
        .headers()
        .get("Content-Disposition")
        .and_then(|cd| cd.to_str().ok())
        .and_then(|cd| cd.split(';').find(|s| s.trim().starts_with("filename=")))
        .and_then(|s| s.split('=').nth(1))
        .map(|filename| filename.trim_matches('"').to_string());

    println!("upload: {:?}", content_disposition);
    let filename = match content_disposition {
        Some(str) => str,
        None => "".to_string(),
    };
    // payload 传入的是一个连续的stream
    while let Some(item) = stream_recv(&mut payload).await {
        bytes.extend_from_slice(&item?);
    }

    let new_file = fs::write(format!("static/images/{}", filename), &bytes).await;

    let access_key = ACCESS_KEY;
    let secret_key = SECRET_KEY;
    let bucket_name = BUCKET_NAME;
    let object_name = &filename;
    let credential = Credential::new(access_key, secret_key);
    let upload_manager = UploadManager::builder(UploadTokenSigner::new_credential_provider(
        credential,
        bucket_name,
        Duration::from_secs(3600),
    ))
    .build();
    let uploader: AutoUploader = upload_manager.auto_uploader();
    let params = AutoUploaderObjectParams::builder()
        .object_name(object_name)
        .file_name(object_name)
        .build();
    uploader
        .upload_path(format!("static/images/{}", &filename), params) //读取本地文件上传
        // .upload_reader(Cursor::new(bytes), params)
        .unwrap();
    match new_file {
        Ok(_) => {
            // 删除
            fs::remove_file(format!("static/images/{}", &filename))
                .await
                .expect("");
            let url: String = format!("{}/{}", DOMAIN_NAME, object_name);

            let img_row = sqlx::query!("INSERT INTO images (url) VALUES ($1) RETURNING id", url)
                .fetch_one(db_pool)
                .await?;

            // 返回上传之后外链地址
            Ok(Json((img_row.id, url)))
        }
        Err(_) => Err(CustomError::BadRequest("upload err".to_string())),
    }
}

pub async fn get_qiniu_token(_: UserToken) -> Result<String, CustomError> {
    let access_key = ACCESS_KEY;
    let secret_key = SECRET_KEY;
    let upload_policy =
        UploadPolicy::new_for_bucket(BUCKET_NAME, Duration::from_secs(3600))
            .file_type(FileType::InfrequentAccess)
            .build();
    let credential = Credential::new(access_key, secret_key);
    let provider = upload_policy.into_dynamic_upload_token_provider(credential);
    let token_string = provider.async_to_token_string(Default::default()).await?;

    println!("upload_token: {:?}", token_string);
    Ok("()".to_string())
}
