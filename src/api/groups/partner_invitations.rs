use chrono::{DateTime, Duration, Utc};
use ntex::web::{
    self,
    types::{Path, State},
    Responder, ServiceConfig,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::config::AppState;
use crate::errors::CustomError;
use crate::middlewares::auth::UserToken;
use crate::utils::response::ApiResponse;

const INVITATION_TTL_HOURS: i64 = 24;

pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(
        web::resource("/api/groups/partner-invitations")
            .route(web::post().to(create_partner_invitation)),
    );
    cfg.service(
        web::resource("/api/groups/partner-invitations/{token}")
            .route(web::get().to(preview_partner_invitation)),
    );
    cfg.service(
        web::resource("/api/groups/partner-invitations/{token}/accept")
            .route(web::post().to(accept_partner_invitation)),
    );
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartnerInvitationCreated {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartnerInvitationPreview {
    pub inviter_nickname: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PartnerInvitationAccepted {
    pub group_id: i64,
    pub role: String,
}

fn generate_invitation_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn hash_invitation_token(token: &str) -> Result<String, CustomError> {
    if !(32..=128).contains(&token.len()) || !token.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(CustomError::NotFound("邀请已失效".into()));
    }
    Ok(format!("{:x}", Sha256::digest(token.as_bytes())))
}

#[utoipa::path(
    post,
    path = "/api/groups/partner-invitations",
    tag = "双人组",
    responses(
        (status = 200, description = "邀请创建成功", body = PartnerInvitationCreated),
        (status = 400, description = "当前厨房无法继续邀请"),
        (status = 401, description = "未登录")
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_partner_invitation(
    token: UserToken,
    state: State<Arc<AppState>>,
) -> Result<impl Responder, CustomError> {
    let mut tx = state.db_pool.begin().await?;
    sqlx::query_scalar::<_, i64>(
        "SELECT user_id FROM users WHERE user_id = $1 AND status = 'ACTIVE'::user_status_enum FOR UPDATE",
    )
    .bind(token.user_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| CustomError::NotFound("用户不存在".into()))?;

    let target_group_id: Option<i64> = sqlx::query_scalar(
        r#"SELECT agm.group_id
           FROM association_group_members agm
           JOIN association_groups g ON g.group_id = agm.group_id
           WHERE agm.user_id = $1
             AND agm.member_status = 'ACTIVE'::group_member_status_enum
             AND g.status = 'ACTIVE'::user_status_enum
           ORDER BY agm.is_primary DESC, agm.group_id
           LIMIT 1"#,
    )
    .bind(token.user_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(group_id) = target_group_id {
        let member_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM association_group_members
               WHERE group_id = $1
                 AND member_status = 'ACTIVE'::group_member_status_enum"#,
        )
        .bind(group_id)
        .fetch_one(&mut *tx)
        .await?;
        if member_count >= 2 {
            return Err(CustomError::inviter_has_partner("当前厨房已有两位成员"));
        }
    }

    let raw_token = generate_invitation_token();
    let token_hash = hash_invitation_token(&raw_token)?;
    let expires_at = Utc::now() + Duration::hours(INVITATION_TTL_HOURS);

    sqlx::query(
        r#"INSERT INTO partner_invitations
               (token_hash, inviter_user_id, target_group_id, expires_at)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(&token_hash)
    .bind(token.user_id)
    .bind(target_group_id)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(ApiResponse::success(PartnerInvitationCreated {
        token: raw_token,
        expires_at,
    }))
}

#[utoipa::path(
    get,
    path = "/api/groups/partner-invitations/{token}",
    tag = "双人组",
    params(("token" = String, Path, description = "一次性伙伴邀请凭证")),
    responses(
        (status = 200, description = "邀请有效", body = PartnerInvitationPreview),
        (status = 404, description = "邀请无效或已过期")
    ),
    security(("bearer_auth" = []))
)]
pub async fn preview_partner_invitation(
    _user: UserToken,
    state: State<Arc<AppState>>,
    token: Path<String>,
) -> Result<impl Responder, CustomError> {
    let token_hash = hash_invitation_token(&token.into_inner())?;
    let invitation: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT COALESCE(u.nick_name, u.username), pi.expires_at
           FROM partner_invitations pi
           JOIN users u ON u.user_id = pi.inviter_user_id
           WHERE pi.token_hash = $1
             AND pi.consumed_at IS NULL
             AND pi.revoked_at IS NULL
             AND pi.expires_at > NOW()
             AND u.status = 'ACTIVE'::user_status_enum"#,
    )
    .bind(token_hash)
    .fetch_optional(&state.db_pool)
    .await?;

    let (inviter_nickname, expires_at) =
        invitation.ok_or_else(|| CustomError::NotFound("邀请已失效".into()))?;

    Ok(ApiResponse::success(PartnerInvitationPreview {
        inviter_nickname,
        expires_at,
    }))
}

#[utoipa::path(
    post,
    path = "/api/groups/partner-invitations/{token}/accept",
    tag = "双人组",
    params(("token" = String, Path, description = "一次性伙伴邀请凭证")),
    responses(
        (status = 200, description = "加入成功", body = PartnerInvitationAccepted),
        (status = 400, description = "邀请状态或成员状态不允许加入"),
        (status = 404, description = "邀请无效或已过期")
    ),
    security(("bearer_auth" = []))
)]
pub async fn accept_partner_invitation(
    joiner: UserToken,
    state: State<Arc<AppState>>,
    token: Path<String>,
) -> Result<impl Responder, CustomError> {
    let token_hash = hash_invitation_token(&token.into_inner())?;
    let mut tx = state.db_pool.begin().await?;

    let invitation: Option<(
        i64,
        i64,
        Option<i64>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
    )> = sqlx::query_as(
        r#"SELECT invitation_id, inviter_user_id, target_group_id, expires_at,
                  consumed_at, revoked_at
           FROM partner_invitations
           WHERE token_hash = $1
           FOR UPDATE"#,
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await?;

    let (invitation_id, inviter_id, target_group_id, expires_at, consumed_at, revoked_at) =
        invitation.ok_or_else(|| CustomError::NotFound("邀请已失效".into()))?;

    if consumed_at.is_some() || revoked_at.is_some() || expires_at <= Utc::now() {
        return Err(CustomError::NotFound("邀请已失效".into()));
    }
    if inviter_id == joiner.user_id {
        return Err(CustomError::BadRequest("不能接受自己的邀请".into()));
    }

    // 锁住双方用户行，序列化同一用户并发接受多个邀请的场景。
    let locked_users: Vec<i64> = sqlx::query_scalar(
        r#"SELECT user_id
           FROM users
           WHERE user_id IN ($1, $2)
             AND status = 'ACTIVE'::user_status_enum
           ORDER BY user_id
           FOR UPDATE"#,
    )
    .bind(inviter_id)
    .bind(joiner.user_id)
    .fetch_all(&mut *tx)
    .await?;
    if locked_users.len() != 2 {
        return Err(CustomError::NotFound("邀请已失效".into()));
    }

    let joiner_group: Option<i64> = sqlx::query_scalar(
        r#"SELECT group_id
           FROM association_group_members
           WHERE user_id = $1
             AND member_status = 'ACTIVE'::group_member_status_enum
           LIMIT 1"#,
    )
    .bind(joiner.user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if joiner_group.is_some() {
        return Err(CustomError::user_already_in_group("你已在其他组中"));
    }

    let inviter_membership: Option<(i64, String)> = sqlx::query_as(
        r#"SELECT agm.group_id, agm.role_in_group::text
           FROM association_group_members agm
           JOIN association_groups g ON g.group_id = agm.group_id
           WHERE agm.user_id = $1
             AND agm.member_status = 'ACTIVE'::group_member_status_enum
             AND g.status = 'ACTIVE'::user_status_enum
           ORDER BY agm.is_primary DESC, agm.group_id
           LIMIT 1"#,
    )
    .bind(inviter_id)
    .fetch_optional(&mut *tx)
    .await?;

    // 邀请创建后的组归属发生变化时拒绝，避免旧链接把用户加入意外的组。
    if inviter_membership.as_ref().map(|(group_id, _)| *group_id) != target_group_id {
        return Err(CustomError::NotFound("邀请已失效".into()));
    }

    let (group_id, joiner_role) = match inviter_membership {
        None => {
            let group_name = format!("{}和{}的厨房", inviter_id, joiner.user_id);
            let guest_invite_code = Uuid::new_v4().simple().to_string();
            let group_id: i64 = sqlx::query_scalar(
                r#"INSERT INTO association_groups
                       (group_name, group_type, status, invite_code, member_count,
                        buyer_user_id, seller_user_id, level, exp, created_at, updated_at)
                   VALUES
                       ($1, 'PAIR'::group_type_enum, 'ACTIVE'::user_status_enum, $2, 2,
                        $3, $4, 1, 0, NOW(), NOW())
                   RETURNING group_id"#,
            )
            .bind(group_name)
            .bind(guest_invite_code)
            .bind(inviter_id)
            .bind(joiner.user_id)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                r#"INSERT INTO association_group_members
                       (user_id, group_id, role_in_group, is_primary, member_status, joined_at)
                   VALUES
                       ($1, $3, 'ORDERING'::group_member_role_enum, 1,
                        'ACTIVE'::group_member_status_enum, NOW()),
                       ($2, $3, 'RECEIVING'::group_member_role_enum, 0,
                        'ACTIVE'::group_member_status_enum, NOW())"#,
            )
            .bind(inviter_id)
            .bind(joiner.user_id)
            .bind(group_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                r#"UPDATE users
                   SET role = CASE
                       WHEN user_id = $1 THEN 'ORDERING'::user_role_enum
                       ELSE 'RECEIVING'::user_role_enum
                   END,
                   last_role_switch_at = NOW(),
                   updated_at = NOW()
                   WHERE user_id IN ($1, $2)"#,
            )
            .bind(inviter_id)
            .bind(joiner.user_id)
            .execute(&mut *tx)
            .await?;

            (group_id, "RECEIVING".to_string())
        }
        Some((group_id, inviter_role)) => {
            let group_status: Option<String> = sqlx::query_scalar(
                "SELECT status::text FROM association_groups WHERE group_id = $1 FOR UPDATE",
            )
            .bind(group_id)
            .fetch_optional(&mut *tx)
            .await?;
            if group_status.as_deref() != Some("ACTIVE") {
                return Err(CustomError::NotFound("邀请已失效".into()));
            }

            let member_count: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*)
                   FROM association_group_members
                   WHERE group_id = $1
                     AND member_status = 'ACTIVE'::group_member_status_enum"#,
            )
            .bind(group_id)
            .fetch_one(&mut *tx)
            .await?;
            if member_count >= 2 {
                return Err(CustomError::inviter_has_partner("对方已经有厨房了"));
            }

            let (joiner_role, joiner_user_role, joiner_is_buyer) = if inviter_role == "RECEIVING" {
                ("ORDERING", "ORDERING", true)
            } else {
                ("RECEIVING", "RECEIVING", false)
            };

            sqlx::query(
                r#"INSERT INTO association_group_members
                       (user_id, group_id, role_in_group, is_primary, member_status, joined_at)
                   VALUES ($1, $2, $3::group_member_role_enum, 0,
                           'ACTIVE'::group_member_status_enum, NOW())
                   ON CONFLICT (group_id, user_id) DO UPDATE
                   SET role_in_group = EXCLUDED.role_in_group,
                       is_primary = 0,
                       member_status = 'ACTIVE'::group_member_status_enum,
                       joined_at = NOW()"#,
            )
            .bind(joiner.user_id)
            .bind(group_id)
            .bind(joiner_role)
            .execute(&mut *tx)
            .await?;

            if joiner_is_buyer {
                sqlx::query(
                    "UPDATE association_groups \
                     SET buyer_user_id = $1, member_count = 2, updated_at = NOW() \
                     WHERE group_id = $2",
                )
                .bind(joiner.user_id)
                .bind(group_id)
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query(
                    "UPDATE association_groups \
                     SET seller_user_id = $1, member_count = 2, updated_at = NOW() \
                     WHERE group_id = $2",
                )
                .bind(joiner.user_id)
                .bind(group_id)
                .execute(&mut *tx)
                .await?;
            }

            sqlx::query(
                r#"UPDATE users
                   SET role = $2::user_role_enum,
                       last_role_switch_at = NOW(),
                       updated_at = NOW()
                   WHERE user_id = $1"#,
            )
            .bind(joiner.user_id)
            .bind(joiner_user_role)
            .execute(&mut *tx)
            .await?;

            (group_id, joiner_role.to_string())
        }
    };

    let consumed = sqlx::query(
        r#"UPDATE partner_invitations
           SET consumed_at = NOW(), consumed_by = $2
           WHERE invitation_id = $1
             AND consumed_at IS NULL
             AND revoked_at IS NULL"#,
    )
    .bind(invitation_id)
    .bind(joiner.user_id)
    .execute(&mut *tx)
    .await?;
    if consumed.rows_affected() != 1 {
        return Err(CustomError::NotFound("邀请已失效".into()));
    }

    tx.commit().await?;

    // 双方的 UserPublic.group_id / role 均已变化。
    let _ = state.redis_cache.delete_user(&inviter_id.to_string()).await;
    let _ = state
        .redis_cache
        .delete_user(&joiner.user_id.to_string())
        .await;

    let (buyer_id, seller_id) = if joiner_role == "ORDERING" {
        (Some(joiner.user_id), Some(inviter_id))
    } else {
        (Some(inviter_id), Some(joiner.user_id))
    };
    super::routes::push_group_member_change_notice(
        &state.db_pool,
        group_id,
        "joined",
        joiner.user_id,
        buyer_id,
        seller_id,
    )
    .await;

    Ok(ApiResponse::success(PartnerInvitationAccepted {
        group_id,
        role: joiner_role,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitation_token_is_high_entropy_and_hash_is_stable() {
        let token = generate_invitation_token();
        assert_eq!(token.len(), 64);
        assert_ne!(token, hash_invitation_token(&token).unwrap());
        assert_eq!(
            hash_invitation_token(&token).unwrap(),
            hash_invitation_token(&token).unwrap()
        );
    }

    #[test]
    fn malformed_tokens_are_rejected_before_database_lookup() {
        for token in ["short", "contains/slash", "含中文的邀请凭证"] {
            assert!(hash_invitation_token(token).is_err());
        }
    }
}
