// src/cache.rs
use redis::{Client, AsyncCommands};
use crate::models::users::UserInfo;  // 假设您有一个用户模型

// 定义Redis缓存服务
#[derive(Debug, Clone)]
pub struct RedisCache {
    client: Client,
}

impl RedisCache {
    pub fn new(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = Client::open(redis_url)?;
        Ok(Self { client })
    }
    
    pub async fn get_user(&self, user_id: &i32) -> Result<Option<UserInfo>, redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;
        
        // 尝试从Redis获取用户信息
        let user_json: Option<String> = conn.get(format!("user:{}", user_id)).await?;
        
        match user_json {
            Some(json) => {
                // 反序列化JSON到User结构
                match serde_json::from_str(&json) {
                    Ok(user) => Ok(Some(user)),
                    Err(_) => Ok(None),
                }
            },
            None => Ok(None),
        }
    }
    
    pub async fn set_user(&self, user: &UserInfo, expire_secs: usize) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;
        
        // 序列化User到JSON
        let user_json = serde_json::to_string(user).unwrap();
        
        // 存入Redis并设置过期时间
        let _: () = conn.set_ex(
            format!("user:{:?}", user.user_id), 
            user_json, 
            expire_secs
        ).await?;
        
        Ok(())
    }
    
    pub async fn delete_user(&self, user_id: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;
        let _: () = conn.del(format!("user:{}", user_id)).await?;
        Ok(())
    }
}