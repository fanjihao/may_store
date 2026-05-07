// 基础设施层 - PostgreSQL 订单仓储实现
// 实现 domain::order::OrderRepository trait

use sqlx::{PgPool, Row};
use crate::domain::order::{OrderRepository, OrderItemRepository, OrderRecord, OrderItemRecord};
use crate::errors::CustomError;

/// PostgreSQL 订单仓储
pub struct PostgresOrderRepository {
    pool: PgPool,
}

impl PostgresOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl OrderRepository for PostgresOrderRepository {
    async fn find_by_id(&self, order_id: i64) -> Result<Option<OrderRecord>, CustomError> {
        let rec = sqlx::query_as::<_, OrderRecord>(
            r#"SELECT order_id, user_id, guest_id, group_id, status, goal_time, remark, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at, is_guest
               FROM orders WHERE order_id = $1"#
        )
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(rec)
    }

    async fn find_by_user_id(&self, user_id: i64, limit: i64) -> Result<Vec<OrderRecord>, CustomError> {
        let recs = sqlx::query_as::<_, OrderRecord>(
            r#"SELECT order_id, user_id, guest_id, group_id, status, goal_time, remark, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at, is_guest
               FROM orders WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2"#
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(recs)
    }

    async fn find_by_group_id(&self, group_id: i64, limit: i64) -> Result<Vec<OrderRecord>, CustomError> {
        let recs = sqlx::query_as::<_, OrderRecord>(
            r#"SELECT order_id, user_id, guest_id, group_id, status, goal_time, remark, points_reward, cancel_reason, reject_reason, last_status_change_at, created_at, updated_at, is_guest
               FROM orders WHERE group_id = $1 ORDER BY created_at DESC LIMIT $2"#
        )
        .bind(group_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(recs)
    }

    async fn save(&self, order: &OrderRecord) -> Result<(), CustomError> {
        sqlx::query(
            r#"INSERT INTO orders (order_id, user_id, group_id, status, goal_time, remark, points_reward)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#
        )
        .bind(order.order_id)
        .bind(order.user_id)
        .bind(order.group_id)
        .bind(&order.status)
        .bind(order.goal_time)
        .bind(&order.remark)
        .bind(order.points_reward)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_status(&self, order_id: i64, status: &str) -> Result<(), CustomError> {
        sqlx::query("UPDATE orders SET status = $2, last_status_change_at = NOW() WHERE order_id = $1")
            .bind(order_id)
            .bind(status)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete(&self, order_id: i64) -> Result<(), CustomError> {
        sqlx::query("UPDATE orders SET status = 'DELETED' WHERE order_id = $1")
            .bind(order_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

impl OrderItemRepository for PostgresOrderRepository {
    async fn find_by_order_id(&self, order_id: i64) -> Result<Vec<OrderItemRecord>, CustomError> {
        let recs = sqlx::query_as::<_, OrderItemRecord>(
            r#"SELECT id, order_id, food_id, quantity, price, snapshot_json, created_at
               FROM order_items WHERE order_id = $1"#
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(recs)
    }

    async fn save(&self, item: &OrderItemRecord) -> Result<(), CustomError> {
        sqlx::query(
            r#"INSERT INTO order_items (order_id, food_id, quantity, price, snapshot_json)
               VALUES ($1, $2, $3, $4, $5)"#
        )
        .bind(item.order_id)
        .bind(item.food_id)
        .bind(item.quantity)
        .bind(item.price)
        .bind(&item.snapshot_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_by_order_id(&self, order_id: i64) -> Result<(), CustomError> {
        sqlx::query("DELETE FROM order_items WHERE order_id = $1")
            .bind(order_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
