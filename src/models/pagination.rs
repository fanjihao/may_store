use serde::{Deserialize, Serialize};
use utoipa::{ToSchema, IntoParams};
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

pub fn encode_cursor<T: Serialize>(cursor: &T) -> String {
    let json = serde_json::to_string(cursor).unwrap_or_default();
    STANDARD.encode(json)
}

pub fn decode_cursor<T: for<'de> Deserialize<'de>>(cursor_str: &str) -> Option<T> {
    let bytes = STANDARD.decode(cursor_str).ok()?;
    serde_json::from_slice(&bytes).ok()
}
