use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoomStatus {
    Lobby,
    Started,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomIn {
    pub room_code: String,
    pub user_id: i64,
    pub nick_name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub create_if_not_exists: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveRoomIn {
    pub room_code: String,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetReadyIn {
    pub room_code: String,
    pub user_id: i64,
    pub ready: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartGameIn {
    pub room_code: String,
    pub user_id: i64,
    pub game_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastIn {
    pub room_code: String,
    pub user_id: i64,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRevealIn {
    pub room_code: String,
    pub user_id: i64,
    pub to_user_id: i64,
    pub role: Role,
    #[serde(default)]
    pub wolf_user_ids: Vec<i64>,
    #[serde(default)]
    pub wolf_count: i64,
    #[serde(default)]
    pub total_players: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomStateOut {
    pub room_code: String,
    pub host_user_id: i64,
    pub status: RoomStatus,
    pub members: Vec<RoomMemberOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<GameOut>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameOut {
    pub key: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMemberOut {
    pub user_id: i64,
    pub nick_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub ready: bool,
    #[serde(default)]
    pub is_host: bool,
    pub joined_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    Wolf,
    Villager,
    Witness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleRevealOut {
    pub to_user_id: i64,
    pub role: Role,
    pub wolf_user_ids: Vec<i64>,
    pub wolf_count: i64,
    pub total_players: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsOutboundEnvelope {
    #[serde(rename = "type")]
    pub r#type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsInboundEnvelope {
    #[serde(rename = "type")]
    pub r#type: String,
    pub payload: Value,
}
