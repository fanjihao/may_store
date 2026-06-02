-- =========================================================
-- File: 002_phase1_foundation.sql
-- Description: Phase 1 - Foundation tables and schema updates
--              For FSD.latest.md compliance
-- Date: 2026-06-02
-- =========================================================

-- ================= NEW ENUM TYPES =================

-- Order type: NORMAL (group order) vs GUEST (guest order)
CREATE TYPE order_type_enum AS ENUM ('NORMAL', 'GUEST');

-- Wish status: 7-state model per FSD
CREATE TYPE wish_status_enum_v2 AS ENUM (
    'DRAFT',           -- 发起人创建草稿
    'NEGOTIATING',     -- 双方协商积分和履约期限
    'CREATED',         -- 双方已确认，进入组内心愿池
    'CLAIMED',         -- 发起人已选择，积分已冻结，待履约
    'FINISHED',        -- 已履约并打卡，积分正式扣减
    'EXPIRED',         -- 履约人逾期未履约，积分已退还
    'CLOSED'           -- 双方关闭或作废
);

-- Point grant status for orders
CREATE TYPE point_grant_status_enum AS ENUM (
    'NONE',            -- 无积分发放
    'PENDING_REVIEW',   -- 待人工审核
    'GRANTED',         -- 已发放
    'REJECTED',        -- 已拒绝
    'REVOKED',         -- 已撤销（事后风控）
    'REJECTED_LIMIT'   -- 超过每日上限被拒绝
);

-- Group exp grant status
CREATE TYPE exp_grant_status_enum AS ENUM (
    'NONE',
    'PENDING_REVIEW',
    'GRANTED',
    'REJECTED',
    'REVOKED',
    'REJECTED_LIMIT'
);

-- Risk status for orders
CREATE TYPE risk_status_enum AS ENUM ('PASS', 'SUSPECT', 'BLOCKED');

-- Wish negotiation action
CREATE TYPE wish_negotiation_action_enum AS ENUM (
    'QUOTE',           -- 报价
    'COUNTER',         -- 还价
    'SET_DEADLINE',    -- 设置期限
    'ACCEPT',          -- 接受
    'REJECT',          -- 拒绝
    'CLOSE'            -- 关闭
);

-- Love point transaction type
CREATE TYPE love_point_tx_type_enum AS ENUM (
    'EARN',            -- 获得积分
    'FREEZE',          -- 冻结积分
    'UNFREEZE',        -- 解冻积分
    'DEDUCT',          -- 扣减积分
    'ADJUST'           -- 人工调整
);

-- Group exp transaction type
CREATE TYPE group_exp_tx_type_enum AS ENUM (
    'EARN',
    'ADJUST',
    'REVOKE'
);

-- Diamond transaction type
CREATE TYPE diamond_tx_type_enum AS ENUM (
    'EARN',
    'CONSUME',
    'ADJUST'
);

-- Guest invitation status
CREATE TYPE guest_invite_status_enum AS ENUM ('ACTIVE', 'EXPIRED', 'REVOKED');

-- Wish quality review status
CREATE TYPE wish_quality_status_enum AS ENUM ('NONE', 'REVIEWED');

-- ================= NEW TABLES =================

-- 1. User Group Points: Love points bound to user_id + group_id
CREATE TABLE user_group_points (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    available_love_point BIGINT NOT NULL DEFAULT 0,
    frozen_love_point BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, group_id)
);
COMMENT ON TABLE user_group_points IS '用户组内爱心积分 - 按user_id+group_id独立计算';
COMMENT ON COLUMN user_group_points.available_love_point IS '可用爱心积分';
COMMENT ON COLUMN user_group_points.frozen_love_point IS '冻结爱心积分（心愿选择后待打卡扣减）';

CREATE INDEX idx_ugp_user_group ON user_group_points(user_id, group_id);

-- 2. Love Point Transactions: Full transaction log with available/frozen tracking
CREATE TABLE love_point_transactions (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    type love_point_tx_type_enum NOT NULL,
    amount BIGINT NOT NULL,
    available_before BIGINT NOT NULL,
    available_after BIGINT NOT NULL,
    frozen_before BIGINT NOT NULL,
    frozen_after BIGINT NOT NULL,
    biz_type VARCHAR(50) NOT NULL,
    biz_id BIGINT,
    idempotency_key VARCHAR(128),
    trace_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE love_point_transactions IS '爱心积分流水 - 所有积分变动必须写流水';
CREATE UNIQUE INDEX idx_lpt_idempotency ON love_point_transactions(idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX idx_lpt_user_group_created ON love_point_transactions(user_id, group_id, created_at);

-- 3. Group Exp Transactions: Group experience with level tracking
CREATE TABLE group_exp_transactions (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    type group_exp_tx_type_enum NOT NULL,
    amount BIGINT NOT NULL,
    exp_before BIGINT NOT NULL,
    exp_after BIGINT NOT NULL,
    level_before INT NOT NULL,
    level_after INT NOT NULL,
    biz_type VARCHAR(50) NOT NULL,
    biz_id BIGINT,
    idempotency_key VARCHAR(128),
    trace_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE group_exp_transactions IS '组经验流水 - 含等级变化';
CREATE UNIQUE INDEX idx_get_idempotency ON group_exp_transactions(idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX idx_get_group_created ON group_exp_transactions(group_id, created_at);

-- 4. Diamond Transactions: Group diamond transactions
CREATE TABLE diamond_transactions (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    type diamond_tx_type_enum NOT NULL,
    amount BIGINT NOT NULL,
    balance_before BIGINT NOT NULL,
    balance_after BIGINT NOT NULL,
    biz_type VARCHAR(50) NOT NULL,
    biz_id BIGINT,
    idempotency_key VARCHAR(128),
    trace_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE diamond_transactions IS '组钻石流水';
CREATE UNIQUE INDEX idx_dt_idempotency ON diamond_transactions(idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX idx_dt_group_created ON diamond_transactions(group_id, created_at);

-- 5. Daily Reward Counters: Daily limits tracking per user+group+day
CREATE TABLE daily_reward_counters (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    user_id BIGINT, -- NULL for group-level exp counter
    love_point_earned BIGINT NOT NULL DEFAULT 0,
    group_exp_earned BIGINT NOT NULL DEFAULT 0,
    normal_order_count INT NOT NULL DEFAULT 0,
    guest_order_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(stat_date, group_id, user_id)
);
COMMENT ON TABLE daily_reward_counters IS '每日奖励上限统计 - 超上限后订单完成但不发放奖励';
CREATE INDEX idx_drc_group_date ON daily_reward_counters(group_id, stat_date);

-- 6. Wish Negotiations: Wish negotiation history
CREATE TABLE wish_negotiations (
    id BIGSERIAL PRIMARY KEY,
    wish_id BIGINT NOT NULL REFERENCES wishes(wish_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    operator_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    operator_role_snapshot VARCHAR(32),
    action wish_negotiation_action_enum NOT NULL,
    cost INT,
    deadline_hours INT,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wish_negotiations IS '心愿协商记录 - 报价/还价/期限/确认/拒绝/关闭';
CREATE INDEX idx_wn_wish ON wish_negotiations(wish_id);
CREATE INDEX idx_wn_group ON wish_negotiations(group_id);

-- 7. Wish Check-ins: Wish fulfillment check-ins
CREATE TABLE wish_checkins (
    id BIGSERIAL PRIMARY KEY,
    wish_id BIGINT NOT NULL REFERENCES wishes(wish_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content TEXT,
    location VARCHAR(256),
    images JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wish_checkins IS '心愿打卡记录 - 发起人提交履约证明';
CREATE INDEX idx_wc_wish ON wish_checkins(wish_id);

-- 8. Guest Invitations: Guest invitation system
CREATE TABLE guest_invitations (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    invite_code VARCHAR(32) NOT NULL UNIQUE,
    created_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    max_uses INT NOT NULL DEFAULT 1,
    used_count INT NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ,
    status guest_invite_status_enum NOT NULL DEFAULT 'ACTIVE',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE guest_invitations IS '做客邀请 - 邀请码机制';
CREATE INDEX idx_gi_code ON guest_invitations(invite_code);
CREATE INDEX idx_gi_group ON guest_invitations(group_id);

-- ================= MODIFY EXISTING TABLES =================

-- Modify association_groups: Add buyer/seller mapping and group level/exp
ALTER TABLE association_groups ADD COLUMN buyer_user_id BIGINT REFERENCES users(user_id);
ALTER TABLE association_groups ADD COLUMN seller_user_id BIGINT REFERENCES users(user_id);
ALTER TABLE association_groups ADD COLUMN level INT NOT NULL DEFAULT 1;
ALTER TABLE association_groups ADD COLUMN exp BIGINT NOT NULL DEFAULT 0;
ALTER TABLE association_groups ADD COLUMN settings JSONB DEFAULT '{}';

COMMENT ON COLUMN association_groups.buyer_user_id IS '当前Buyer用户ID';
COMMENT ON COLUMN association_groups.seller_user_id IS '当前Seller用户ID';
COMMENT ON COLUMN association_groups.level IS '组等级';
COMMENT ON COLUMN association_groups.exp IS '当前等级内经验';
COMMENT ON COLUMN association_groups.settings IS '组配置JSON';

-- Modify orders: Add type, guest fields, risk fields, role snapshots
ALTER TABLE orders ADD COLUMN type order_type_enum NOT NULL DEFAULT 'NORMAL';
ALTER TABLE orders ADD COLUMN creator_role_snapshot VARCHAR(32);
ALTER TABLE orders ADD COLUMN assignee_id BIGINT REFERENCES users(user_id);
ALTER TABLE orders ADD COLUMN assignee_role_snapshot VARCHAR(32);
ALTER TABLE orders ADD COLUMN guest_user_id BIGINT REFERENCES users(user_id);
ALTER TABLE orders ADD COLUMN guest_invite_id BIGINT REFERENCES guest_invitations(id);
ALTER TABLE orders ADD COLUMN guest_remark TEXT;
ALTER TABLE orders ADD COLUMN guest_mark_tags JSONB DEFAULT '[]';
ALTER TABLE orders ADD COLUMN point_grant_status point_grant_status_enum DEFAULT 'NONE';
ALTER TABLE orders ADD COLUMN exp_grant_status exp_grant_status_enum DEFAULT 'NONE';
ALTER TABLE orders ADD COLUMN risk_status risk_status_enum DEFAULT 'PASS';
ALTER TABLE orders ADD COLUMN risk_detail JSONB;
ALTER TABLE orders ADD COLUMN title VARCHAR(128);
ALTER TABLE orders ADD COLUMN content TEXT;
ALTER TABLE orders ADD COLUMN deadline TIMESTAMPTZ;
ALTER TABLE orders ADD COLUMN version INT NOT NULL DEFAULT 1;
ALTER TABLE orders ADD COLUMN accepted_at TIMESTAMPTZ;
ALTER TABLE orders ADD COLUMN completed_at TIMESTAMPTZ;
ALTER TABLE orders ADD COLUMN confirmed_at TIMESTAMPTZ;

COMMENT ON COLUMN orders.type IS '订单类型: NORMAL(组内订单), GUEST(做客订单)';
COMMENT ON COLUMN orders.creator_role_snapshot IS '创建时角色快照';
COMMENT ON COLUMN orders.assignee_id IS '接单人ID';
COMMENT ON COLUMN orders.assignee_role_snapshot IS '接单时角色快照';
COMMENT ON COLUMN orders.guest_user_id IS '做客用户ID';
COMMENT ON COLUMN orders.guest_invite_id IS '做客邀请ID';
COMMENT ON COLUMN orders.guest_remark IS '做客备注、口味偏好、忌口、到访时间';
COMMENT ON COLUMN orders.guest_mark_tags IS '做客订单标记';
COMMENT ON COLUMN orders.point_grant_status IS '爱心积分发放状态';
COMMENT ON COLUMN orders.exp_grant_status IS '组经验发放状态';
COMMENT ON COLUMN orders.risk_status IS '风控状态';
COMMENT ON COLUMN orders.risk_detail IS '命中风控规则详情';
COMMENT ON COLUMN orders.title IS '订单标题';
COMMENT ON COLUMN orders.content IS '订单内容';
COMMENT ON COLUMN orders.deadline IS '订单截止时间';
COMMENT ON COLUMN orders.version IS '乐观锁版本';
COMMENT ON COLUMN orders.accepted_at IS '接单时间';
COMMENT ON COLUMN orders.completed_at IS '完成时间';
COMMENT ON COLUMN orders.confirmed_at IS '确认时间';

-- Modify wishes: Add new fields for 7-state model with natural person binding
ALTER TABLE wishes ADD COLUMN requester_id BIGINT REFERENCES users(user_id);
ALTER TABLE wishes ADD COLUMN fulfiller_id BIGINT REFERENCES users(user_id);
ALTER TABLE wishes ADD COLUMN creator_role_snapshot VARCHAR(32);
ALTER TABLE wishes ADD COLUMN initial_cost INT;
ALTER TABLE wishes ADD COLUMN final_cost INT;
ALTER TABLE wishes ADD COLUMN fulfillment_deadline_hours INT;
ALTER TABLE wishes ADD COLUMN selected_by BIGINT REFERENCES users(user_id);
ALTER TABLE wishes ADD COLUMN selected_at TIMESTAMPTZ;
ALTER TABLE wishes ADD COLUMN fulfillment_due_at TIMESTAMPTZ;
ALTER TABLE wishes ADD COLUMN fulfilled_at TIMESTAMPTZ;
ALTER TABLE wishes ADD COLUMN expired_at TIMESTAMPTZ;
ALTER TABLE wishes ADD COLUMN quality_review_status wish_quality_status_enum DEFAULT 'NONE';
ALTER TABLE wishes ADD COLUMN quality_reviewer_id BIGINT REFERENCES users(user_id);
ALTER TABLE wishes ADD COLUMN quality_remark TEXT;
ALTER TABLE wishes ADD COLUMN diamond_reward INT DEFAULT 0;
ALTER TABLE wishes ADD COLUMN finished_at TIMESTAMPTZ;
ALTER TABLE wishes ADD COLUMN closed_at TIMESTAMPTZ;
ALTER TABLE wishes ADD COLUMN version INT NOT NULL DEFAULT 1;

COMMENT ON COLUMN wishes.requester_id IS '发起人(选择心愿和支付积分的人)';
COMMENT ON COLUMN wishes.fulfiller_id IS '履约人(线下完成心愿的人)';
COMMENT ON COLUMN wishes.creator_role_snapshot IS '创建时角色快照';
COMMENT ON COLUMN wishes.initial_cost IS '初始报价';
COMMENT ON COLUMN wishes.final_cost IS '双方确认后的爱心积分价格';
COMMENT ON COLUMN wishes.fulfillment_deadline_hours IS '履约期限小时数';
COMMENT ON COLUMN wishes.selected_by IS '选择心愿的人(通常等于requester_id)';
COMMENT ON COLUMN wishes.selected_at IS '选择时间';
COMMENT ON COLUMN wishes.fulfillment_due_at IS '履约截止时间';
COMMENT ON COLUMN wishes.fulfilled_at IS '发起人打卡确认履约时间';
COMMENT ON COLUMN wishes.expired_at IS '逾期时间';
COMMENT ON COLUMN wishes.quality_review_status IS '质量查看状态';
COMMENT ON COLUMN wishes.quality_reviewer_id IS '质量查看管理员';
COMMENT ON COLUMN wishes.quality_remark IS '质量备注';
COMMENT ON COLUMN wishes.diamond_reward IS '质量奖励发放钻石';
COMMENT ON COLUMN wishes.finished_at IS '完成时间';
COMMENT ON COLUMN wishes.closed_at IS '关闭时间';
COMMENT ON COLUMN wishes.version IS '乐观锁版本';

-- Update wish status enum - add new statuses (existing CREATED/CLAIMED/FINISHED/CLOSED remain)
ALTER TABLE wishes ALTER COLUMN status TYPE wish_status_enum_v2
    USING CASE status
        WHEN 'CREATED' THEN 'CREATED'::wish_status_enum_v2
        WHEN 'CLAIMED' THEN 'CLAIMED'::wish_status_enum_v2
        WHEN 'FINISHED' THEN 'FINISHED'::wish_status_enum_v2
        WHEN 'CLOSED' THEN 'CLOSED'::wish_status_enum_v2
        ELSE 'DRAFT'::wish_status_enum_v2
    END;

-- ================= UPDATE INDEXES =================

-- Add indexes for new foreign keys
CREATE INDEX idx_orders_assignee ON orders(assignee_id);
CREATE INDEX idx_orders_guest_user ON orders(guest_user_id);
CREATE INDEX idx_orders_guest_invite ON orders(guest_invite_id);
CREATE INDEX idx_wishes_requester ON wishes(requester_id);
CREATE INDEX idx_wishes_fulfiller ON wishes(fulfiller_id);
CREATE INDEX idx_wishes_selected_by ON wishes(selected_by);

-- ================= DATA MIGRATION HELPERS =================

-- Migration view to help migrate old data
CREATE OR REPLACE VIEW v_migration_wish_requester_claimer AS
SELECT
    wish_id,
    created_by AS requester_id,
    claimed_by AS fulfiller_id,
    claim_cost AS final_cost
FROM wishes
WHERE claimed_by IS NOT NULL;

CREATE OR REPLACE VIEW v_migration_order_assignee AS
SELECT
    order_id,
    guest_id AS guest_user_id,
    user_id AS creator_id
FROM orders
WHERE is_guest = TRUE;