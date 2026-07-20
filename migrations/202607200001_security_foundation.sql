-- Forward-only production upgrade for the 2026-07 P0 security work.
-- src/v3.sql remains the canonical end-state schema for clean installations.

ALTER TYPE upload_business_ref_enum
    ADD VALUE IF NOT EXISTS 'group_avatar';

CREATE TABLE IF NOT EXISTS partner_invitations (
    invitation_id BIGSERIAL PRIMARY KEY,
    token_hash CHAR(64) NOT NULL UNIQUE,
    inviter_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    target_group_id BIGINT REFERENCES association_groups(group_id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    consumed_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (expires_at > created_at),
    CHECK ((consumed_at IS NULL) = (consumed_by IS NULL))
);

COMMENT ON TABLE partner_invitations IS '伙伴绑定的一次性服务端邀请凭证';
COMMENT ON COLUMN partner_invitations.token_hash IS '原始邀请 token 的 SHA-256 十六进制摘要';

CREATE INDEX IF NOT EXISTS idx_partner_inviter_created
    ON partner_invitations(inviter_user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_partner_active_expiry
    ON partner_invitations(expires_at)
    WHERE consumed_at IS NULL AND revoked_at IS NULL;
