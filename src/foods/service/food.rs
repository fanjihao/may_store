use crate::errors::CustomError;
use crate::foods::models::food::FoodCursor;
use crate::foods::models::food::{
    ApplyStatusEnum, BlindBoxDrawInput, BlindBoxDrawResultOut, BlindBoxFoodSnapshot,
    FoodCreateInput, FoodFilterQuery, FoodOut, FoodRecord, FoodStatusEnum, FoodUpdateInput,
    FoodWithStatsRecord, MarkTypeEnum, SubmitRoleEnum,
};
use crate::foods::models::tag::TagRecord;
use crate::models::pagination::{decode_cursor, encode_cursor, CursorPage};
use crate::users::models::user::UserToken;
use sqlx::{PgPool, QueryBuilder, Row};
use std::collections::HashMap;

pub struct FoodService;
impl FoodService {
    pub async fn create_food(
        db: &PgPool,
        token: &UserToken,
        data: &FoodCreateInput,
    ) -> Result<FoodOut, CustomError> {
        let mut tx = db.begin().await?;

        let role: Option<String> =
            sqlx::query_scalar("SELECT role::text FROM users WHERE user_id=$1")
                .bind(token.user_id as i64)
                .fetch_optional(&mut *tx)
                .await?;

        if role.is_none() {
            return Err(CustomError::BadRequest("用户不存在".into()));
        }
        let role = role.unwrap();

        let submit_role = if role == "RECEIVING" {
            SubmitRoleEnum::ReceivingCreate
        } else {
            SubmitRoleEnum::OrderingApply
        };
        let apply_status = if matches!(submit_role, SubmitRoleEnum::ReceivingCreate) {
            ApplyStatusEnum::APPROVED
        } else {
            ApplyStatusEnum::PENDING
        };
        let food_status = if matches!(apply_status, ApplyStatusEnum::APPROVED) {
            FoodStatusEnum::NORMAL
        } else {
            FoodStatusEnum::AUDITING
        };

        let rec = sqlx::query_as::<_, FoodRecord>(
            "INSERT INTO foods (food_name, food_photo, ingredients, steps, submit_role, apply_status, food_status, created_by, owner_user_id, group_id, apply_remark, tag_id) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11) RETURNING food_id, food_name, food_photo, ingredients, steps, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at, tag_id"
        )
        .bind(&data.food_name)
        .bind(&data.food_photo)
        .bind(&data.ingredients)
        .bind(&data.steps)
        .bind(submit_role)
        .bind(apply_status)
        .bind(food_status)
        .bind(token.user_id as i64)
        .bind(data.group_id.map(|v| v as i64))
        .bind(Option::<String>::None)
        .bind(data.tag_id)
        .fetch_one(&mut *tx)
        .await?;

        let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
            sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
                .bind(tid)
                .fetch_optional(&mut *tx)
                .await?
        } else {
            None
        };

        let marks: Vec<String> = sqlx::query(
            "SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2",
        )
        .bind(token.user_id as i64)
        .bind(rec.food_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .collect();

        let mark_enums = marks
            .into_iter()
            .filter_map(|s| match s.as_str() {
                "LIKE" => Some(MarkTypeEnum::LIKE),
                "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
                _ => None,
            })
            .collect();

        tx.commit().await?;
        Ok(FoodOut::from((rec, tag_row, mark_enums)))
    }

    pub async fn get_foods(
        db: &PgPool,
        token: &UserToken,
        q: &FoodFilterQuery,
    ) -> Result<CursorPage<FoodOut>, CustomError> {
        let limit = q.limit.unwrap_or(50).clamp(1, 200);

        let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "SELECT f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, f.tag_id, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id WHERE f.is_del=0"
        );

        if let Some(kw) = &q.keyword {
            qb.push(" AND f.food_name ILIKE '%' || ")
                .push_bind(kw)
                .push(" || '%'");
        }
        if let Some(fs) = q.food_status {
            qb.push(" AND f.food_status = ").push_bind(fs);
        }
        if let Some(as_) = q.apply_status {
            qb.push(" AND f.apply_status = ").push_bind(as_);
        }
        if let Some(sr) = q.submit_role {
            qb.push(" AND f.submit_role = ").push_bind(sr);
        }
        if let Some(gid) = q.group_id {
            qb.push(" AND f.group_id = ").push_bind(gid);
        } else if let Some(user) = q.created_by {
            qb.push(" AND f.created_by = ").push_bind(user);
        }
        if let Some(tag_id) = q.tag_id {
            qb.push(" AND f.tag_id = ").push_bind(tag_id);
        }
        if q.only_active.unwrap_or(false) {
            qb.push(" AND f.food_status='NORMAL' AND f.apply_status='APPROVED'");
        }

        if let Some(cursor_str) = &q.cursor {
            if let Some(cursor) = decode_cursor::<FoodCursor>(cursor_str) {
                qb.push(" AND (f.created_at, f.food_id) < (");
                qb.push_bind(cursor.created_at);
                qb.push(", ");
                qb.push_bind(cursor.food_id);
                qb.push(")");
            }
        }

        qb.push(" ORDER BY f.created_at DESC, f.food_id DESC LIMIT ")
            .push_bind(limit + 1);

        let mut rows: Vec<FoodWithStatsRecord> = qb.build_query_as().fetch_all(db).await?;
        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.pop();
        }

        let next_cursor = if has_more {
            rows.last().map(|r| {
                encode_cursor(&FoodCursor {
                    created_at: r.created_at,
                    food_id: r.food_id,
                })
            })
        } else {
            None
        };

        let tag_ids: Vec<i64> = rows.iter().filter_map(|r| r.tag_id).collect();
        let mut tags_map: HashMap<i64, TagRecord> = HashMap::new();
        if !tag_ids.is_empty() {
            let tag_rows =
                sqlx::query_as::<_, TagRecord>("SELECT * FROM tags WHERE tag_id = ANY($1)")
                    .bind(&tag_ids)
                    .fetch_all(db)
                    .await?;
            for t in tag_rows {
                tags_map.insert(t.tag_id, t);
            }
        }

        let mut marks_map: HashMap<i64, Vec<MarkTypeEnum>> = HashMap::new();
        if !rows.is_empty() {
            let food_ids: Vec<i64> = rows.iter().map(|r| r.food_id).collect();
            let mark_rows = sqlx::query(
                "SELECT food_id, mark_type::text AS mark_type FROM user_food_mark WHERE user_id=$1 AND food_id = ANY($2)"
            )
            .bind(token.user_id as i64)
            .bind(&food_ids)
            .fetch_all(db)
            .await?;
            for r in mark_rows {
                let fid: i64 = r.get("food_id");
                let mtxt: String = r.get("mark_type");
                if let Some(ev) = match mtxt.as_str() {
                    "LIKE" => Some(MarkTypeEnum::LIKE),
                    "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
                    _ => None,
                } {
                    marks_map.entry(fid).or_default().push(ev);
                }
            }
        }

        let items = rows
            .into_iter()
            .map(|rec| {
                let tag = rec.tag_id.and_then(|tid| tags_map.get(&tid).cloned());
                let mark_vec = marks_map.remove(&rec.food_id).unwrap_or_default();
                FoodOut::from_with_stats(rec, tag, mark_vec)
            })
            .collect();

        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            total: None,
        })
    }

    pub async fn get_food_detail(
        db: &PgPool,
        token: Option<&UserToken>,
        id: i64,
    ) -> Result<FoodOut, CustomError> {
        let rec = sqlx::query_as::<_, FoodWithStatsRecord>(
            "SELECT f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, f.tag_id, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id WHERE f.food_id=$1"
        )
        .bind(id)
        .fetch_optional(db)
        .await?;

        let rec = rec.ok_or_else(|| CustomError::BadRequest("未找到菜品".into()))?;

        let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
            sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
                .bind(tid)
                .fetch_optional(db)
                .await?
        } else {
            None
        };

        let mark_enums = if let Some(t) = token {
            let marks: Vec<String> = sqlx::query(
                "SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2",
            )
            .bind(t.user_id as i64)
            .bind(rec.food_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect();
            marks
                .into_iter()
                .filter_map(|s| match s.as_str() {
                    "LIKE" => Some(MarkTypeEnum::LIKE),
                    "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(FoodOut::from_with_stats(rec, tag_row, mark_enums))
    }

    pub async fn update_food(
        db: &PgPool,
        token: &UserToken,
        id: i64,
        data: &FoodUpdateInput,
    ) -> Result<FoodOut, CustomError> {
        let mut tx = db.begin().await?;

        let mut rec = sqlx::query_as::<_, FoodRecord>(
            "SELECT food_id, food_name, food_photo, ingredients, steps, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at, tag_id FROM foods WHERE food_id=$1 FOR UPDATE"
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| CustomError::BadRequest("菜品不存在".into()))?;

        let role: Option<String> =
            sqlx::query_scalar("SELECT role::text FROM users WHERE user_id=$1")
                .bind(token.user_id as i64)
                .fetch_optional(&mut *tx)
                .await?;
        let role = role.ok_or_else(|| CustomError::BadRequest("用户不存在".into()))?;
        let is_admin = role == "ADMIN";
        let is_receiving = role == "RECEIVING";
        let uid = token.user_id as i64;

        let is_team_member = if let Some(gid) = rec.group_id {
            let member: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM association_group_members WHERE user_id=$1 AND group_id=$2",
            )
            .bind(uid)
            .bind(gid)
            .fetch_optional(&mut *tx)
            .await?;
            member.is_some()
        } else {
            false
        };

        let can_update = match rec.submit_role {
            SubmitRoleEnum::ReceivingCreate => is_admin || is_team_member,
            SubmitRoleEnum::OrderingApply => {
                is_admin || rec.created_by == uid || (is_receiving && rec.created_by != uid)
            }
        };
        if !can_update {
            return Err(CustomError::BadRequest("无权限修改".into()));
        }

        if !is_admin
            && matches!(rec.submit_role, SubmitRoleEnum::OrderingApply)
            && rec.created_by == uid
            && data.apply_status.is_some()
        {
            return Err(CustomError::BadRequest("禁止自审".into()));
        }

        if !is_admin
            && matches!(rec.submit_role, SubmitRoleEnum::OrderingApply)
            && data.apply_status.is_some()
            && !is_receiving
        {
            return Err(CustomError::BadRequest("仅接单角色可审核".into()));
        }

        if let Some(name) = &data.food_name {
            rec.food_name = name.clone();
        }
        if let Some(photo) = &data.food_photo {
            rec.food_photo = Some(photo.clone());
        }
        if let Some(ing) = &data.ingredients {
            rec.ingredients = Some(ing.clone());
        }
        if let Some(st) = &data.steps {
            rec.steps = Some(st.clone());
        }
        if let Some(r) = &data.apply_remark {
            rec.apply_remark = Some(r.clone());
        }
        if let Some(status) = data.food_status {
            rec.food_status = status;
        }
        if let Some(app_status) = data.apply_status {
            rec.apply_status = app_status;
        }
        if let Some(tid) = data.tag_id {
            rec.tag_id = Some(tid);
        }

        sqlx::query(
            "UPDATE foods SET food_name=$2, food_photo=$3, ingredients=$4, steps=$5, apply_remark=$6, food_status=$7, apply_status=$8, tag_id=$9, updated_at=NOW() WHERE food_id=$1"
        )
        .bind(rec.food_id)
        .bind(&rec.food_name)
        .bind(&rec.food_photo)
        .bind(&rec.ingredients)
        .bind(&rec.steps)
        .bind(&rec.apply_remark)
        .bind(rec.food_status)
        .bind(rec.apply_status)
        .bind(rec.tag_id)
        .execute(&mut *tx)
        .await?;

        let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
            sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
                .bind(tid)
                .fetch_optional(&mut *tx)
                .await?
        } else {
            None
        };

        let marks: Vec<String> = sqlx::query(
            "SELECT mark_type::text FROM user_food_mark WHERE user_id=$1 AND food_id=$2",
        )
        .bind(uid)
        .bind(rec.food_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .collect();

        let mark_enums = marks
            .into_iter()
            .filter_map(|s| match s.as_str() {
                "LIKE" => Some(MarkTypeEnum::LIKE),
                "NOT_RECOMMEND" => Some(MarkTypeEnum::NotRecommend),
                _ => None,
            })
            .collect();

        tx.commit().await?;
        Ok(FoodOut::from((rec, tag_row, mark_enums)))
    }

    pub async fn mark_food(
        db: &PgPool,
        user_id: i64,
        food_id: i64,
        mark_type: MarkTypeEnum,
    ) -> Result<(), CustomError> {
        sqlx::query("INSERT INTO user_food_mark (user_id, food_id, mark_type) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(food_id)
            .bind(mark_type)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn unmark_food(
        db: &PgPool,
        user_id: i64,
        food_id: i64,
        mark_type: MarkTypeEnum,
    ) -> Result<(), CustomError> {
        sqlx::query("DELETE FROM user_food_mark WHERE user_id=$1 AND food_id=$2 AND mark_type=$3")
            .bind(user_id)
            .bind(food_id)
            .bind(mark_type)
            .execute(db)
            .await?;
        Ok(())
    }

    pub async fn get_marked_foods(
        db: &PgPool,
        token: &UserToken,
        q: &FoodFilterQuery,
    ) -> Result<CursorPage<FoodOut>, CustomError> {
        let limit = q.limit.unwrap_or(50).clamp(1, 200);

        let mut qb = QueryBuilder::<sqlx::Postgres>::new(
            "SELECT f.food_id, f.food_name, f.food_photo, f.ingredients, f.steps, f.food_status, f.submit_role, f.apply_status, f.apply_remark, f.created_by, f.owner_user_id, f.group_id, f.approved_at, f.approved_by, f.is_del, f.created_at, f.updated_at, f.tag_id, fs.total_order_count, fs.completed_order_count, fs.last_order_time, fs.last_complete_time \
             FROM foods f LEFT JOIN food_stats fs ON fs.food_id=f.food_id JOIN user_food_mark m ON f.food_id=m.food_id WHERE m.user_id="
        );
        qb.push_bind(token.user_id as i64);
        qb.push(" AND m.mark_type='LIKE'");

        if let Some(cursor_str) = &q.cursor {
            if let Some(cursor) = decode_cursor::<FoodCursor>(cursor_str) {
                qb.push(" AND (f.created_at, f.food_id) < (");
                qb.push_bind(cursor.created_at);
                qb.push(", ");
                qb.push_bind(cursor.food_id);
                qb.push(")");
            }
        }

        qb.push(" ORDER BY f.created_at DESC, f.food_id DESC LIMIT ")
            .push_bind(limit + 1);

        let mut rows: Vec<FoodWithStatsRecord> = qb.build_query_as().fetch_all(db).await?;

        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.pop();
        }

        let next_cursor = if has_more {
            rows.last().map(|r| {
                encode_cursor(&FoodCursor {
                    created_at: r.created_at,
                    food_id: r.food_id,
                })
            })
        } else {
            None
        };

        let mut items = Vec::new();
        for rec in rows {
            let tag_row: Option<TagRecord> = if let Some(tid) = rec.tag_id {
                sqlx::query_as("SELECT * FROM tags WHERE tag_id=$1")
                    .bind(tid)
                    .fetch_optional(db)
                    .await?
            } else {
                None
            };
            items.push(FoodOut::from_with_stats(
                rec,
                tag_row,
                vec![MarkTypeEnum::LIKE],
            ));
        }

        Ok(CursorPage {
            items,
            next_cursor,
            has_more,
            total: None,
        })
    }

    pub async fn draw_blind_box(
        db: &PgPool,
        token: &UserToken,
        data: &BlindBoxDrawInput,
    ) -> Result<BlindBoxDrawResultOut, CustomError> {
        let group_id = if let Some(gid) = data.group_id {
            gid
        } else {
            sqlx::query_scalar("SELECT group_id FROM association_group_members WHERE user_id=$1 ORDER BY created_at LIMIT 1")
                .bind(token.user_id as i64)
                .fetch_optional(db)
                .await?
                .unwrap_or(0)
        };
        if group_id == 0 {
            return Err(CustomError::BadRequest("未找到绑定组".into()));
        }

        let limit_each = data.limit_each.unwrap_or(1) as i64;
        let mut results: Vec<BlindBoxFoodSnapshot> = Vec::new();

        for tag_id in &data.tag_ids {
            let rows = sqlx::query(
                "SELECT food_id, food_name, food_photo FROM foods WHERE group_id=$1 AND tag_id=$2 AND food_status='NORMAL' AND apply_status='APPROVED' ORDER BY random() LIMIT $3"
            )
            .bind(group_id as i64)
            .bind(*tag_id)
            .bind(limit_each)
            .fetch_all(db)
            .await?;

            for r in rows {
                results.push(BlindBoxFoodSnapshot {
                    food_id: r.get::<i64, _>(0),
                    food_name: r.get::<String, _>(1),
                    food_photo: r.get::<Option<String>, _>(2),
                });
            }
        }

        Ok(BlindBoxDrawResultOut {
            results,
            requested_tags: data.tag_ids.clone(),
        })
    }

    pub async fn delete_food(db: &PgPool, token: &UserToken, id: i64) -> Result<(), CustomError> {
        let mut tx = db.begin().await?;

        let rec_opt = sqlx::query_as::<_, FoodRecord>(
            "SELECT food_id, food_name, food_photo, ingredients, steps, food_status, submit_role, apply_status, apply_remark, created_by, owner_user_id, group_id, approved_at, approved_by, is_del, created_at, updated_at, tag_id FROM foods WHERE food_id=$1 AND is_del=0 FOR UPDATE"
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let rec = match rec_opt {
            Some(r) => r,
            None => return Err(CustomError::NotFound("菜品不存在".into())),
        };

        let role: Option<String> =
            sqlx::query_scalar("SELECT role::text FROM users WHERE user_id=$1")
                .bind(token.user_id as i64)
                .fetch_optional(&mut *tx)
                .await?;
        let role = role.unwrap_or_default();
        let is_admin = role == "ADMIN";
        let is_team_member = if let Some(gid) = rec.group_id {
            let member: Option<i32> = sqlx::query_scalar(
                "SELECT 1 FROM association_group_members WHERE user_id=$1 AND group_id=$2",
            )
            .bind(token.user_id as i64)
            .bind(gid)
            .fetch_optional(&mut *tx)
            .await?;
            member.is_some()
        } else {
            false
        };

        let can_delete = is_admin || rec.created_by == (token.user_id as i64) || is_team_member;
        if !can_delete {
            return Err(CustomError::BadRequest("无权限删除该菜品".into()));
        }

        sqlx::query("UPDATE foods SET is_del=1 WHERE food_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
