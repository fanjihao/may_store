use crate::{errors::CustomError, AppState};
use chrono::{Duration, Utc};
use sqlx::Acquire;
use sqlx::Row;
use std::sync::Arc; // bring trait for row.get

// Runs periodic expiration: any PENDING order older than 30 minutes without receiver_id becomes EXPIRED.
pub async fn run_expiration_worker(state: Arc<AppState>) {
    let db = &state.db_pool;
    loop {
        if let Err(e) = expire_pending(db).await {
            log::warn!("order expiration task error: {}", e);
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await; // run each minute
    }
}

async fn expire_pending(db: &sqlx::Pool<sqlx::Postgres>) -> Result<(), CustomError> {
    let threshold = Utc::now() - Duration::minutes(30);
    let mut conn = db.acquire().await?;

    // Find candidate orders (no receiver, still PENDING, older than threshold)
    let rows = sqlx::query(
        "SELECT order_id FROM orders WHERE status='PENDING' AND receiver_id IS NULL AND created_at < $1"
    )
    .bind(threshold)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let mut ids: Vec<i64> = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.try_get("order_id").unwrap_or_default();
        ids.push(id);
    }

    // Update status & write history in a transaction
    let mut tx = conn.begin().await?;
    for oid in &ids {
        sqlx::query("UPDATE orders SET status='EXPIRED', last_status_change_at=NOW(), updated_at=NOW() WHERE order_id=$1")
            .bind(oid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO order_status_history (order_id, from_status, to_status, changed_by, remark) VALUES ($1,$2,$3,$4,$5)")
            .bind(oid)
            .bind("PENDING")
            .bind("EXPIRED")
            .bind(None::<Option<i64>>) // system
            .bind(Some("自动过期".to_string()))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    // 异步推送过期状态
    for oid in ids {
        let pool_clone = db.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::services::notifications::push_order_status(oid, pool_clone).await {
                log::warn!("order expire push error: {}", e);
            }
        });
    }
    Ok(())
}
