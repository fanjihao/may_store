use super::models::{
    WishCreateInput, WishFeedbackInput, WishFeedbackRecord, WishRecord, WishStatusEnum,
    WishUpdateInput,
};
use crate::errors::CustomError;
use sqlx::{PgPool, QueryBuilder, Row};

pub struct WishService;

impl WishService {
    pub async fn list_wishes(
        db: &PgPool,
        group_id: i64,
        user_id: i64,
        limit: i64,
        cursor_condition: Option<(chrono::DateTime<chrono::Utc>, i64)>,
    ) -> Result<(Vec<WishRecord>, i64), CustomError> {
        let visibility_sql = " AND (created_by = $2 OR claimed_by = $2 OR status != 'CLOSED') ";

        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM wishes WHERE group_id = $1 {}",
            visibility_sql
        ))
        .bind(group_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;

        let mut qb: QueryBuilder<sqlx::Postgres> =
            QueryBuilder::new("SELECT * FROM wishes WHERE group_id = ");
        qb.push_bind(group_id);
        qb.push(" AND (created_by = ");
        qb.push_bind(user_id);
        qb.push(" OR claimed_by = ");
        qb.push_bind(user_id);
        qb.push(" OR status != 'CLOSED') ");

        if let Some((dt, w_id)) = cursor_condition {
            qb.push(" AND (created_at, wish_id) < (");
            qb.push_bind(dt);
            qb.push(", ");
            qb.push_bind(w_id);
            qb.push(") ");
        }

        qb.push(" ORDER BY created_at DESC, wish_id DESC ");
        qb.push(" LIMIT ");
        qb.push_bind(limit + 1);

        let rows = qb.build_query_as::<WishRecord>().fetch_all(db).await?;

        Ok((rows, total))
    }

    pub async fn create_wish(
        db: &PgPool,
        user_id: i64,
        data: &WishCreateInput,
    ) -> Result<WishRecord, CustomError> {
        if data.wish_name.trim().is_empty() {
            return Err(CustomError::BadRequest("心愿名称不能为空".into()));
        }
        if data.wish_cost <= 0 {
            return Err(CustomError::BadRequest("心愿积分必须大于0".into()));
        }

        let is_member =
            sqlx::query("SELECT 1 FROM association_group_members WHERE user_id=$1 AND group_id=$2")
                .bind(user_id)
                .bind(data.group_id)
                .fetch_optional(db)
                .await?;

        if is_member.is_none() {
            return Err(CustomError::BadRequest("您不是该组成员".into()));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "INSERT INTO wishes (wish_name, wish_cost, created_by, group_id, status) VALUES ($1,$2,$3,$4,'CREATED') RETURNING *"
        )
        .bind(&data.wish_name)
        .bind(data.wish_cost)
        .bind(user_id)
        .bind(data.group_id)
        .fetch_one(db).await?;

        Ok(rec)
    }

    pub async fn get_wish(
        db: &PgPool,
        wish_id: i64,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        let rec = sqlx::query_as::<_, WishRecord>("SELECT * FROM wishes WHERE wish_id=$1")
            .bind(wish_id)
            .fetch_optional(db)
            .await?;

        let Some(r) = rec else {
            return Err(CustomError::BadRequest("心愿不存在".into()));
        };

        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT * FROM wish_feedbacks WHERE wish_id=$1",
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;

        Ok((r, feedback))
    }

    pub async fn update_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        data: &WishUpdateInput,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        if data.wish_name.is_none() && data.wish_cost.is_none() && data.status.is_none() {
            return Err(CustomError::BadRequest("无修改内容".into()));
        }

        let row = sqlx::query("SELECT created_by FROM wishes WHERE wish_id=$1")
            .bind(wish_id)
            .fetch_optional(db)
            .await?;

        let Some(r) = row else {
            return Err(CustomError::BadRequest("心愿不存在".into()));
        };
        let created_by: i64 = r.get("created_by");
        if created_by != user_id {
            return Err(CustomError::BadRequest("只能修改自己创建的心愿".into()));
        }

        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new("UPDATE wishes SET ");
        let mut first = true;
        if let Some(name) = &data.wish_name {
            if !first {
                qb.push(", ");
            }
            first = false;
            qb.push(" wish_name = ").push_bind(name);
        }
        if let Some(cost) = data.wish_cost {
            if !first {
                qb.push(", ");
            }
            first = false;
            qb.push(" wish_cost = ").push_bind(cost);
        }
        if let Some(st) = &data.status {
            if !first {
                qb.push(", ");
            }
            qb.push(" status = ").push_bind(st);
        }

        qb.push(", updated_at = NOW() WHERE wish_id = ")
            .push_bind(wish_id)
            .push(" RETURNING *");

        let updated = qb.build_query_as::<WishRecord>().fetch_one(db).await?;

        let feedback = sqlx::query_as::<_, WishFeedbackRecord>(
            "SELECT * FROM wish_feedbacks WHERE wish_id=$1",
        )
        .bind(wish_id)
        .fetch_optional(db)
        .await?;

        Ok((updated, feedback))
    }

    pub async fn delete_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let row = sqlx::query("SELECT created_by FROM wishes WHERE wish_id=$1")
            .bind(wish_id)
            .fetch_optional(db)
            .await?;

        let Some(r) = row else {
            return Err(CustomError::BadRequest("心愿不存在".into()));
        };
        let created_by: i64 = r.get("created_by");
        if created_by != user_id {
            return Err(CustomError::BadRequest("只能删除自己创建的心愿".into()));
        }

        let rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status='CLOSED', updated_at=NOW() WHERE wish_id=$1 RETURNING *",
        )
        .bind(wish_id)
        .fetch_one(db)
        .await?;

        Ok(rec)
    }

    pub async fn redeem_wish(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
    ) -> Result<WishRecord, CustomError> {
        let mut tx = db.begin().await?;

        let wish_row = sqlx::query("SELECT * FROM wishes WHERE wish_id=$1 FOR UPDATE")
            .bind(wish_id)
            .fetch_optional(&mut *tx)
            .await?;

        let Some(r) = wish_row else {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("心愿不存在".into()));
        };

        let wish_rec: WishRecord = sqlx::FromRow::from_row(&r)
            .map_err(|e: sqlx::Error| CustomError::InternalServerError(e.to_string()))?;

        if wish_rec.status != WishStatusEnum::CREATED {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("心愿状态不可兑换".into()));
        }

        if wish_rec.created_by == user_id {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("无法兑换自己的心愿".into()));
        }

        let cost = wish_rec.wish_cost;

        let user_row = sqlx::query("SELECT love_point FROM users WHERE user_id=$1 FOR UPDATE")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
        let love_point: i32 = user_row.get("love_point");

        if love_point < cost {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("积分不足".into()));
        }

        let balance_after = love_point - cost;

        sqlx::query("UPDATE users SET love_point=$2 WHERE user_id=$1")
            .bind(user_id)
            .bind(balance_after)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO point_transactions(user_id, amount, type, ref_type, ref_id, balance_after) VALUES($1,$2,'WISH_COST',2,$3,$4)"
        )
        .bind(user_id)
        .bind(-cost)
        .bind(wish_id)
        .bind(balance_after)
        .execute(&mut *tx).await?;

        let updated_rec = sqlx::query_as::<_, WishRecord>(
            "UPDATE wishes SET status='CLAIMED', claimed_by=$1, claimed_at=NOW(), claim_cost=$2, updated_at=NOW() WHERE wish_id=$3 RETURNING *"
        )
        .bind(user_id)
        .bind(cost)
        .bind(wish_id)
        .fetch_one(&mut *tx).await?;

        tx.commit().await?;

        Ok(updated_rec)
    }

    pub async fn submit_feedback(
        db: &PgPool,
        user_id: i64,
        wish_id: i64,
        data: &WishFeedbackInput,
    ) -> Result<(WishRecord, Option<WishFeedbackRecord>), CustomError> {
        let mut tx = db.begin().await?;

        let wish_row = sqlx::query("SELECT * FROM wishes WHERE wish_id=$1 FOR UPDATE")
            .bind(wish_id)
            .fetch_optional(&mut *tx)
            .await?;

        let Some(r) = wish_row else {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("心愿不存在".into()));
        };
        let wish_rec: WishRecord = sqlx::FromRow::from_row(&r)
            .map_err(|e: sqlx::Error| CustomError::InternalServerError(e.to_string()))?;

        if wish_rec.status != WishStatusEnum::CLAIMED && wish_rec.status != WishStatusEnum::FINISHED
        {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest("心愿状态不可反馈".into()));
        }

        if wish_rec.claimed_by != Some(user_id) {
            tx.rollback().await.ok();
            return Err(CustomError::BadRequest(
                "只能为自己兑换的心愿提交反馈".into(),
            ));
        }

        let images_json = data.images.clone().map(sqlx::types::Json);

        let feedback_rec = sqlx::query_as::<_, WishFeedbackRecord>(
            r#"
            INSERT INTO wish_feedbacks (wish_id, user_id, content, images, created_at, updated_at)
            VALUES ($1, $2, $3, $4, NOW(), NOW())
            ON CONFLICT (wish_id)
            DO UPDATE SET content = EXCLUDED.content, images = EXCLUDED.images, updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(wish_id)
        .bind(user_id)
        .bind(&data.content)
        .bind(images_json)
        .fetch_one(&mut *tx)
        .await?;

        let final_wish_rec = if wish_rec.status == WishStatusEnum::CLAIMED {
            sqlx::query_as::<_, WishRecord>(
                "UPDATE wishes SET status='FINISHED', updated_at=NOW() WHERE wish_id=$1 RETURNING *"
            )
            .bind(wish_id)
            .fetch_one(&mut *tx).await?
        } else {
            wish_rec
        };

        tx.commit().await?;

        Ok((final_wish_rec, Some(feedback_rec)))
    }
}
