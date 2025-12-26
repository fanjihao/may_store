use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use std::env;

use crate::errors::CustomError;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImUserSigOut {
    pub sdk_app_id: u64,
    pub identifier: String,
    pub user_id: i64,
    pub user_sig: String,
    pub expire_at: i64,
}

#[derive(Debug, Clone)]
pub struct ImConfig {
    pub sdk_app_id: u64,
    pub secret_key: String,
    pub expire_seconds: u64,
}

impl ImConfig {
    pub fn admin_identifier(&self) -> String {
        env::var("TENCENT_IM_ADMIN_IDENTIFIER").unwrap_or_else(|_| "administrator".to_string())
    }
}

// ====== Mini-game room models ======

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImRoomOut {
    pub group_id: String,
    pub name: String,
    pub member_num: Option<u32>,
    pub owner_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImRoomListOut {
    pub rooms: Vec<ImRoomOut>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImStartGameOut {
    pub group_id: String,
    pub started_at: i64,
    pub total_players: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImVoteIn {
    /// 被投票的玩家 identifier；null 表示弃票
    pub target_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImVoteOut {
    pub group_id: String,
    pub voter_identifier: String,
    pub voted_count: u32,
    pub total_alive: u32,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ImDismissRoomIn {
    pub group_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImDismissRoomOut {
    pub group_id: String,
}

impl ImConfig {
    pub fn from_env() -> Result<Self, CustomError> {
        let sdk_app_id = env::var("TENCENT_IM_SDK_APP_ID")
            .map_err(|_| CustomError::BadRequest("Missing env: TENCENT_IM_SDK_APP_ID".into()))?
            .parse::<u64>()
            .map_err(|_| CustomError::BadRequest("Invalid env: TENCENT_IM_SDK_APP_ID".into()))?;

        let secret_key = env::var("TENCENT_IM_SECRET_KEY")
            .map_err(|_| CustomError::BadRequest("Missing env: TENCENT_IM_SECRET_KEY".into()))?;

        let expire_seconds = env::var("TENCENT_IM_EXPIRE_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(86400);

        Ok(Self {
            sdk_app_id,
            secret_key,
            expire_seconds,
        })
    }
}
