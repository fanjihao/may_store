use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsgTemplate {
    pub ship_id: Option<i32>,
    pub user_id: Option<i32>,
    pub bind_id: Option<i32>,
    pub award: Option<String>,
    pub url: Option<String>,
    pub open_id: Option<i32>,
    pub template_id: Option<i32>,
    pub target: Option<i32>,
    pub msg_type: Option<i32>,
    pub msg_content: Option<String>,
    pub recv_user_id: Option<i32>,
    pub order_id: Option<i32>,
    pub food_id: Option<i32>,
    pub msg_status: Option<i32>,
    pub msg_food_repeal: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMessage {
    pub template_id: String,
    pub push_id: String,
    pub date: Option<String>,
    pub city: Option<String>,
    pub weather: Option<String>,
    pub low: Option<String>,
    pub high: Option<String>,
    pub love_days: Option<String>,
    pub birthdays: Option<String>,
}

impl TemplateMessage {
    pub fn send_template(
        template_id: String,
        push_id: String,
        date: Option<String>,
        city: Option<String>,
        weather: Option<String>,
        low: Option<String>,
        high: Option<String>,
        love_days: Option<String>,
        birthdays: Option<String>,
    ) -> Self {
        Self {
            template_id: template_id,
            push_id: push_id,
            date: date,
            city: city,
            weather: weather,
            low: low,
            high: high,
            love_days: love_days,
            birthdays: birthdays,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offical {
    pub signature: Option<String>,
    pub timestamp: Option<String>,
    pub nonce: Option<String>,
    pub echostr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Xml {
    #[serde(rename = "ToUserName")]
    pub to_user_name: Option<String>,

    #[serde(rename = "FromUserName")]
    pub from_user_name: Option<String>,

    #[serde(rename = "CreateTime")]
    create_time: Option<u64>,

    #[serde(rename = "MsgType")]
    pub msg_type: Option<String>,

    #[serde(rename = "Content")]
    pub content: Option<String>,

    #[serde(rename = "MsgId")]
    msg_id: Option<String>,

    #[serde(rename = "MsgDataId")]
    msg_data_id: Option<String>,

    #[serde(rename = "Idx")]
    idx: Option<String>,

    // 图片信息
    #[serde(rename = "PicUrl")]
    pic_url: Option<String>,

    #[serde(rename = "MediaId")]
    media_id: Option<String>,

    // 语音信息
    #[serde(rename = "Format")]
    format: Option<String>,

    #[serde(rename = "Recognition")] // 语音识别结果 utf8
    recognition: Option<String>,

    // 视频信息
    #[serde(rename = "ThumbMediaId")]
    thumb_media_id: Option<String>, // 视频消息缩略图的媒体id，可以调用多媒体文件下载接口拉取数据。

    // 地理信息
    #[serde(rename = "Location_X")]
    location_x: Option<String>,

    #[serde(rename = "Location_Y")]
    location_y: Option<String>,

    #[serde(rename = "Scale")]
    scale: Option<String>,

    #[serde(rename = "Label")]
    label: Option<String>,

    // 链接信息
    #[serde(rename = "Title")]
    title: Option<String>,

    #[serde(rename = "Description")]
    description: Option<String>,

    #[serde(rename = "Url")]
    url: Option<String>,

    #[serde(rename = "Event")]
    pub event: Option<String>,

    #[serde(rename = "EventKey")]
    pub event_key: Option<String>,
}
