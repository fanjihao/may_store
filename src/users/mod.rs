pub mod new;
pub mod view;
pub mod update;
pub mod invitation;
pub mod role;
pub mod group_update;

use argon2::{ Argon2, PasswordHash, PasswordHasher, PasswordVerifier };
use password_hash::{ SaltString };
use rand::thread_rng;

use crate::{ errors::CustomError, utils::{ APP_ID, APP_SECRET } };

pub fn hash_password(plain: &str) -> Result<(String, String), String> {
    let salt = SaltString::generate(&mut thread_rng());
    let argon = Argon2::default();
    let hash = argon
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| e.to_string())?
        .to_string();
    Ok((hash, "argon2id".to_string()))
}

pub fn verify_password(plain: &str, stored_hash: &str) -> Result<bool, String> {
    let parsed = PasswordHash::new(stored_hash).map_err(|e| e.to_string())?;
    let argon = Argon2::default();
    Ok(argon.verify_password(plain.as_bytes(), &parsed).is_ok())
}

// 微信登录
pub async fn weixin_login(code: &str) -> Result<String, CustomError> {
    let res = reqwest
        ::get(
            "https://api.weixin.qq.com/sns/jscode2session?grant_type=authorization_code&appid=".to_string() +
                APP_ID +
                "&secret=" +
                APP_SECRET +
				"&js_code=" +
				code,
        ).await?
        .text().await?;
	
    let response_json: Result<serde_json::Value, serde_json::Error> = serde_json::from_str(&res);
    match response_json {
        Ok(obj) => {
            let mut openid = "";
            if let Some(val) = obj.get("openid") {
                if let Some(t) = val.as_str() {
                    openid = t;
                }
            }
            println!("openid : {}", openid);
            Ok(openid.to_string())
        }
        Err(_) => Err(CustomError::BadRequest("openid 获取失败".to_string())),
    }
}
