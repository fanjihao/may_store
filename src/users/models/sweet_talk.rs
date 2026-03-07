use crate::models::pagination::CursorQuery;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SweetTalkRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SweetTalkOut {
    pub talk_id: i64,
    pub user_id: i64,
    pub nick_name: Option<String>,
    pub avatar: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweetTalkCursor {
    pub created_at: DateTime<Utc>,
    pub talk_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct SweetTalkQuery {
    #[serde(flatten)]
    pub pagination: CursorQuery,
    pub group_id: Option<i64>,
}
