use ntex::web::HttpResponse;
use serde::Serialize;

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub code: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> HttpResponse {
        HttpResponse::Ok().json(&Self {
            code: 200,
            message: "Success".to_string(),
            data: Some(data),
        })
    }
}

impl ApiResponse<()> {
    pub fn ok() -> HttpResponse {
        HttpResponse::Ok().json(&Self {
            code: 200,
            message: "Success".to_string(),
            data: None,
        })
    }
}
