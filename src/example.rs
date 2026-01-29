pub const TOKEN_SECRET_KEY: &[u8] = b"maystore";

// 七牛云
pub const ACCESS_KEY: &str = "xxxxxxxxxx";
pub const SECRET_KEY: &str = "xxxxxxxxxxxxx";
pub const BUCKET_NAME: &str = "maystore";

// 小程序
pub const APP_ID: &str = "xxxxxxxxxxx";
pub const APP_SECRET: &str = "xxxxxxxxxx";
// 公众号
pub const OFFCIAL_APP_ID: &str = "xxxxxxxxxxxxx";
pub const OFFCIAL_APP_SECRET: &str = "xxxxxxxxxxxxxx";
// 百度天气
pub const BAIDU_AK: &str = "xxxxxxxxxxx";

pub fn validate_username(username: &str) -> Result<(), &'static str> {
    if username.len() < 2 || username.len() > 30 {
        return Err("用户名长度必须在2到30个字符之间");
    }
    // Allow alphanumeric, underscore, hyphen.
    if !username.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err("用户名只能包含字母、数字、下划线和连字符");
    }
    Ok(())
}

pub fn validate_nickname(nickname: &str) -> Result<(), &'static str> {
    if nickname.len() > 30 {
        return Err("昵称长度不能超过30个字符");
    }
    // Ban dangerous characters used in SQL injection
    if nickname.contains('\'') || nickname.contains('"') || nickname.contains(";") || nickname.contains("--") {
        return Err("昵称包含非法字符");
    }
    Ok(())
}