// src/cache.rs
use crate::models::users::UserPublic; // 兼容旧命名，实际等价于 UserPublic
use redis::{AsyncCommands, Client};
use serde_json;

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

    pub async fn get_user(&self, user_id: &i32) -> Result<Option<UserPublic>, redis::RedisError> {
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
            }
            None => Ok(None),
        }
    }

    pub async fn set_user(
        &self,
        user: &UserPublic,
        expire_secs: usize,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;

        // 序列化User到JSON
        let user_json = serde_json::to_string(user).unwrap();

        // 存入Redis并设置过期时间
        let _: () = conn
            .set_ex(format!("user:{:?}", user.user_id), user_json, expire_secs)
            .await?;

        Ok(())
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;
        let _: () = conn.del(format!("user:{}", user_id)).await?;
        Ok(())
    }

    // 新增：使用 i64 user_id 的缓存方法，供重构后的用户模块调用
    pub async fn get_user_public(
        &self,
        user_id: &i64,
    ) -> Result<Option<UserPublic>, redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;
        let user_json: Option<String> = conn.get(format!("user:{}", user_id)).await?;
        if let Some(json) = user_json {
            Ok(serde_json::from_str(&json).ok())
        } else {
            Ok(None)
        }
    }

    pub async fn set_user_public(
        &self,
        user: &UserPublic,
        expire_secs: usize,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.client.get_async_connection().await?;
        let user_json = serde_json::to_string(user).unwrap();
        let _: () = conn
            .set_ex(format!("user:{}", user.user_id), user_json, expire_secs)
            .await?;
        Ok(())
    }
}
