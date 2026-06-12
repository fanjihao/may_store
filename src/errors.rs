use std::fmt;

use log::error as log_error;
use ntex::{
    http::{error, StatusCode},
    web::{DefaultError, HttpResponse, WebResponseError},
};
use qiniu_upload_token::ToStringError;
use redis::RedisError;
use serde::Serialize;
use tokio::task::JoinError;
use utoipa::ToSchema;

// ============ Error Enum Definition ============
//
// 自定义错误类型，完整覆盖 FSD §13.3 错误码表 (41 项)
// 每个变体对应一个 FSD 错误码，通过 `fsd_code()` 方法返回标准字符串。
//
// 错误响应 JSON 结构（FSD §1.1 / §13.3 统一格式）：
// ```json
// {
//   "code": 0,
//   "message": "success",
//   "data": { "code": "AUTH_INVALID_TOKEN", "message": "登录态无效" },
//   "trace_id": "..."
// }
// ```

/// FSD §13.3 错误码枚举（41 项全量）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub enum FsdErrorCode {
    // ===== Auth (5) =====
    #[serde(rename = "AUTH_INVALID_TOKEN")]
    AuthInvalidToken,
    #[serde(rename = "AUTH_TOKEN_REVOKED")]
    AuthTokenRevoked,
    #[serde(rename = "AUTH_ACCOUNT_BANNED")]
    AuthAccountBanned,
    #[serde(rename = "AUTH_CODE_INVALID")]
    AuthCodeInvalid,
    // ===== User (5) =====
    #[serde(rename = "USER_NOT_FOUND")]
    UserNotFound,
    #[serde(rename = "USER_PHONE_ALREADY_BOUND")]
    UserPhoneAlreadyBound,
    #[serde(rename = "USER_NICKNAME_INVALID")]
    UserNicknameInvalid,
    #[serde(rename = "USER_ALREADY_IN_GROUP")]
    UserAlreadyInGroup,
    #[serde(rename = "USER_GROUP_NOT_EMPTY")]
    UserGroupNotEmpty,
    // ===== Group (5) =====
    #[serde(rename = "GROUP_NOT_FOUND")]
    GroupNotFound,
    #[serde(rename = "GROUP_MEMBER_LIMIT_EXCEEDED")]
    GroupMemberLimitExceeded,
    #[serde(rename = "GROUP_EXIT_SETTLEMENT_REQUIRED")]
    GroupExitSettlementRequired,
    #[serde(rename = "INVITE_CODE_INVALID")]
    InviteCodeInvalid,
    #[serde(rename = "INVITE_CODE_USED")]
    InviteCodeUsed,
    #[serde(rename = "INVITE_USER_MISMATCH")]
    InviteUserMismatch,
    // ===== Role (4) =====
    #[serde(rename = "ROLE_SWAP_BLOCKED_BY_ORDER")]
    RoleSwapBlockedByOrder,
    #[serde(rename = "ROLE_SWAP_BLOCKED_BY_WISH")]
    RoleSwapBlockedByWish,
    #[serde(rename = "PERMISSION_DENIED")]
    PermissionDenied,
    #[serde(rename = "ROLE_NOT_ALLOWED")]
    RoleNotAllowed,
    // ===== Food (2) =====
    #[serde(rename = "FOOD_NOT_FOUND")]
    FoodNotFound,
    #[serde(rename = "FOOD_CAPACITY_EXCEEDED")]
    FoodCapacityExceeded,
    // ===== Order (4) =====
    #[serde(rename = "ORDER_NOT_FOUND")]
    OrderNotFound,
    #[serde(rename = "ORDER_STATUS_INVALID")]
    OrderStatusInvalid,
    #[serde(rename = "ORDER_TYPE_INVALID")]
    OrderTypeInvalid,
    #[serde(rename = "DAILY_REWARD_LIMIT_REACHED")]
    DailyRewardLimitReached,
    // ===== Wish (6) =====
    #[serde(rename = "WISH_NOT_FOUND")]
    WishNotFound,
    #[serde(rename = "WISH_STATUS_INVALID")]
    WishStatusInvalid,
    #[serde(rename = "WISH_NOT_YOURS")]
    WishNotYours,
    #[serde(rename = "AGREEMENT_NOT_MUTUAL")]
    AgreementNotMutual,
    #[serde(rename = "QUALITY_ALREADY_REVIEWED")]
    QualityAlreadyReviewed,
    #[serde(rename = "WISH_NOT_FINISHED")]
    WishNotFinished,
    // ===== Economy (3) =====
    #[serde(rename = "LOVE_POINT_INSUFFICIENT")]
    LovePointInsufficient,
    #[serde(rename = "DIAMOND_INSUFFICIENT")]
    DiamondInsufficient,
    #[serde(rename = "ADMIN_DAILY_DIAMOND_LIMIT_REACHED")]
    AdminDailyDiamondLimitReached,
    // ===== Sign-in (1) =====
    #[serde(rename = "SIGN_IN_ALREADY_DONE")]
    SignInAlreadyDone,
    // ===== Upload (4) =====
    #[serde(rename = "UPLOAD_SIZE_EXCEEDED")]
    UploadSizeExceeded,
    #[serde(rename = "UPLOAD_TYPE_NOT_ALLOWED")]
    UploadTypeNotAllowed,
    #[serde(rename = "UPLOAD_CONTENT_REJECTED")]
    UploadContentRejected,
    #[serde(rename = "UPLOAD_FILE_NOT_FOUND")]
    UploadFileNotFound,
    #[serde(rename = "UPLOAD_PERMISSION_DENIED")]
    UploadPermissionDenied,
    // ===== General (3) =====
    #[serde(rename = "IDEMPOTENCY_CONFLICT")]
    IdempotencyConflict,
    #[serde(rename = "INVALID_PARAMETER")]
    InvalidParameter,
    #[serde(rename = "INTERNAL_ERROR")]
    InternalError,
    #[serde(rename = "RESOURCE_NOT_FOUND")]
    ResourceNotFound,
}

impl FsdErrorCode {
    /// 错误码字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthInvalidToken => "AUTH_INVALID_TOKEN",
            Self::AuthTokenRevoked => "AUTH_TOKEN_REVOKED",
            Self::AuthAccountBanned => "AUTH_ACCOUNT_BANNED",
            Self::AuthCodeInvalid => "AUTH_CODE_INVALID",
            Self::UserNotFound => "USER_NOT_FOUND",
            Self::UserPhoneAlreadyBound => "USER_PHONE_ALREADY_BOUND",
            Self::UserNicknameInvalid => "USER_NICKNAME_INVALID",
            Self::UserAlreadyInGroup => "USER_ALREADY_IN_GROUP",
            Self::UserGroupNotEmpty => "USER_GROUP_NOT_EMPTY",
            Self::GroupNotFound => "GROUP_NOT_FOUND",
            Self::GroupMemberLimitExceeded => "GROUP_MEMBER_LIMIT_EXCEEDED",
            Self::GroupExitSettlementRequired => "GROUP_EXIT_SETTLEMENT_REQUIRED",
            Self::InviteCodeInvalid => "INVITE_CODE_INVALID",
            Self::InviteCodeUsed => "INVITE_CODE_USED",
            Self::InviteUserMismatch => "INVITE_USER_MISMATCH",
            Self::RoleSwapBlockedByOrder => "ROLE_SWAP_BLOCKED_BY_ORDER",
            Self::RoleSwapBlockedByWish => "ROLE_SWAP_BLOCKED_BY_WISH",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::RoleNotAllowed => "ROLE_NOT_ALLOWED",
            Self::FoodNotFound => "FOOD_NOT_FOUND",
            Self::FoodCapacityExceeded => "FOOD_CAPACITY_EXCEEDED",
            Self::OrderNotFound => "ORDER_NOT_FOUND",
            Self::OrderStatusInvalid => "ORDER_STATUS_INVALID",
            Self::OrderTypeInvalid => "ORDER_TYPE_INVALID",
            Self::DailyRewardLimitReached => "DAILY_REWARD_LIMIT_REACHED",
            Self::WishNotFound => "WISH_NOT_FOUND",
            Self::WishStatusInvalid => "WISH_STATUS_INVALID",
            Self::WishNotYours => "WISH_NOT_YOURS",
            Self::AgreementNotMutual => "AGREEMENT_NOT_MUTUAL",
            Self::QualityAlreadyReviewed => "QUALITY_ALREADY_REVIEWED",
            Self::WishNotFinished => "WISH_NOT_FINISHED",
            Self::LovePointInsufficient => "LOVE_POINT_INSUFFICIENT",
            Self::DiamondInsufficient => "DIAMOND_INSUFFICIENT",
            Self::AdminDailyDiamondLimitReached => "ADMIN_DAILY_DIAMOND_LIMIT_REACHED",
            Self::SignInAlreadyDone => "SIGN_IN_ALREADY_DONE",
            Self::UploadSizeExceeded => "UPLOAD_SIZE_EXCEEDED",
            Self::UploadTypeNotAllowed => "UPLOAD_TYPE_NOT_ALLOWED",
            Self::UploadContentRejected => "UPLOAD_CONTENT_REJECTED",
            Self::UploadFileNotFound => "UPLOAD_FILE_NOT_FOUND",
            Self::UploadPermissionDenied => "UPLOAD_PERMISSION_DENIED",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::InvalidParameter => "INVALID_PARAMETER",
            Self::InternalError => "INTERNAL_ERROR",
            Self::ResourceNotFound => "RESOURCE_NOT_FOUND",
        }
    }

    /// 默认 HTTP 状态码
    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::AuthInvalidToken
            | Self::AuthTokenRevoked
            | Self::AuthCodeInvalid
            | Self::AuthAccountBanned => StatusCode::UNAUTHORIZED,
            Self::PermissionDenied
            | Self::RoleNotAllowed
            | Self::UploadPermissionDenied => StatusCode::FORBIDDEN,
            Self::UserNotFound
            | Self::GroupNotFound
            | Self::FoodNotFound
            | Self::OrderNotFound
            | Self::WishNotFound
            | Self::UploadFileNotFound
            | Self::ResourceNotFound => StatusCode::NOT_FOUND,
            Self::UserPhoneAlreadyBound
            | Self::UserAlreadyInGroup
            | Self::InviteCodeUsed
            | Self::QualityAlreadyReviewed
            | Self::IdempotencyConflict => StatusCode::CONFLICT,
            Self::OrderStatusInvalid
            | Self::OrderTypeInvalid
            | Self::WishStatusInvalid
            | Self::WishNotYours
            | Self::AgreementNotMutual
            | Self::RoleSwapBlockedByOrder
            | Self::RoleSwapBlockedByWish
            | Self::UserNicknameInvalid
            | Self::InviteUserMismatch
            | Self::GroupMemberLimitExceeded
            | Self::GroupExitSettlementRequired
            | Self::UserGroupNotEmpty
            | Self::FoodCapacityExceeded
            | Self::DailyRewardLimitReached
            | Self::LovePointInsufficient
            | Self::DiamondInsufficient
            | Self::AdminDailyDiamondLimitReached
            | Self::SignInAlreadyDone
            | Self::InviteCodeInvalid
            | Self::WishNotFinished
            | Self::InvalidParameter => StatusCode::BAD_REQUEST,
            Self::UploadSizeExceeded => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UploadTypeNotAllowed => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::UploadContentRejected => StatusCode::UNPROCESSABLE_ENTITY,
            Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl fmt::Display for FsdErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Custom error types for the application
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "type", content = "message")]
pub enum CustomError {
    // ===== 通用兜底错误（FSD 未明确定义时使用） =====
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
    /// 428 User Not Found - 用户未注册（微信静默登录场景）
    #[serde(rename = "user_not_found")]
    UserNotFoundLegacy(String),
    /// 409 Conflict - 数据冲突（如重复添加）
    #[serde(rename = "conflict")]
    Conflict(String),
    /// 500 Internal Server Error - 服务器内部错误
    #[serde(rename = "internal_error")]
    InternalServerError(String),

    // ===== FSD §13.3 业务错误码（精确语义）=====
    // Auth
    AuthInvalidToken(String),
    AuthTokenRevoked(String),
    AuthAccountBanned(String),
    AuthCodeInvalid(String),
    // User
    UserNotFound(String),
    UserPhoneAlreadyBound(String),
    UserNicknameInvalid(String),
    UserAlreadyInGroup(String),
    UserGroupNotEmpty(String),
    // Group
    GroupNotFound(String),
    GroupMemberLimitExceeded(String),
    GroupExitSettlementRequired(String),
    InviteCodeInvalid(String),
    InviteCodeUsed(String),
    InviteUserMismatch(String),
    // Role
    RoleSwapBlockedByOrder(String),
    RoleSwapBlockedByWish(String),
    PermissionDenied(String),
    RoleNotAllowed(String),
    // Food
    FoodNotFound(String),
    FoodCapacityExceeded(String),
    // Order
    OrderNotFound(String),
    OrderStatusInvalid(String),
    OrderTypeInvalid(String),
    DailyRewardLimitReached(String),
    // Wish
    WishNotFound(String),
    WishStatusInvalid(String),
    WishNotYours(String),
    AgreementNotMutual(String),
    QualityAlreadyReviewed(String),
    WishNotFinished(String),
    // Economy
    LovePointInsufficient(String),
    DiamondInsufficient(String),
    AdminDailyDiamondLimitReached(String),
    // Sign-in
    SignInAlreadyDone(String),
    // Upload
    UploadSizeExceeded(String),
    UploadTypeNotAllowed(String),
    UploadTokenInvalid(String),
    UploadContentRejected(String),
    UploadFileNotFound(String),
    UploadPermissionDenied(String),
    // General
    IdempotencyConflict(String),
    InvalidParameter(String),
    InternalError(String),
    ResourceNotFound(String),
}

impl CustomError {
    // ============ 通用兜底构造函数 ============

    pub fn bad_request<S: Into<String>>(msg: S) -> Self {
        Self::BadRequest(msg.into())
    }

    #[allow(dead_code)]
    pub fn unauthorized<S: Into<String>>(msg: S) -> Self {
        Self::Unauthorized(msg.into())
    }

    #[allow(dead_code)]
    pub fn forbidden<S: Into<String>>(msg: S) -> Self {
        Self::Forbidden(msg.into())
    }

    pub fn not_found<S: Into<String>>(msg: S) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict<S: Into<String>>(msg: S) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::InternalServerError(msg.into())
    }

    // ============ FSD §13.3 业务构造函数（41 个）============

    // Auth
    pub fn auth_invalid_token<S: Into<String>>(msg: S) -> Self { Self::AuthInvalidToken(msg.into()) }
    pub fn auth_token_revoked<S: Into<String>>(msg: S) -> Self { Self::AuthTokenRevoked(msg.into()) }
    pub fn auth_account_banned<S: Into<String>>(msg: S) -> Self { Self::AuthAccountBanned(msg.into()) }
    pub fn auth_code_invalid<S: Into<String>>(msg: S) -> Self { Self::AuthCodeInvalid(msg.into()) }
    // User
    pub fn user_not_found<S: Into<String>>(msg: S) -> Self { Self::UserNotFound(msg.into()) }
    pub fn user_phone_already_bound<S: Into<String>>(msg: S) -> Self { Self::UserPhoneAlreadyBound(msg.into()) }
    pub fn user_nickname_invalid<S: Into<String>>(msg: S) -> Self { Self::UserNicknameInvalid(msg.into()) }
    pub fn user_already_in_group<S: Into<String>>(msg: S) -> Self { Self::UserAlreadyInGroup(msg.into()) }
    pub fn user_group_not_empty<S: Into<String>>(msg: S) -> Self { Self::UserGroupNotEmpty(msg.into()) }
    // Group
    pub fn group_not_found<S: Into<String>>(msg: S) -> Self { Self::GroupNotFound(msg.into()) }
    pub fn group_member_limit_exceeded<S: Into<String>>(msg: S) -> Self { Self::GroupMemberLimitExceeded(msg.into()) }
    pub fn group_exit_settlement_required<S: Into<String>>(msg: S) -> Self { Self::GroupExitSettlementRequired(msg.into()) }
    pub fn invite_code_invalid<S: Into<String>>(msg: S) -> Self { Self::InviteCodeInvalid(msg.into()) }
    pub fn invite_code_used<S: Into<String>>(msg: S) -> Self { Self::InviteCodeUsed(msg.into()) }
    pub fn invite_user_mismatch<S: Into<String>>(msg: S) -> Self { Self::InviteUserMismatch(msg.into()) }
    // Role
    pub fn role_swap_blocked_by_order<S: Into<String>>(msg: S) -> Self { Self::RoleSwapBlockedByOrder(msg.into()) }
    pub fn role_swap_blocked_by_wish<S: Into<String>>(msg: S) -> Self { Self::RoleSwapBlockedByWish(msg.into()) }
    pub fn permission_denied<S: Into<String>>(msg: S) -> Self { Self::PermissionDenied(msg.into()) }
    pub fn role_not_allowed<S: Into<String>>(msg: S) -> Self { Self::RoleNotAllowed(msg.into()) }
    // Food
    pub fn food_not_found<S: Into<String>>(msg: S) -> Self { Self::FoodNotFound(msg.into()) }
    pub fn food_capacity_exceeded<S: Into<String>>(msg: S) -> Self { Self::FoodCapacityExceeded(msg.into()) }
    // Order
    pub fn order_not_found<S: Into<String>>(msg: S) -> Self { Self::OrderNotFound(msg.into()) }
    pub fn order_status_invalid<S: Into<String>>(msg: S) -> Self { Self::OrderStatusInvalid(msg.into()) }
    pub fn order_type_invalid<S: Into<String>>(msg: S) -> Self { Self::OrderTypeInvalid(msg.into()) }
    pub fn daily_reward_limit_reached<S: Into<String>>(msg: S) -> Self { Self::DailyRewardLimitReached(msg.into()) }
    // Wish
    pub fn wish_not_found<S: Into<String>>(msg: S) -> Self { Self::WishNotFound(msg.into()) }
    pub fn wish_status_invalid<S: Into<String>>(msg: S) -> Self { Self::WishStatusInvalid(msg.into()) }
    pub fn wish_not_yours<S: Into<String>>(msg: S) -> Self { Self::WishNotYours(msg.into()) }
    pub fn agreement_not_mutual<S: Into<String>>(msg: S) -> Self { Self::AgreementNotMutual(msg.into()) }
    pub fn quality_already_reviewed<S: Into<String>>(msg: S) -> Self { Self::QualityAlreadyReviewed(msg.into()) }
    pub fn wish_not_finished<S: Into<String>>(msg: S) -> Self { Self::WishNotFinished(msg.into()) }
    // Economy
    pub fn love_point_insufficient<S: Into<String>>(msg: S) -> Self { Self::LovePointInsufficient(msg.into()) }
    pub fn diamond_insufficient<S: Into<String>>(msg: S) -> Self { Self::DiamondInsufficient(msg.into()) }
    pub fn admin_daily_diamond_limit_reached<S: Into<String>>(msg: S) -> Self { Self::AdminDailyDiamondLimitReached(msg.into()) }
    // Sign-in
    pub fn sign_in_already_done<S: Into<String>>(msg: S) -> Self { Self::SignInAlreadyDone(msg.into()) }
    // Upload
    pub fn upload_size_exceeded<S: Into<String>>(msg: S) -> Self { Self::UploadSizeExceeded(msg.into()) }
    pub fn upload_type_not_allowed<S: Into<String>>(msg: S) -> Self { Self::UploadTypeNotAllowed(msg.into()) }
    pub fn upload_token_invalid<S: Into<String>>(msg: S) -> Self { Self::UploadTokenInvalid(msg.into()) }
    pub fn upload_content_rejected<S: Into<String>>(msg: S) -> Self { Self::UploadContentRejected(msg.into()) }
    pub fn upload_file_not_found<S: Into<String>>(msg: S) -> Self { Self::UploadFileNotFound(msg.into()) }
    pub fn upload_permission_denied<S: Into<String>>(msg: S) -> Self { Self::UploadPermissionDenied(msg.into()) }
    // General
    pub fn idempotency_conflict<S: Into<String>>(msg: S) -> Self { Self::IdempotencyConflict(msg.into()) }
    pub fn invalid_parameter<S: Into<String>>(msg: S) -> Self { Self::InvalidParameter(msg.into()) }
    pub fn internal_error<S: Into<String>>(msg: S) -> Self { Self::InternalError(msg.into()) }
    pub fn resource_not_found<S: Into<String>>(msg: S) -> Self { Self::ResourceNotFound(msg.into()) }

    // ============ 错误码 + 状态码映射 ============

    /// 获取 FSD §13.3 错误码
    pub fn fsd_code(&self) -> FsdErrorCode {
        match self {
            // 通用兜底
            Self::BadRequest(_) | Self::InvalidParameter(_) => FsdErrorCode::InvalidParameter,
            Self::Unauthorized(_) => FsdErrorCode::AuthInvalidToken,
            Self::Forbidden(_) => FsdErrorCode::PermissionDenied,
            Self::NotFound(_) | Self::UserNotFoundLegacy(_) => FsdErrorCode::ResourceNotFound,
            Self::Conflict(_) => FsdErrorCode::IdempotencyConflict,
            Self::InternalServerError(_) => FsdErrorCode::InternalError,
            // 业务错误
            Self::AuthInvalidToken(_) => FsdErrorCode::AuthInvalidToken,
            Self::AuthTokenRevoked(_) => FsdErrorCode::AuthTokenRevoked,
            Self::AuthAccountBanned(_) => FsdErrorCode::AuthAccountBanned,
            Self::AuthCodeInvalid(_) => FsdErrorCode::AuthCodeInvalid,
            Self::UserNotFound(_) => FsdErrorCode::UserNotFound,
            Self::UserPhoneAlreadyBound(_) => FsdErrorCode::UserPhoneAlreadyBound,
            Self::UserNicknameInvalid(_) => FsdErrorCode::UserNicknameInvalid,
            Self::UserAlreadyInGroup(_) => FsdErrorCode::UserAlreadyInGroup,
            Self::UserGroupNotEmpty(_) => FsdErrorCode::UserGroupNotEmpty,
            Self::GroupNotFound(_) => FsdErrorCode::GroupNotFound,
            Self::GroupMemberLimitExceeded(_) => FsdErrorCode::GroupMemberLimitExceeded,
            Self::GroupExitSettlementRequired(_) => FsdErrorCode::GroupExitSettlementRequired,
            Self::InviteCodeInvalid(_) => FsdErrorCode::InviteCodeInvalid,
            Self::InviteCodeUsed(_) => FsdErrorCode::InviteCodeUsed,
            Self::InviteUserMismatch(_) => FsdErrorCode::InviteUserMismatch,
            Self::RoleSwapBlockedByOrder(_) => FsdErrorCode::RoleSwapBlockedByOrder,
            Self::RoleSwapBlockedByWish(_) => FsdErrorCode::RoleSwapBlockedByWish,
            Self::PermissionDenied(_) => FsdErrorCode::PermissionDenied,
            Self::RoleNotAllowed(_) => FsdErrorCode::RoleNotAllowed,
            Self::FoodNotFound(_) => FsdErrorCode::FoodNotFound,
            Self::FoodCapacityExceeded(_) => FsdErrorCode::FoodCapacityExceeded,
            Self::OrderNotFound(_) => FsdErrorCode::OrderNotFound,
            Self::OrderStatusInvalid(_) => FsdErrorCode::OrderStatusInvalid,
            Self::OrderTypeInvalid(_) => FsdErrorCode::OrderTypeInvalid,
            Self::DailyRewardLimitReached(_) => FsdErrorCode::DailyRewardLimitReached,
            Self::WishNotFound(_) => FsdErrorCode::WishNotFound,
            Self::WishStatusInvalid(_) => FsdErrorCode::WishStatusInvalid,
            Self::WishNotYours(_) => FsdErrorCode::WishNotYours,
            Self::AgreementNotMutual(_) => FsdErrorCode::AgreementNotMutual,
            Self::QualityAlreadyReviewed(_) => FsdErrorCode::QualityAlreadyReviewed,
            Self::WishNotFinished(_) => FsdErrorCode::WishNotFinished,
            Self::LovePointInsufficient(_) => FsdErrorCode::LovePointInsufficient,
            Self::DiamondInsufficient(_) => FsdErrorCode::DiamondInsufficient,
            Self::AdminDailyDiamondLimitReached(_) => FsdErrorCode::AdminDailyDiamondLimitReached,
            Self::SignInAlreadyDone(_) => FsdErrorCode::SignInAlreadyDone,
            Self::UploadSizeExceeded(_) => FsdErrorCode::UploadSizeExceeded,
            Self::UploadTypeNotAllowed(_) => FsdErrorCode::UploadTypeNotAllowed,
            Self::UploadTokenInvalid(_) => FsdErrorCode::UploadContentRejected, // FSD 没有独立 upload_token_invalid，归到 UPLOAD_CONTENT_REJECTED
            Self::UploadContentRejected(_) => FsdErrorCode::UploadContentRejected,
            Self::UploadFileNotFound(_) => FsdErrorCode::UploadFileNotFound,
            Self::UploadPermissionDenied(_) => FsdErrorCode::UploadPermissionDenied,
            Self::IdempotencyConflict(_) => FsdErrorCode::IdempotencyConflict,
            Self::InternalError(_) => FsdErrorCode::InternalError,
            Self::ResourceNotFound(_) => FsdErrorCode::ResourceNotFound,
        }
    }

    /// HTTP 状态码
    pub fn http_status(&self) -> StatusCode {
        self.fsd_code().http_status()
    }

    /// 错误消息
    pub fn message(&self) -> String {
        match self {
            Self::BadRequest(m) | Self::Unauthorized(m) | Self::Forbidden(m)
            | Self::NotFound(m) | Self::UserNotFoundLegacy(m) | Self::Conflict(m)
            | Self::InternalServerError(m)
            | Self::AuthInvalidToken(m) | Self::AuthTokenRevoked(m)
            | Self::AuthAccountBanned(m) | Self::AuthCodeInvalid(m)
            | Self::UserNotFound(m) | Self::UserPhoneAlreadyBound(m)
            | Self::UserNicknameInvalid(m) | Self::UserAlreadyInGroup(m)
            | Self::UserGroupNotEmpty(m)
            | Self::GroupNotFound(m) | Self::GroupMemberLimitExceeded(m)
            | Self::GroupExitSettlementRequired(m)
            | Self::InviteCodeInvalid(m) | Self::InviteCodeUsed(m)
            | Self::InviteUserMismatch(m)
            | Self::RoleSwapBlockedByOrder(m) | Self::RoleSwapBlockedByWish(m)
            | Self::PermissionDenied(m) | Self::RoleNotAllowed(m)
            | Self::FoodNotFound(m) | Self::FoodCapacityExceeded(m)
            | Self::OrderNotFound(m) | Self::OrderStatusInvalid(m)
            | Self::OrderTypeInvalid(m) | Self::DailyRewardLimitReached(m)
            | Self::WishNotFound(m) | Self::WishStatusInvalid(m)
            | Self::WishNotYours(m) | Self::AgreementNotMutual(m)
            | Self::QualityAlreadyReviewed(m) | Self::WishNotFinished(m)
            | Self::LovePointInsufficient(m) | Self::DiamondInsufficient(m)
            | Self::AdminDailyDiamondLimitReached(m)
            | Self::SignInAlreadyDone(m)
            | Self::UploadSizeExceeded(m) | Self::UploadTypeNotAllowed(m)
            | Self::UploadTokenInvalid(m)
            | Self::UploadContentRejected(m) | Self::UploadFileNotFound(m)
            | Self::UploadPermissionDenied(m)
            | Self::IdempotencyConflict(m) | Self::InvalidParameter(m)
            | Self::InternalError(m) | Self::ResourceNotFound(m) => m.clone(),
        }
    }
}

// ============ WebResponseError Trait ============

impl WebResponseError for CustomError {
    fn status_code(&self) -> StatusCode {
        self.http_status()
    }

    fn error_response(&self, _: &ntex::web::HttpRequest) -> HttpResponse {
        // FSD §1.1 / §13.3 统一错误响应：
        // { "code": 错误码数字, "message": "FSD 错误码字符串", "data": { "message": "详情" } }
        let fsd_code = self.fsd_code();
        let status = fsd_code.http_status();
        let message = self.message();

        #[derive(Serialize)]
        struct ErrorEnvelope {
            code: u16,
            message: &'static str,         // FSD 错误码 (e.g. "AUTH_INVALID_TOKEN")
            data: ErrorData,
        }
        #[derive(Serialize)]
        struct ErrorData {
            message: String,               // 详情描述
        }

        let body = ErrorEnvelope {
            code: status.as_u16(),
            message: fsd_code.as_str(),
            data: ErrorData { message },
        };

        HttpResponse::build(status)
            .content_type("application/json; charset=utf-8")
            .json(&body)
    }
}

// ============ Display Trait ============

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.fsd_code().as_str(), self.message())
    }
}

// ============ From Implementations ============

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
        Self::invalid_parameter(format!("格式转换异常: {e}"))
    }
}

impl From<error::PayloadError> for CustomError {
    fn from(e: error::PayloadError) -> Self {
        Self::invalid_parameter(format!("请求体解析错误: {e}"))
    }
}

impl From<DefaultError> for CustomError {
    fn from(e: DefaultError) -> Self {
        Self::invalid_parameter(format!("参数错误: {e:?}"))
    }
}

impl From<JoinError> for CustomError {
    fn from(e: JoinError) -> Self {
        Self::internal_error(format!("异步任务执行失败: {e:?}"))
    }
}

impl From<reqwest::Error> for CustomError {
    fn from(e: reqwest::Error) -> Self {
        log_error!(target: "reqwest", "reqwest error: {e:?}");
        if e.is_timeout() {
            Self::internal_error("请求超时".to_string())
        } else if e.is_connect() {
            Self::invalid_parameter("无法连接到远程服务".to_string())
        } else {
            Self::internal_error(e.to_string())
        }
    }
}

impl From<RedisError> for CustomError {
    fn from(e: RedisError) -> Self {
        Self::internal_error(format!("Redis错误: {e}"))
    }
}

impl From<ToStringError> for CustomError {
    fn from(e: ToStringError) -> Self {
        Self::upload_token_invalid(format!("七牛云Token转换失败: {e:?}"))
    }
}

impl From<serde_json::Error> for CustomError {
    fn from(e: serde_json::Error) -> Self {
        Self::invalid_parameter(format!("JSON解析失败: {e}"))
    }
}

impl From<idgenerator::error::OptionError> for CustomError {
    fn from(e: idgenerator::error::OptionError) -> Self {
        Self::internal_error(format!("ID生成器错误: {e:?}"))
    }
}

impl From<ntex::ws::error::HandshakeError> for CustomError {
    fn from(e: ntex::ws::error::HandshakeError) -> Self {
        Self::invalid_parameter(format!("WebSocket握手失败: {e:?}"))
    }
}

impl From<sqlx::Error> for CustomError {
    fn from(e: sqlx::Error) -> Self {
        if let Some(db_err) = e.as_database_error() {
            let code = db_err.code();
            let message = db_err.message();

            match code {
                Some(cow) if cow == "23505" => Self::idempotency_conflict("数据已存在，请勿重复添加"),
                Some(cow) if cow == "23503" => Self::invalid_parameter(format!("关联数据不存在: {message}")),
                Some(cow) if cow == "23502" => Self::invalid_parameter(format!("必填字段不能为空: {message}")),
                _ => {
                    log::debug!("Unhandled database error: {code:?} - {message}");
                    Self::internal_error("数据库操作失败")
                }
            }
        } else {
            match e {
                sqlx::Error::RowNotFound => Self::resource_not_found("找不到对应数据"),
                sqlx::Error::ColumnNotFound(col) => Self::invalid_parameter(format!("查询字段不存在: {col}")),
                sqlx::Error::Decode(err) => Self::invalid_parameter(format!("数据解码失败: {err}")),
                _ => {
                    log::debug!("Unhandled sqlx error: {e:?}");
                    Self::internal_error("数据库操作失败")
                }
            }
        }
    }
}
