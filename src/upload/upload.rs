use ntex::web::{Responder,HttpResponse};

use crate::errors::CustomError;


pub async fn upload_file() -> Result<impl Responder, CustomError> {
    Ok(HttpResponse::Created().body("注册成功.".to_string()))
}