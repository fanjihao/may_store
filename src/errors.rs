use std::fmt;
use log::error as logError;
use ntex::{
    http::{error, StatusCode},
    web::{HttpResponse, WebResponseError, DefaultError},
};
use qiniu_upload_token::ToStringError;
use serde::Serialize;
use tokio::task::JoinError;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub enum CustomError {
    NotFound(String),
    InternalServerError(String),
    BadRequest(String),
    AuthFailed(String),
}

impl WebResponseError for CustomError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::AuthFailed(_) => StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self, _: &ntex::web::HttpRequest) -> HttpResponse {
        HttpResponse::new(self.status_code()).set_body(
            match self {
                Self::NotFound(e) => {
                    println!("{:?}", e);
                    e
                },
                Self::InternalServerError(e) => e,
                Self::BadRequest(e) => e,
                Self::AuthFailed(e) => e,
            }
            .into(),
        )
    }
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomError::NotFound(e) => write!(f, "{e}"),
            CustomError::BadRequest(e) => write!(f, "{e}"),
            CustomError::AuthFailed(e) => write!(f, "{e}"),
            CustomError::InternalServerError(e) => write!(f, "{e}"),
        }
    }
}

impl From<sqlx::Error> for CustomError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::NotFound("找不到对应数据".into()),
            err => {
                println!("sql 错误: {:?}", err);
                Self::InternalServerError("服务器内部发生错误,请联系管理员".into())
            },
        }
    }
}

impl From<std::io::Error> for CustomError {
    fn from(e: std::io::Error) -> Self {
        CustomError::InternalServerError(e.to_string())
    }
}

impl From<error::PayloadError> for CustomError {
    fn from(e: error::PayloadError) -> Self {
        CustomError::InternalServerError(e.to_string())
    }
}

impl From<std::num::ParseIntError> for CustomError {
    fn from(value: std::num::ParseIntError) -> Self {
        CustomError::BadRequest(format!("格式转换异常: {}", value.to_string()))
    }
}

impl From<reqwest::Error> for CustomError {
    fn from(e: reqwest::Error) -> Self {
        logError!(target: "reqwest", "reqwest error: {:?}", e);
        CustomError::BadRequest(e.to_string())
    }
}

impl From<DefaultError> for CustomError {
    fn from(value: DefaultError) -> Self {
        CustomError::BadRequest(format!("Parameter Error: {:#?}", value))
    }
}

impl From<JoinError> for CustomError {
    fn from(value: JoinError) -> Self {
        CustomError::InternalServerError(format!("tokio 线程错误: {:#?}", value))
    }
}

impl From<idgenerator::error::OptionError> for CustomError {
    fn from(value: idgenerator::error::OptionError) -> Self {
        CustomError::BadRequest(format!("id生成器初始化失败: {:#?}", value))
    }
}

impl From<serde_json::Error> for CustomError {
    fn from(value: serde_json::Error) -> Self {
        CustomError::BadRequest(format!("json 数据解析失败: {:#?}", value))
    }
}

impl From<ToStringError> for CustomError {
    fn from(value: ToStringError) -> Self {
        CustomError::BadRequest(format!("七牛云转str失败: {:#?}", value))
    }
}