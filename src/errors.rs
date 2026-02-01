use std::fmt;

use log::error as log_error;
use ntex::{
    http::{error, StatusCode},
    web::{HttpResponse, WebResponseError, DefaultError},
};
use qiniu_upload_token::ToStringError;
use redis::RedisError;
use serde::Serialize;
use tokio::task::JoinError;
use utoipa::ToSchema;

// ============ Error Enum Definition ============

/// Custom error types for the application
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "type", content = "message")]
pub enum CustomError {
    /// 400 Bad Request - 参数或业务验证失败
    #[serde(rename = "bad_request")]
    BadRequest(String),

    /// 401 Unauthorized - 未登录或登录过期
    #[serde(rename = "unauthorized")]
    Unauthorized(String),

    /// 403 Forbidden - 无权限访问
    #[serde(rename = "forbidden")]
    Forbidden(String),

    /// 404 Not Found - 资源不存在
    #[serde(rename = "not_found")]
    NotFound(String),

    /// 409 Conflict - 数据冲突（如重复添加）
    #[serde(rename = "conflict")]
    Conflict(String),

    /// 500 Internal Server Error - 服务器内部错误
    #[serde(rename = "internal_error")]
    InternalServerError(String),
}

impl CustomError {
    // ============ Constructor Methods ============

    /// Create a bad request error
    pub fn bad_request<S: Into<String>>(msg: S) -> Self {
        Self::BadRequest(msg.into())
    }

    /// Create an unauthorized error
    pub fn unauthorized<S: Into<String>>(msg: S) -> Self {
        Self::Unauthorized(msg.into())
    }

    /// Create a forbidden error
    pub fn forbidden<S: Into<String>>(msg: S) -> Self {
        Self::Forbidden(msg.into())
    }

    /// Create a not found error
    pub fn not_found<S: Into<String>>(msg: S) -> Self {
        Self::NotFound(msg.into())
    }

    /// Create a conflict error (e.g., duplicate data)
    pub fn conflict<S: Into<String>>(msg: S) -> Self {
        Self::Conflict(msg.into())
    }

    /// Create an internal server error
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::InternalServerError(msg.into())
    }

    // ============ Status Code ============

    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// ============ WebResponseError Trait ============

impl WebResponseError for CustomError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self, _: &ntex::web::HttpRequest) -> HttpResponse {
        #[derive(Serialize)]
        struct ErrorBody {
            code: u16,
            message: String,
        }

        let status = self.status_code();
        let message = match self {
            Self::BadRequest(msg) => msg.clone(),
            Self::Unauthorized(msg) => msg.clone(),
            Self::Forbidden(msg) => msg.clone(),
            Self::NotFound(msg) => msg.clone(),
            Self::Conflict(msg) => msg.clone(),
            Self::InternalServerError(msg) => msg.clone(),
        };

        let body = ErrorBody {
            code: status.as_u16(),
            message,
        };

        HttpResponse::build(status)
            .content_type("application/json; charset=utf-8")
            .json(&body)
    }
}

// ============ Display Trait ============

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "{msg}"),
            Self::Unauthorized(msg) => write!(f, "{msg}"),
            Self::Forbidden(msg) => write!(f, "{msg}"),
            Self::NotFound(msg) => write!(f, "{msg}"),
            Self::Conflict(msg) => write!(f, "{msg}"),
            Self::InternalServerError(msg) => write!(f, "{msg}"),
        }
    }
}

// ============ From Implementations ============

// --- Common Types ---

impl From<&str> for CustomError {
    fn from(s: &str) -> Self {
        Self::bad_request(s)
    }
}

impl From<String> for CustomError {
    fn from(s: String) -> Self {
        Self::bad_request(s)
    }
}

impl From<std::io::Error> for CustomError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(format!("IO错误: {}", e))
    }
}

impl From<std::num::ParseIntError> for CustomError {
    fn from(e: std::num::ParseIntError) -> Self {
        Self::bad_request(format!("格式转换异常: {e}"))
    }
}

impl From<error::PayloadError> for CustomError {
    fn from(e: error::PayloadError) -> Self {
        Self::bad_request(format!("请求体解析错误: {e}"))
    }
}

impl From<DefaultError> for CustomError {
    fn from(e: DefaultError) -> Self {
        Self::bad_request(format!("参数错误: {e:?}"))
    }
}

// --- Async Tasks ---

impl From<JoinError> for CustomError {
    fn from(e: JoinError) -> Self {
        Self::internal(format!("异步任务执行失败: {e:?}"))
    }
}

// --- External Services ---

impl From<reqwest::Error> for CustomError {
    fn from(e: reqwest::Error) -> Self {
        log_error!(target: "reqwest", "reqwest error: {e:?}");
        if e.is_timeout() {
            Self::internal("请求超时".to_string())
        } else if e.is_connect() {
            Self::bad_request("无法连接到远程服务".to_string())
        } else {
            Self::bad_request(e.to_string())
        }
    }
}

impl From<RedisError> for CustomError {
    fn from(e: RedisError) -> Self {
        Self::bad_request(format!("Redis错误: {e}"))
    }
}

impl From<ToStringError> for CustomError {
    fn from(e: ToStringError) -> Self {
        Self::internal(format!("七牛云Token转换失败: {e:?}"))
    }
}

// --- JSON/Serialization ---

impl From<serde_json::Error> for CustomError {
    fn from(e: serde_json::Error) -> Self {
        Self::bad_request(format!("JSON解析失败: {e}"))
    }
}

// --- ID Generator ---

impl From<idgenerator::error::OptionError> for CustomError {
    fn from(e: idgenerator::error::OptionError) -> Self {
        Self::internal(format!("ID生成器错误: {e:?}"))
    }
}

// --- WebSocket ---

impl From<ntex::ws::error::HandshakeError> for CustomError {
    fn from(e: ntex::ws::error::HandshakeError) -> Self {
        Self::bad_request(format!("WebSocket握手失败: {e:?}"))
    }
}

// --- SQLx / Database ---

impl From<sqlx::Error> for CustomError {
    fn from(e: sqlx::Error) -> Self {

        // Handle database errors
        if let Some(db_err) = e.as_database_error() {
            let code = db_err.code();
            let message = db_err.message();

            match code {
                // PostgreSQL unique constraint violation
                Some(cow) if cow == "23505" => Self::conflict("数据已存在，请勿重复添加"),
                // PostgreSQL foreign key violation
                Some(cow) if cow == "23503" => Self::bad_request(format!("关联数据不存在: {message}")),
                // PostgreSQL not null violation
                Some(cow) if cow == "23502" => Self::bad_request(format!("必填字段不能为空: {message}")),
                _ => {
                    log::debug!("Unhandled database error: {code:?} - {message}");
                    println!("Unhandled database error: {code:?} - {message}");
                    Self::internal("数据库操作失败")
                }
            }
        } else {
            match e {
                sqlx::Error::RowNotFound => Self::not_found("找不到对应数据"),
                sqlx::Error::ColumnNotFound(col) => {
                    Self::bad_request(format!("查询字段不存在: {col}"))
                }
                sqlx::Error::Decode(err) => {
                    Self::bad_request(format!("数据解码失败: {err}"))
                }
                sqlx::Error::Database(_) => Self::internal("数据库错误"),
                sqlx::Error::Io(_) => Self::internal("数据库IO错误"),
                sqlx::Error::Tls(_) => Self::internal("数据库TLS错误"),
                sqlx::Error::Protocol(_) => Self::internal("数据库协议错误"),
                _ => {
                    log::debug!("Unhandled sqlx error: {e:?}");
                    println!("Unhandled sqlx error: {e:?}");
                    Self::internal("数据库操作失败")
                }
            }
        }
    }
}
