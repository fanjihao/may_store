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
    #[allow(dead_code)]
    pub fn success(data: T) -> HttpResponse {
        HttpResponse::Ok().json(&Self {
            code: 200,
            message: "Success".to_string(),
            data: Some(data),
        })
    }
}

impl ApiResponse<()> {
    #[allow(dead_code)]
    pub fn ok() -> HttpResponse {
        HttpResponse::Ok().json(&Self {
            code: 200,
            message: "Success".to_string(),
            data: None,
        })
    }

    /// 201 Created,空 data。用于 POST 注册等"创建成功且无返回体"的接口。
    #[allow(dead_code)]
    pub fn created() -> HttpResponse {
        HttpResponse::Created().json(&Self {
            code: 201,
            message: "Created".to_string(),
            data: None,
        })
    }
}
