use std::io::Write;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use flate2::{write::ZlibEncoder, Compression};
use hmac::{Hmac, Mac};
use ntex::web::types::State;
use ntex::web::{HttpResponse, Responder};
use serde_json::json;
use sha2::Sha256;

use crate::errors::CustomError;
use crate::models::game_im::ImUserSigOut;
use crate::models::users::UserToken;
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

fn usersig_encode_urlsafe(b64: String) -> String {
    b64.replace('+', "*").replace('/', "-").replace('=', "_")
}

pub fn generate_user_sig(
    identifier: &str,
    sdk_app_id: u64,
    secret_key: &str,
    expire_seconds: u64,
    now_seconds: i64,
) -> Result<String, CustomError> {
    let content = format!(
		"TLS.identifier:{identifier}\nTLS.sdkappid:{sdk_app_id}\nTLS.time:{now_seconds}\nTLS.expire:{expire_seconds}\n",
	);

    let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
        .map_err(|_| CustomError::InternalServerError("Invalid IM secret key".into()))?;
    mac.update(content.as_bytes());
    let sig = STANDARD.encode(mac.finalize().into_bytes());

    let payload = json!({
        "TLS.ver": "2.0",
        "TLS.identifier": identifier,
        "TLS.sdkappid": sdk_app_id,
        "TLS.expire": expire_seconds,
        "TLS.time": now_seconds,
        "TLS.sig": sig,
    });

    let payload_str = serde_json::to_string(&payload)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload_str.as_bytes())?;
    let compressed = encoder.finish()?;

    let b64 = STANDARD.encode(compressed);
    Ok(usersig_encode_urlsafe(b64))
}

#[utoipa::path(
	get,
	path = "/im/usersig",
	tag = "IM",
	summary = "获取当前登录用户的腾讯云 IM UserSig",
	responses(
		(status = 200, body = ImUserSigOut),
		(status = 400, body = CustomError),
		(status = 401, body = CustomError)
	),
	security(("cookie_auth" = []))
)]
pub async fn get_user_sig(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let Some(cfg) = state.im_config.clone() else {
        return Err(CustomError::BadRequest(
            "IM 未配置：请设置 TENCENT_IM_SDK_APP_ID / TENCENT_IM_SECRET_KEY".into(),
        ));
    };

    let now = Utc::now().timestamp();
    let identifier = token.user_id.to_string();
    let user_sig = generate_user_sig(
        &identifier,
        cfg.sdk_app_id,
        &cfg.secret_key,
        cfg.expire_seconds,
        now,
    )?;
    let expire_at = now + cfg.expire_seconds as i64;

    Ok(HttpResponse::Ok().json(&ImUserSigOut {
        sdk_app_id: cfg.sdk_app_id,
        identifier,
        user_id: token.user_id,
        user_sig,
        expire_at,
    }))
}
