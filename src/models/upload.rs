use serde::Deserialize;
use utoipa::ToSchema;


#[derive(Deserialize, ToSchema)]
pub struct UploadFile {
    #[schema(format = "binary")]
    pub file: String,
}