pub const TOKEN_SECRET_KEY: &[u8] = b"maystore";

pub const ACCESS_KEY: &str = "yvnXawd924WCoI6fqyIFK1AS0aQFPY8oee89NF5s";
pub const SECRET_KEY: &str = "GzIKz9OGjc0ydYRvHRaQ4d8ryCicGv9B4afJIFmn";
pub const BUCKET_NAME: &str = "maystore";

pub const APP_ID: &str = "wx14fd1ae66e63259e";
pub const APP_SECRET: &str = "b02ef6ea4d2f1955bac66fdf9efbad06";
pub const OFFCIAL_APP_ID: &str = "wx640a2bd5fb33a287";
pub const OFFCIAL_APP_SECRET: &str = "9a735dfa8561f510663a7e9e3c3a6a2e";

pub fn validate_username(username: &str) -> Result<(), &'static str> {
    if username.len() < 2 || username.len() > 30 {
        return Err("用户名长度必须在2到30个字符之间");
    }
    // Allow alphanumeric, underscore, hyphen.
    if !username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err("用户名只能包含字母、数字、下划线和连字符");
    }
    Ok(())
}

pub fn validate_nickname(nickname: &str) -> Result<(), &'static str> {
    if nickname.len() > 30 {
        return Err("昵称长度不能超过30个字符");
    }
    // Ban dangerous characters used in SQL injection
    if nickname.contains('\'')
        || nickname.contains('"')
        || nickname.contains(";")
        || nickname.contains("--")
    {
        return Err("昵称包含非法字符");
    }
    Ok(())
}
pub mod response;
