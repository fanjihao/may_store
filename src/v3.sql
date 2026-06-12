-- =========================================================
-- File: v3.sql
-- DB: PostgreSQL 15+
-- Date: 2026-06-03
-- Description: FSD.latest.md compliant unified schema
--              Complete database design for 心愿菜单 MVP
-- =========================================================

-- =========================================================
-- DROP existing objects (idempotent re-run support)
-- =========================================================

-- Drop tables (CASCADE removes dependent objects automatically)
DROP TABLE IF EXISTS support_tickets CASCADE;
DROP TABLE IF EXISTS admin_users CASCADE;
DROP TABLE IF EXISTS group_configs CASCADE;
DROP TABLE IF EXISTS global_configs CASCADE;
DROP TABLE IF EXISTS group_achievements CASCADE;
DROP TABLE IF EXISTS user_achievements CASCADE;
DROP TABLE IF EXISTS achievements CASCADE;
DROP TABLE IF EXISTS audit_logs CASCADE;
DROP TABLE IF EXISTS upload_files CASCADE;
DROP TABLE IF EXISTS notifications CASCADE;
DROP TABLE IF EXISTS group_footprint_capacity CASCADE;
DROP TABLE IF EXISTS footprints CASCADE;
DROP TABLE IF EXISTS sign_in_records CASCADE;
DROP TABLE IF EXISTS group_invites CASCADE;
DROP TABLE IF EXISTS memorial_day CASCADE;
DROP TABLE IF EXISTS event_log CASCADE;
DROP TABLE IF EXISTS achievement_definitions CASCADE;
DROP TABLE IF EXISTS user_record CASCADE;
DROP TABLE IF EXISTS record_group CASCADE;
DROP TABLE IF EXISTS group_diamond_flow CASCADE;
DROP TABLE IF EXISTS diamond_flow CASCADE;
DROP TABLE IF EXISTS user_diamond CASCADE;
DROP TABLE IF EXISTS group_point_configs CASCADE;
DROP TABLE IF EXISTS wx_subscription_templates CASCADE;
DROP TABLE IF EXISTS food_stats CASCADE;
DROP TABLE IF EXISTS cart_items CASCADE;
DROP TABLE IF EXISTS carts CASCADE;
DROP TABLE IF EXISTS feedback CASCADE;
DROP TABLE IF EXISTS user_message_state CASCADE;
DROP TABLE IF EXISTS messages CASCADE;
DROP TABLE IF EXISTS message_categories CASCADE;
DROP TABLE IF EXISTS lottery_draw_results CASCADE;
DROP TABLE IF EXISTS lottery_draws CASCADE;
DROP TABLE IF EXISTS order_ratings CASCADE;
DROP TABLE IF EXISTS sweet_talks CASCADE;
DROP TABLE IF EXISTS sign_records CASCADE;
DROP TABLE IF EXISTS point_transactions CASCADE;
DROP TABLE IF EXISTS daily_reward_counters CASCADE;
DROP TABLE IF EXISTS diamond_transactions CASCADE;
DROP TABLE IF EXISTS group_exp_transactions CASCADE;
DROP TABLE IF EXISTS love_point_transactions CASCADE;
DROP TABLE IF EXISTS wish_feedbacks CASCADE;
DROP TABLE IF EXISTS wish_checkins CASCADE;
DROP TABLE IF EXISTS wish_negotiations CASCADE;
DROP TABLE IF EXISTS wishes CASCADE;
DROP TABLE IF EXISTS order_status_history CASCADE;
DROP TABLE IF EXISTS order_items CASCADE;
DROP TABLE IF EXISTS orders CASCADE;
DROP TABLE IF EXISTS food_audit_logs CASCADE;
DROP TABLE IF EXISTS user_food_mark CASCADE;
DROP TABLE IF EXISTS ingredients CASCADE;
DROP TABLE IF EXISTS foods CASCADE;
DROP TABLE IF EXISTS tags CASCADE;
DROP TABLE IF EXISTS guest_invitations CASCADE;
DROP TABLE IF EXISTS association_group_requests CASCADE;
DROP TABLE IF EXISTS user_group_points CASCADE;
DROP TABLE IF EXISTS association_group_members CASCADE;
DROP TABLE IF EXISTS association_groups CASCADE;
DROP TABLE IF EXISTS users CASCADE;

-- Drop enum types
DROP TYPE IF EXISTS login_method_enum CASCADE;
DROP TYPE IF EXISTS gender_enum CASCADE;
DROP TYPE IF EXISTS mark_type_enum CASCADE;
DROP TYPE IF EXISTS cart_status_enum CASCADE;
DROP TYPE IF EXISTS feedback_status_enum CASCADE;
DROP TYPE IF EXISTS message_status_enum CASCADE;
DROP TYPE IF EXISTS lottery_success_enum CASCADE;
DROP TYPE IF EXISTS point_tx_type_enum CASCADE;
DROP TYPE IF EXISTS event_status_enum CASCADE;
DROP TYPE IF EXISTS guest_invite_status_enum CASCADE;
DROP TYPE IF EXISTS diamond_tx_type_enum CASCADE;
DROP TYPE IF EXISTS group_exp_tx_type_enum CASCADE;
DROP TYPE IF EXISTS love_point_tx_type_enum CASCADE;
DROP TYPE IF EXISTS config_category_enum CASCADE;
DROP TYPE IF EXISTS admin_role_enum CASCADE;
DROP TYPE IF EXISTS achievement_category_enum CASCADE;
DROP TYPE IF EXISTS audit_action_enum CASCADE;
DROP TYPE IF EXISTS upload_business_ref_enum CASCADE;
DROP TYPE IF EXISTS content_check_status_enum CASCADE;
DROP TYPE IF EXISTS notification_type_enum CASCADE;
DROP TYPE IF EXISTS group_invite_status_enum CASCADE;
DROP TYPE IF EXISTS food_status_v2_enum CASCADE;
DROP TYPE IF EXISTS wish_quality_level_enum CASCADE;
DROP TYPE IF EXISTS wish_quality_status_enum CASCADE;
DROP TYPE IF EXISTS wish_negotiation_action_enum CASCADE;
DROP TYPE IF EXISTS wish_status_enum CASCADE;
DROP TYPE IF EXISTS risk_status_enum CASCADE;
DROP TYPE IF EXISTS exp_grant_status_enum CASCADE;
DROP TYPE IF EXISTS point_grant_status_enum CASCADE;
DROP TYPE IF EXISTS order_type_enum CASCADE;
DROP TYPE IF EXISTS order_status_enum CASCADE;
DROP TYPE IF EXISTS apply_status_enum CASCADE;
DROP TYPE IF EXISTS submit_role_enum CASCADE;
DROP TYPE IF EXISTS food_status_enum CASCADE;
DROP TYPE IF EXISTS group_member_status_enum CASCADE;
DROP TYPE IF EXISTS group_member_role_enum CASCADE;
DROP TYPE IF EXISTS group_type_enum CASCADE;
DROP TYPE IF EXISTS user_role_enum CASCADE;
DROP TYPE IF EXISTS user_status_enum CASCADE;

-- ================= ENUM TYPE DEFINITIONS =================

-- User status
CREATE TYPE user_status_enum AS ENUM ('ACTIVE', 'BANNED', 'DELETED');

-- User role in group
CREATE TYPE user_role_enum AS ENUM ('ORDERING', 'RECEIVING', 'ADMIN');

-- Group type
CREATE TYPE group_type_enum AS ENUM ('PAIR', 'FAMILY', 'TEAM');

-- Group member role
CREATE TYPE group_member_role_enum AS ENUM ('ORDERING', 'RECEIVING', 'ADMIN');

-- Group member status
CREATE TYPE group_member_status_enum AS ENUM ('ACTIVE', 'LEFT');

-- Food status
CREATE TYPE food_status_enum AS ENUM ('NORMAL', 'OFF', 'AUDITING', 'REJECTED');

-- Submit role
CREATE TYPE submit_role_enum AS ENUM ('ORDERING_APPLY', 'RECEIVING_CREATE');

-- Apply status
CREATE TYPE apply_status_enum AS ENUM ('PENDING', 'APPROVED', 'REJECTED');

-- Order status: 6 core states + 3 terminal states per FSD.latest.md
-- CREATED → ACCEPTED → PRODUCTION_COMPLETED → CONFIRMED_COMPLETED
--                                                → CONFIRMED_INCOMPLETE
-- CREATED → REJECTED / CANCELLED
-- CREATED/ACCEPTED → TIMEOUT
CREATE TYPE order_status_enum AS ENUM (
    'CREATED',                  -- 待接单
    'ACCEPTED',                -- 已接单
    'PRODUCTION_COMPLETED',    -- 生产完成
    'CONFIRMED_COMPLETED',      -- 确认完成
    'CONFIRMED_INCOMPLETE',     -- 确认未完成
    'REJECTED',                -- 已拒绝
    'CANCELLED',               -- 已取消
    'TIMEOUT'                  -- 超时
);

-- Order type: NORMAL (group order) vs GUEST (guest order)
CREATE TYPE order_type_enum AS ENUM ('NORMAL', 'GUEST');

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

-- Wish status: 7-state model per FSD.latest.md
-- DRAFT → NEGOTIATING → CREATED → CLAIMED → FINISHED
--                                         → EXPIRED
-- 任意非终态 → 双方协商一致关闭 → CLOSED
CREATE TYPE wish_status_enum AS ENUM (
    'DRAFT',           -- 发起人创建草稿
    'NEGOTIATING',     -- 双方协商积分和履约期限
    'CREATED',         -- 双方已确认，进入组内心愿池
    'CLAIMED',         -- 发起人已选择，积分已冻结，待履约
    'FINISHED',        -- 已履约并打卡，积分正式扣减
    'EXPIRED',         -- 履约人逾期未履约，积分已退还
    'CLOSED'           -- 双方关闭或作废
);

-- Wish negotiation action
CREATE TYPE wish_negotiation_action_enum AS ENUM (
    'QUOTE',           -- 报价
    'COUNTER',         -- 还价
    'SET_DEADLINE',    -- 设置期限
    'ACCEPT',          -- 接受
    'REJECT',          -- 拒绝
    'CLOSE'            -- 关闭
);

-- Wish quality review status (FSD §11.21)
CREATE TYPE wish_quality_status_enum AS ENUM ('NONE', 'REVIEWED');

-- Wish quality level (FSD §7.9)
CREATE TYPE wish_quality_level_enum AS ENUM ('NONE', 'NORMAL', 'GOOD', 'EXCELLENT');

-- Food status (FSD §5.2)
CREATE TYPE food_status_v2_enum AS ENUM ('ACTIVE', 'HIDDEN', 'DELETED');

-- Group invite status (FSD §11.14)
CREATE TYPE group_invite_status_enum AS ENUM ('ACTIVE', 'EXPIRED', 'EXHAUSTED');

-- Notification type (FSD §10.1)
CREATE TYPE notification_type_enum AS ENUM ('ORDER', 'WISH', 'SIGN_IN', 'SYSTEM');

-- Content moderation status (FSD §11.19)
CREATE TYPE content_check_status_enum AS ENUM ('PENDING', 'PASS', 'REJECTED');

-- Upload file business ref type (FSD §11.19)
CREATE TYPE upload_business_ref_enum AS ENUM ('food', 'footprint', 'checkin', 'avatar');

-- Audit log action type (FSD §11.20)
CREATE TYPE audit_action_enum AS ENUM (
    'USER_BAN', 'USER_UNBAN', 'CONFIG_UPDATE',
    'POINT_COMPENSATE', 'DIAMOND_COMPENSATE',
    'ORDER_REWARD_REVIEW', 'WISH_QUALITY_REWARD',
    'WISH_CLOSE', 'ORDER_CANCEL', 'ORDER_FORCE_TIMEOUT',
    'ROLE_SWAP_ADMIN', 'OTHER'
);

-- Achievement category (FSD §11.21)
CREATE TYPE achievement_category_enum AS ENUM ('USER', 'GROUP');

-- Admin role (FSD §11.24)
CREATE TYPE admin_role_enum AS ENUM ('SUPER_ADMIN', 'OPS', 'RISK_REVIEWER');

-- Config category (FSD §11.22)
CREATE TYPE config_category_enum AS ENUM (
    'REWARDS', 'SIGN_IN', 'WISH', 'ORDER',
    'RISK', 'UPLOAD', 'GENERAL'
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

-- Event log status
CREATE TYPE event_status_enum AS ENUM (
    'PENDING',
    'PROCESSING',
    'DONE',
    'FAILED',
    'DEAD'
);

-- Point transaction type (legacy)
CREATE TYPE point_tx_type_enum AS ENUM (
    'ORDER_REWARD',
    'FINISH_REWARD',
    'WISH_COST',
    'ORDER_RATING',
    'ADMIN_ADJUST',
    'LOTTERY_REWARD',
    'SIGN_IN_REWARD',
    'SWEET_TALK_REWARD',
    'OTHER'
);

-- Lottery success
CREATE TYPE lottery_success_enum AS ENUM ('SUCCESS', 'FAIL');

-- Message status
CREATE TYPE message_status_enum AS ENUM ('ACTIVE', 'REVOKED');

-- Feedback status
CREATE TYPE feedback_status_enum AS ENUM ('NEW', 'PROCESSING', 'CLOSED');

-- Cart status
CREATE TYPE cart_status_enum AS ENUM ('ACTIVE', 'SETTLED', 'CLEARED');

-- Mark type
CREATE TYPE mark_type_enum AS ENUM ('LIKE', 'NOT_RECOMMEND');

-- Gender
CREATE TYPE gender_enum AS ENUM ('MALE', 'FEMALE', 'OTHER', 'UNKNOWN');

-- Login method
CREATE TYPE login_method_enum AS ENUM ('PASSWORD', 'PHONE_CODE', 'OAUTH', 'MIXED', 'WEIXIN');

-- ================= USERS =================
CREATE TABLE users (
    user_id BIGSERIAL PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    nick_name VARCHAR(64),
    email VARCHAR(128),
    role user_role_enum NOT NULL DEFAULT 'ORDERING',
    love_point INT NOT NULL DEFAULT 0,
    diamond INT NOT NULL DEFAULT 0,
    avatar VARCHAR(256) NOT NULL DEFAULT 'https://store.impeter.fun/default-avatar.png',
    phone VARCHAR(32),
    open_id VARCHAR(128) UNIQUE,
    status user_status_enum NOT NULL DEFAULT 'ACTIVE',
    password_hash VARCHAR(255),
    password_algo VARCHAR(32),
    gender gender_enum NOT NULL DEFAULT 'UNKNOWN',
    birthday DATE,
    username_change BOOLEAN NOT NULL DEFAULT FALSE,
    login_method login_method_enum NOT NULL DEFAULT 'PASSWORD',
    last_login_at TIMESTAMPTZ,
    password_updated_at TIMESTAMPTZ,
    is_temp_password BOOLEAN NOT NULL DEFAULT FALSE,
    push_id VARCHAR(255),
    last_role_switch_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE users IS '用户信息';
COMMENT ON COLUMN users.user_id IS '用户主键ID';
COMMENT ON COLUMN users.username IS '用户名（唯一）';
COMMENT ON COLUMN users.nick_name IS '昵称';
COMMENT ON COLUMN users.email IS '邮箱地址';
COMMENT ON COLUMN users.role IS '角色：ORDERING下单/RECEIVING接单/ADMIN管理';
COMMENT ON COLUMN users.love_point IS '爱心积分(仅通过订单获取)';
COMMENT ON COLUMN users.diamond IS '钻石(通过签到获取)';
COMMENT ON COLUMN users.avatar IS '头像URL';
COMMENT ON COLUMN users.phone IS '手机号';
COMMENT ON COLUMN users.open_id IS '微信绑定openid';
COMMENT ON COLUMN users.status IS '状态：ACTIVE/BANNED/DELETED';
COMMENT ON COLUMN users.password_hash IS '哈希后的密码（永不存明文）';
COMMENT ON COLUMN users.password_algo IS '密码哈希算法标识';
COMMENT ON COLUMN users.gender IS '性别';
COMMENT ON COLUMN users.birthday IS '生日';
COMMENT ON COLUMN users.username_change IS '用户名是否修改过';
COMMENT ON COLUMN users.login_method IS '最近登录方式';
COMMENT ON COLUMN users.last_login_at IS '最后登录时间';
COMMENT ON COLUMN users.password_updated_at IS '最近密码更新时间';
COMMENT ON COLUMN users.is_temp_password IS '是否临时密码需修改';
COMMENT ON COLUMN users.push_id IS '推送ID用于消息通知';
COMMENT ON COLUMN users.last_role_switch_at IS '最近一次下单/接单角色对换时间';
COMMENT ON COLUMN users.created_at IS '创建时间';
COMMENT ON COLUMN users.updated_at IS '更新时间';
CREATE INDEX idx_users_status ON users(status);
CREATE INDEX idx_users_phone ON users(phone);
CREATE INDEX idx_users_open_id ON users(open_id);
CREATE INDEX idx_users_login_method ON users(login_method);
CREATE INDEX idx_users_last_login ON users(last_login_at);

-- ================= ASSOCIATION GROUPS =================
CREATE TABLE association_groups (
    group_id BIGSERIAL PRIMARY KEY,
    group_name VARCHAR(128),
    group_type group_type_enum NOT NULL DEFAULT 'PAIR',
    status user_status_enum NOT NULL DEFAULT 'ACTIVE',
    invite_code VARCHAR(32),
    diamond INT NOT NULL DEFAULT 0,
    footprint_capacity INT NOT NULL DEFAULT 10,
    footprint_count INT NOT NULL DEFAULT 0,
    -- FSD v2 fields per FSD.latest.md
    buyer_user_id BIGINT REFERENCES users(user_id),
    seller_user_id BIGINT REFERENCES users(user_id),
    level INT NOT NULL DEFAULT 1,
    exp BIGINT NOT NULL DEFAULT 0,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE association_groups IS '双人组';
COMMENT ON COLUMN association_groups.group_id IS '组ID主键';
COMMENT ON COLUMN association_groups.group_name IS '组名称';
COMMENT ON COLUMN association_groups.group_type IS '组类型：PAIR/FAMILY/TEAM';
COMMENT ON COLUMN association_groups.status IS '状态：ACTIVE/BANNED/DELETED';
COMMENT ON COLUMN association_groups.invite_code IS '做客邀请码';
COMMENT ON COLUMN association_groups.diamond IS '组钻石余额';
COMMENT ON COLUMN association_groups.footprint_capacity IS '足迹全局容量';
COMMENT ON COLUMN association_groups.footprint_count IS '当前足迹记录总数';
COMMENT ON COLUMN association_groups.buyer_user_id IS '当前Buyer用户ID';
COMMENT ON COLUMN association_groups.seller_user_id IS '当前Seller用户ID';
COMMENT ON COLUMN association_groups.level IS '组等级';
COMMENT ON COLUMN association_groups.exp IS '当前等级内经验';
COMMENT ON COLUMN association_groups.settings IS '组配置JSON';
COMMENT ON COLUMN association_groups.created_at IS '创建时间';
COMMENT ON COLUMN association_groups.updated_at IS '更新时间';
CREATE INDEX idx_groups_buyer ON association_groups(buyer_user_id);
CREATE INDEX idx_groups_seller ON association_groups(seller_user_id);

-- ================= ASSOCIATION GROUP MEMBERS =================
-- Note: Table named association_group_members to match code conventions (FSD 11.4 uses group_members)
CREATE TABLE association_group_members (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    role_in_group group_member_role_enum NOT NULL,
    member_status group_member_status_enum NOT NULL DEFAULT 'ACTIVE',
    is_primary SMALLINT NOT NULL DEFAULT 0,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (group_id, user_id)
);
COMMENT ON TABLE association_group_members IS '组成员';
COMMENT ON COLUMN association_group_members.id IS '成员记录主键';
COMMENT ON COLUMN association_group_members.group_id IS '关联组ID';
COMMENT ON COLUMN association_group_members.user_id IS '用户ID';
COMMENT ON COLUMN association_group_members.role_in_group IS '组内角色';
COMMENT ON COLUMN association_group_members.member_status IS '成员状态：ACTIVE/LEFT';
COMMENT ON COLUMN association_group_members.is_primary IS '是否主成员标记';
COMMENT ON COLUMN association_group_members.joined_at IS '添加时间';
CREATE INDEX idx_gm_user_status ON association_group_members(user_id, member_status);
CREATE INDEX idx_gm_group_status ON association_group_members(group_id, member_status);

-- ================= USER GROUP POINTS =================
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

-- ================= ASSOCIATION GROUP REQUESTS =================
CREATE TABLE association_group_requests (
    request_id BIGSERIAL PRIMARY KEY,
    requester_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    target_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    status SMALLINT NOT NULL DEFAULT 0,
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    handled_at TIMESTAMPTZ
);
COMMENT ON TABLE association_group_requests IS '绑定申请记录';
COMMENT ON COLUMN association_group_requests.request_id IS '申请记录主键';
COMMENT ON COLUMN association_group_requests.requester_id IS '发起者用户ID';
COMMENT ON COLUMN association_group_requests.target_user_id IS '目标用户ID';
COMMENT ON COLUMN association_group_requests.status IS '申请状态：0待处理 1同意 2拒绝';
COMMENT ON COLUMN association_group_requests.remark IS '备注/理由';
COMMENT ON COLUMN association_group_requests.created_at IS '创建时间';
COMMENT ON COLUMN association_group_requests.handled_at IS '处理时间';
CREATE INDEX idx_agr_target_status ON association_group_requests(target_user_id, status);

-- ================= GUEST INVITATIONS =================
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

-- ================= FOODS =================
CREATE TABLE tags (
    tag_id BIGSERIAL PRIMARY KEY,
    tag_name VARCHAR(64) NOT NULL,
    icon VARCHAR(256),
    group_id BIGINT REFERENCES association_groups(group_id) ON DELETE CASCADE,
    sort INT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tag_name, group_id)
);
COMMENT ON TABLE tags IS '菜品标签';
COMMENT ON COLUMN tags.tag_id IS '标签主键ID';
COMMENT ON COLUMN tags.tag_name IS '标签名称唯一';
COMMENT ON COLUMN tags.icon IS '标签图标URL';
COMMENT ON COLUMN tags.sort IS '排序值';
COMMENT ON COLUMN tags.created_at IS '创建时间';

CREATE TABLE foods (
    food_id BIGSERIAL PRIMARY KEY,
    food_name VARCHAR(128) NOT NULL,
    food_photo VARCHAR(256),
    tag_id BIGINT REFERENCES tags(tag_id) ON DELETE SET NULL,
    ingredients TEXT,
    steps TEXT,
    food_status food_status_enum NOT NULL DEFAULT 'NORMAL',
    submit_role submit_role_enum NOT NULL DEFAULT 'ORDERING_APPLY',
    apply_status apply_status_enum NOT NULL DEFAULT 'PENDING',
    apply_remark VARCHAR(255),
    created_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    owner_user_id BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    group_id BIGINT REFERENCES association_groups(group_id) ON DELETE SET NULL,
    approved_at TIMESTAMPTZ,
    approved_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    is_del SMALLINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE foods IS '菜品（含申请与审核）';
COMMENT ON COLUMN foods.food_id IS '菜品主键ID';
COMMENT ON COLUMN foods.food_name IS '菜品名称';
COMMENT ON COLUMN foods.food_photo IS '菜品图片URL';
COMMENT ON COLUMN foods.tag_id IS '标签ID';
COMMENT ON COLUMN foods.ingredients IS '配料/食材';
COMMENT ON COLUMN foods.steps IS '制作步骤';
COMMENT ON COLUMN foods.food_status IS '状态：NORMAL/OFF/AUDITING/REJECTED';
COMMENT ON COLUMN foods.submit_role IS '提交来源';
COMMENT ON COLUMN foods.apply_status IS '审核状态';
COMMENT ON COLUMN foods.apply_remark IS '审核备注';
COMMENT ON COLUMN foods.created_by IS '创建者用户ID';
COMMENT ON COLUMN foods.owner_user_id IS '拥有者用户ID';
COMMENT ON COLUMN foods.group_id IS '所属关联组ID';
COMMENT ON COLUMN foods.approved_at IS '审核通过时间';
COMMENT ON COLUMN foods.approved_by IS '审核人用户ID';
COMMENT ON COLUMN foods.is_del IS '逻辑删除标记';
COMMENT ON COLUMN foods.created_at IS '创建时间';
COMMENT ON COLUMN foods.updated_at IS '更新时间';
CREATE INDEX idx_food_group_apply ON foods(group_id, apply_status);
CREATE INDEX idx_food_owner ON foods(owner_user_id);

-- ================= INGREDIENTS =================
CREATE TABLE ingredients (
    ingredient_id BIGSERIAL PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    group_id BIGINT REFERENCES association_groups(group_id) ON DELETE CASCADE,
    unit VARCHAR(32) DEFAULT '份',
    calories INT DEFAULT 0,
    description TEXT,
    icon VARCHAR(256),
    sort INT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (name, group_id)
);
COMMENT ON TABLE ingredients IS '食材库（按组管理的字典表）';
CREATE INDEX idx_ingredient_group ON ingredients(group_id);

CREATE TABLE user_food_mark (
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE CASCADE,
    mark_type mark_type_enum NOT NULL DEFAULT 'LIKE',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, food_id, mark_type)
);
COMMENT ON TABLE user_food_mark IS '用户菜品标记/收藏';
CREATE INDEX idx_ufm_food ON user_food_mark(food_id);

CREATE TABLE food_audit_logs (
    id BIGSERIAL PRIMARY KEY,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE CASCADE,
    action SMALLINT NOT NULL,
    from_status apply_status_enum,
    to_status apply_status_enum NOT NULL,
    acted_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE food_audit_logs IS '菜品审核历史';
CREATE INDEX idx_fal_food ON food_audit_logs(food_id);
CREATE INDEX idx_fal_actor ON food_audit_logs(acted_by);

-- ================= ORDERS =================
CREATE TABLE orders (
    order_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    group_id BIGINT REFERENCES association_groups(group_id) ON DELETE SET NULL,
    status order_status_enum NOT NULL DEFAULT 'CREATED',
    type order_type_enum NOT NULL DEFAULT 'NORMAL',
    goal_time TIMESTAMPTZ,
    remark VARCHAR(255),
    -- Role snapshots per FSD.latest.md
    creator_role_snapshot VARCHAR(32),
    assignee_id BIGINT REFERENCES users(user_id),
    assignee_role_snapshot VARCHAR(32),
    -- Guest order fields
    guest_user_id BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    guest_invite_id BIGINT REFERENCES guest_invitations(id) ON DELETE SET NULL,
    guest_remark TEXT,
    guest_mark_tags JSONB DEFAULT '[]',
    -- Reward fields
    points_reward INT NOT NULL DEFAULT 0,
    group_exp_reward INT NOT NULL DEFAULT 0,
    point_grant_status point_grant_status_enum DEFAULT 'NONE',
    exp_grant_status exp_grant_status_enum DEFAULT 'NONE',
    -- Risk fields
    risk_status risk_status_enum DEFAULT 'PASS',
    risk_detail JSONB,
    -- Order content per FSD.latest.md
    title VARCHAR(128),
    content TEXT,
    deadline TIMESTAMPTZ,
    -- Legacy fields
    cancel_reason VARCHAR(255),
    reject_reason VARCHAR(255),
    last_status_change_at TIMESTAMPTZ,
    -- Timestamps
    version INT NOT NULL DEFAULT 1,
    accepted_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    confirmed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_guest BOOLEAN NOT NULL DEFAULT FALSE
);
COMMENT ON TABLE orders IS '订单主表';
COMMENT ON COLUMN orders.order_id IS '订单主键ID';
COMMENT ON COLUMN orders.user_id IS '下单用户ID';
COMMENT ON COLUMN orders.group_id IS '所属关联组ID';
COMMENT ON COLUMN orders.status IS '订单状态';
COMMENT ON COLUMN orders.type IS '订单类型: NORMAL/GUEST';
COMMENT ON COLUMN orders.goal_time IS '期望完成时间';
COMMENT ON COLUMN orders.remark IS '下单备注';
COMMENT ON COLUMN orders.creator_role_snapshot IS '创建时角色快照';
COMMENT ON COLUMN orders.assignee_id IS '接单人ID';
COMMENT ON COLUMN orders.assignee_role_snapshot IS '接单时角色快照';
COMMENT ON COLUMN orders.guest_user_id IS '做客用户ID';
COMMENT ON COLUMN orders.guest_invite_id IS '做客邀请ID';
COMMENT ON COLUMN orders.guest_remark IS '做客备注、口味偏好、忌口、到访时间';
COMMENT ON COLUMN orders.guest_mark_tags IS '做客订单标记';
COMMENT ON COLUMN orders.points_reward IS '奖励积分';
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
COMMENT ON COLUMN orders.created_at IS '创建时间';
COMMENT ON COLUMN orders.updated_at IS '更新时间';
COMMENT ON COLUMN orders.is_guest IS '是否是客人下单';
CREATE INDEX idx_orders_user ON orders(user_id);
CREATE INDEX idx_orders_group_status ON orders(group_id, status);
CREATE INDEX idx_orders_assignee ON orders(assignee_id);
CREATE INDEX idx_orders_guest_user ON orders(guest_user_id);
CREATE INDEX idx_orders_guest_invite ON orders(guest_invite_id);

CREATE TABLE order_items (
    id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE RESTRICT,
    quantity INT NOT NULL DEFAULT 1,
    price NUMERIC(10, 2),
    snapshot_json JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE order_items IS '订单菜品明细';
CREATE INDEX idx_oi_order ON order_items(order_id);
CREATE INDEX idx_oi_food ON order_items(food_id);

CREATE TABLE order_status_history (
    id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
    from_status order_status_enum,
    to_status order_status_enum NOT NULL,
    changed_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    remark VARCHAR(255),
    changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE order_status_history IS '订单状态变更历史';
CREATE INDEX idx_osh_order ON order_status_history(order_id);
CREATE INDEX idx_osh_changed ON order_status_history(changed_at);

-- ================= WISHES =================
CREATE TABLE wishes (
    wish_id BIGSERIAL PRIMARY KEY,
    wish_name VARCHAR(128) NOT NULL,
    wish_cost INT NOT NULL,
    status wish_status_enum NOT NULL DEFAULT 'DRAFT',
    created_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    -- FSD v2: natural person binding (not role binding)
    requester_id BIGINT REFERENCES users(user_id),
    fulfiller_id BIGINT REFERENCES users(user_id),
    creator_role_snapshot VARCHAR(32),
    initial_cost INT,
    final_cost INT,
    fulfillment_deadline_hours INT,
    -- Selection fields
    selected_by BIGINT REFERENCES users(user_id),
    selected_at TIMESTAMPTZ,
    claim_cost INT,
    -- Fulfillment tracking
    fulfillment_due_at TIMESTAMPTZ,
    fulfilled_at TIMESTAMPTZ,
    expired_at TIMESTAMPTZ,
    -- Quality review
    quality_review_status wish_quality_status_enum DEFAULT 'NONE',
    quality_level wish_quality_level_enum DEFAULT 'NONE',
    quality_reviewer_id BIGINT REFERENCES users(user_id),
    quality_remark TEXT,
    diamond_reward INT DEFAULT 0,
    -- Timestamps
    claimed_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    claimed_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    version INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wishes IS '心愿模板及状态 - 7状态模型';
COMMENT ON COLUMN wishes.wish_id IS '心愿ID';
COMMENT ON COLUMN wishes.wish_name IS '心愿名称';
COMMENT ON COLUMN wishes.wish_cost IS '心愿所需积分';
COMMENT ON COLUMN wishes.status IS '心愿状态: DRAFT/NEGOTIATING/CREATED/CLAIMED/FINISHED/EXPIRED/CLOSED';
COMMENT ON COLUMN wishes.created_by IS '创建者用户ID';
COMMENT ON COLUMN wishes.group_id IS '所属关联组ID';
COMMENT ON COLUMN wishes.requester_id IS '发起人(选择心愿和支付积分的人)';
COMMENT ON COLUMN wishes.fulfiller_id IS '履约人(线下完成心愿的人)';
COMMENT ON COLUMN wishes.creator_role_snapshot IS '创建时角色快照';
COMMENT ON COLUMN wishes.initial_cost IS '初始报价';
COMMENT ON COLUMN wishes.final_cost IS '双方确认后的爱心积分价格';
COMMENT ON COLUMN wishes.fulfillment_deadline_hours IS '履约期限小时数';
COMMENT ON COLUMN wishes.selected_by IS '选择心愿的人(通常等于requester_id)';
COMMENT ON COLUMN wishes.selected_at IS '选择时间';
COMMENT ON COLUMN wishes.claim_cost IS '兑换时消耗积分';
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
COMMENT ON COLUMN wishes.created_at IS '创建时间';
COMMENT ON COLUMN wishes.updated_at IS '更新时间';
CREATE INDEX idx_wish_status ON wishes(status);
CREATE INDEX idx_wish_created_by ON wishes(created_by);
CREATE INDEX idx_wish_group ON wishes(group_id);
CREATE INDEX idx_wish_requester ON wishes(requester_id);
CREATE INDEX idx_wish_fulfiller ON wishes(fulfiller_id);
CREATE INDEX idx_wish_selected_by ON wishes(selected_by);
CREATE INDEX idx_wish_claimed_by ON wishes(claimed_by);

-- ================= WISH NEGOTIATIONS =================
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
COMMENT ON TABLE wish_negotiations IS '心愿协商记录';
CREATE INDEX idx_wn_wish ON wish_negotiations(wish_id);
CREATE INDEX idx_wn_group ON wish_negotiations(group_id);

-- ================= WISH CHECKINS =================
CREATE TABLE wish_checkins (
    id BIGSERIAL PRIMARY KEY,
    wish_id BIGINT NOT NULL REFERENCES wishes(wish_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content TEXT,
    location VARCHAR(256),
    images JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    -- FSD §11.9: user_id 必须是心愿的 requester_id (打卡权仅限发起人)
    -- 校验在应用层完成 (POST /api/wishes/{wish_id}/feedback 路由)
);
COMMENT ON TABLE wish_checkins IS '心愿打卡记录 - FSD §11.9';
COMMENT ON COLUMN wish_checkins.user_id IS '提交人(必须为发起人 requester_id,应用层校验)';
CREATE INDEX idx_wc_wish ON wish_checkins(wish_id);
CREATE INDEX idx_wc_user ON wish_checkins(user_id);

-- ================= WISH FEEDBACKS =================
CREATE TABLE wish_feedbacks (
    feedback_id BIGSERIAL PRIMARY KEY,
    wish_id BIGINT NOT NULL REFERENCES wishes(wish_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content TEXT,
    images JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(wish_id)
);
COMMENT ON TABLE wish_feedbacks IS '心愿反馈记录';
CREATE INDEX idx_wf_wish ON wish_feedbacks(wish_id);

-- ================= LOVE POINT TRANSACTIONS =================
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

-- ================= GROUP EXP TRANSACTIONS =================
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

-- ================= DIAMOND TRANSACTIONS =================
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

-- ================= DAILY REWARD COUNTERS =================
CREATE TABLE daily_reward_counters (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    user_id BIGINT,
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

-- ================= POINT TRANSACTIONS (legacy) =================
CREATE TABLE point_transactions (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    amount INT NOT NULL,
    type point_tx_type_enum NOT NULL,
    ref_type SMALLINT,
    ref_id BIGINT,
    balance_after INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE point_transactions IS '积分变动流水(legacy)';
CREATE INDEX idx_pt_user_created ON point_transactions(user_id, created_at);
CREATE INDEX idx_pt_ref ON point_transactions(ref_type, ref_id);
CREATE INDEX idx_pt_type ON point_transactions(type);

-- ================= SIGN IN =================
CREATE TABLE sign_records (
    sign_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    sign_date DATE NOT NULL,
    consecutive_days INT NOT NULL DEFAULT 1,
    diamond_reward INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, group_id, sign_date)
);
COMMENT ON TABLE sign_records IS '用户签到记录';
COMMENT ON COLUMN sign_records.sign_id IS '签到记录主键ID';
COMMENT ON COLUMN sign_records.user_id IS '用户ID';
COMMENT ON COLUMN sign_records.group_id IS '组ID';
COMMENT ON COLUMN sign_records.sign_date IS '签到日期';
COMMENT ON COLUMN sign_records.consecutive_days IS '连续签到天数';
COMMENT ON COLUMN sign_records.diamond_reward IS '本次签到获得钻石';
COMMENT ON COLUMN sign_records.created_at IS '签到时间';
CREATE INDEX idx_sr_user_date ON sign_records(user_id, sign_date DESC);
CREATE INDEX idx_sr_group_date ON sign_records(group_id, sign_date DESC);

-- ================= SWEET TALKS =================
CREATE TABLE sweet_talks (
    talk_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE sweet_talks IS '每日情话记录';
CREATE INDEX idx_st_group_time ON sweet_talks(group_id, created_at DESC);
CREATE INDEX idx_st_user_today ON sweet_talks(user_id, (CAST(created_at AT TIME ZONE 'Asia/Shanghai' AS DATE)));

-- ================= ORDER RATINGS =================
CREATE TABLE order_ratings (
    rating_id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
    rater_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    target_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    delta INT NOT NULL,
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(order_id)
);
COMMENT ON TABLE order_ratings IS '订单完成后的评分加减分记录';
CREATE INDEX idx_or_target ON order_ratings(target_user_id);
CREATE INDEX idx_or_rater ON order_ratings(rater_user_id);

-- ================= LOTTERY =================
CREATE TABLE lottery_draws (
    draw_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    food_types_requested VARCHAR(32),
    request_payload JSONB,
    is_success lottery_success_enum NOT NULL DEFAULT 'SUCCESS',
    fail_reason VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE lottery_draws IS '抽奖主记录';
CREATE INDEX idx_ld_user_time ON lottery_draws(user_id, created_at);

CREATE TABLE lottery_draw_results (
    id BIGSERIAL PRIMARY KEY,
    draw_id BIGINT NOT NULL REFERENCES lottery_draws(draw_id) ON DELETE CASCADE,
    food_type SMALLINT NOT NULL,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE RESTRICT,
    food_name_snapshot VARCHAR(128),
    food_photo_snapshot VARCHAR(256),
    allocated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(draw_id, food_type)
);
COMMENT ON TABLE lottery_draw_results IS '抽奖结果明细';
CREATE INDEX idx_ldr_draw ON lottery_draw_results(draw_id);

-- ================= MESSAGES =================
CREATE TABLE message_categories (
    category_id BIGSERIAL PRIMARY KEY,
    type_name VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE message_categories IS '消息类别';

CREATE TABLE messages (
    message_id BIGSERIAL PRIMARY KEY,
    category_id BIGINT NOT NULL REFERENCES message_categories(category_id) ON DELETE CASCADE,
    sender_id BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    target_user_id BIGINT REFERENCES users(user_id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    status message_status_enum NOT NULL DEFAULT 'ACTIVE',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE messages IS '消息记录';
CREATE INDEX idx_msg_category_time ON messages(category_id, created_at);
CREATE INDEX idx_msg_target ON messages(target_user_id);

CREATE TABLE user_message_state (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    category_id BIGINT NOT NULL REFERENCES message_categories(category_id) ON DELETE CASCADE,
    last_read_at TIMESTAMPTZ,
    unread_count INT NOT NULL DEFAULT 0,
    UNIQUE(user_id, category_id)
);
COMMENT ON TABLE user_message_state IS '用户消息阅读状态';

-- ================= FEEDBACK =================
CREATE TABLE feedback (
    feedback_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    status feedback_status_enum NOT NULL DEFAULT 'NEW',
    reply TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE feedback IS '用户反馈';
CREATE INDEX idx_fb_status ON feedback(status);

-- ================= CART =================
CREATE TABLE carts (
    cart_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    status cart_status_enum NOT NULL DEFAULT 'ACTIVE',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, group_id, status)
);
COMMENT ON TABLE carts IS '购物车主表';

CREATE TABLE cart_items (
    id BIGSERIAL PRIMARY KEY,
    cart_id BIGINT NOT NULL REFERENCES carts(cart_id) ON DELETE CASCADE,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE RESTRICT,
    quantity INT NOT NULL DEFAULT 1,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE cart_items IS '购物车明细';
CREATE INDEX idx_ci_cart ON cart_items(cart_id);
CREATE INDEX idx_ci_food ON cart_items(food_id);

-- ================= FOOD STATS =================
CREATE TABLE food_stats (
    food_id BIGINT PRIMARY KEY REFERENCES foods(food_id) ON DELETE CASCADE,
    total_order_count INT NOT NULL DEFAULT 0,
    completed_order_count INT NOT NULL DEFAULT 0,
    last_order_time TIMESTAMPTZ,
    last_complete_time TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE food_stats IS '菜品统计宽表';
CREATE INDEX idx_fs_order_count ON food_stats(total_order_count);
CREATE INDEX idx_fs_complete_count ON food_stats(completed_order_count);

-- ================= WECHAT SUBSCRIPTION TEMPLATES =================
CREATE TABLE wx_subscription_templates (
    template_id BIGSERIAL PRIMARY KEY,
    template_code VARCHAR(128) NOT NULL UNIQUE,
    template_name VARCHAR(128) NOT NULL,
    wx_template_id VARCHAR(256) NOT NULL UNIQUE,
    description VARCHAR(255),
    is_active SMALLINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wx_subscription_templates IS '微信小程序订阅消息模板';
CREATE INDEX idx_wst_code ON wx_subscription_templates(template_code);
CREATE INDEX idx_wst_active ON wx_subscription_templates(is_active);

-- ================= GROUP POINT CONFIGS =================
CREATE TABLE group_point_configs (
    group_id BIGINT PRIMARY KEY REFERENCES association_groups(group_id) ON DELETE CASCADE,
    breeder_closed_points INT NOT NULL DEFAULT -8,
    confirmed_finished_points INT NOT NULL DEFAULT 10,
    confirmed_unfinished_points INT NOT NULL DEFAULT -5,
    timeout_points INT NOT NULL DEFAULT -3,
    overdue_unfinished_points INT NOT NULL DEFAULT -10,
    unlock_card_diamond_cost INT NOT NULL DEFAULT 100,
    default_footprint_capacity INT NOT NULL DEFAULT 10,
    daily_checkin_rewards INT[] NOT NULL DEFAULT '{5,6,7,8,9,10,20}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE group_point_configs IS '组积分奖惩配置';

-- ================= USER DIAMOND (legacy) =================
CREATE TABLE user_diamond (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL UNIQUE REFERENCES users(user_id) ON DELETE CASCADE,
    diamond_balance INT NOT NULL DEFAULT 0,
    total_get INT NOT NULL DEFAULT 0,
    total_consume INT NOT NULL DEFAULT 0,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE user_diamond IS '用户钻石余额表(legacy)';

-- ================= DIAMOND FLOW (legacy) =================
CREATE TABLE diamond_flow (
    id BIGSERIAL PRIMARY KEY,
    flow_no VARCHAR(64) NOT NULL UNIQUE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    type SMALLINT NOT NULL,
    scene VARCHAR(32) NOT NULL,
    diamond_num INT NOT NULL,
    balance_after INT NOT NULL,
    relation_id BIGINT,
    remark VARCHAR(255),
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE diamond_flow IS '钻石流水记录表(legacy)';
CREATE INDEX idx_diamond_flow_user_id ON diamond_flow(user_id);

-- ================= GROUP DIAMOND FLOW =================
CREATE TABLE group_diamond_flow (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    type SMALLINT NOT NULL,
    scene VARCHAR(50) NOT NULL,
    diamond_num INT NOT NULL,
    balance_after INT NOT NULL,
    remark TEXT,
    ref_id BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE group_diamond_flow IS '组钻石流水记录表';
CREATE INDEX idx_gdf_group_id ON group_diamond_flow(group_id);
CREATE INDEX idx_gdf_created ON group_diamond_flow(created_at);

-- ================= RECORD GROUP =================
CREATE TABLE record_group (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    group_name VARCHAR(50) NOT NULL,
    group_type SMALLINT NOT NULL,
    max_capacity INT NOT NULL DEFAULT 50,
    current_count INT NOT NULL DEFAULT 0,
    status SMALLINT NOT NULL DEFAULT 1,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(group_id, group_name)
);
COMMENT ON TABLE record_group IS '足迹记录分组表';
CREATE INDEX idx_record_group_group_id ON record_group(group_id);

-- ================= USER RECORD =================
CREATE TABLE user_record (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    record_group_id BIGINT NOT NULL REFERENCES record_group(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    order_id BIGINT UNIQUE REFERENCES orders(order_id) ON DELETE SET NULL,
    title VARCHAR(128),
    images TEXT NOT NULL,
    content TEXT,
    address VARCHAR(255),
    record_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    like_count INT NOT NULL DEFAULT 0,
    comment_count INT NOT NULL DEFAULT 0,
    is_draft SMALLINT NOT NULL DEFAULT 0,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE user_record IS '用户足迹记录表';
CREATE INDEX idx_user_record_group_id ON user_record(group_id);
CREATE INDEX idx_user_record_rg_id ON user_record(record_group_id);

-- ================= COMMENTS & LIKES (V2.0 暂缓) =================
-- 足迹评论与点赞 V1.0 不实现。表结构从 v3.sql 移除。
-- FSD §24.11 旧章节已删除，§24.12 标记 V2.0 暂缓。
-- V2.0 重新设计时新建 record_comment / record_like 表。

-- ================= ACHIEVEMENTS =================
CREATE TABLE achievement_definitions (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(128) NOT NULL,
    icon VARCHAR(256),
    description TEXT,
    requirement_type VARCHAR(64) NOT NULL,
    requirement_value INT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE achievement_definitions IS '成就/勋章定义表 (DEPRECATED - 已被 achievements 替代,FSD §11.21)';

-- ================= EVENT LOG =================
CREATE TABLE event_log (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(64) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    user_id BIGINT,
    group_id BIGINT,
    ref_type VARCHAR(32),
    ref_id BIGINT,
    status event_status_enum NOT NULL DEFAULT 'PENDING',
    retry_count INT NOT NULL DEFAULT 0,
    max_retries INT NOT NULL DEFAULT 3,
    error_message TEXT,
    idempotency_key VARCHAR(128),
    trace_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);
COMMENT ON TABLE event_log IS '事件日志表 - 事件驱动架构核心';
CREATE INDEX idx_event_log_status ON event_log(status, created_at);
CREATE INDEX idx_event_log_user_id ON event_log(user_id);
CREATE INDEX idx_event_log_group_id ON event_log(group_id);
CREATE INDEX idx_event_log_type ON event_log(event_type);
CREATE UNIQUE INDEX idx_event_log_idempotency ON event_log(idempotency_key) WHERE idempotency_key IS NOT NULL;
CREATE INDEX idx_event_log_trace ON event_log(trace_id);

-- ================= MEMORIAL DAY =================
CREATE TABLE memorial_day (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    memorial_date DATE NOT NULL,
    calendar_type VARCHAR(16) NOT NULL DEFAULT 'SOLAR',
    lunar_month SMALLINT,
    lunar_day SMALLINT,
    is_leap_month BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_default SMALLINT NOT NULL DEFAULT 0,
    UNIQUE(group_id, name)
);
COMMENT ON TABLE memorial_day IS '纪念日表';
CREATE INDEX idx_memorial_group ON memorial_day(group_id);

-- ============================================
-- ===== FSD §11 REQUIRED TABLES (v2026-06-03) =====
-- ============================================

-- ================= GROUP INVITES (§11.14) =================
-- 替代旧的 guest_invitations；统一处理组邀请
CREATE TABLE group_invites (
    invite_id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    invite_code VARCHAR(32) NOT NULL UNIQUE,
    created_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    expire_at TIMESTAMPTZ NOT NULL,
    max_uses INT NOT NULL DEFAULT 1,
    used_count INT NOT NULL DEFAULT 0,
    status group_invite_status_enum NOT NULL DEFAULT 'ACTIVE',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE group_invites IS '组邀请链接/邀请码 - FSD §11.14';
COMMENT ON COLUMN group_invites.invite_code IS '6位字母数字邀请码';
COMMENT ON COLUMN group_invites.expire_at IS '失效时间';
COMMENT ON COLUMN group_invites.max_uses IS '最大使用次数';
COMMENT ON COLUMN group_invites.status IS 'ACTIVE/EXPIRED/EXHAUSTED';
CREATE UNIQUE INDEX uniq_group_invites_code ON group_invites(invite_code);
CREATE INDEX idx_group_invites_group_status ON group_invites(group_id, status);

-- ================= SIGN IN RECORDS (§11.15) =================
-- 替代旧的 sign_records
CREATE TABLE sign_in_records (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    sign_date DATE NOT NULL,
    consecutive_days INT NOT NULL DEFAULT 1,
    diamond_reward INT NOT NULL DEFAULT 0,
    full_team_bonus BOOLEAN NOT NULL DEFAULT FALSE,
    full_team_bonus_amt INT NOT NULL DEFAULT 0,
    idempotency_key VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(group_id, user_id, sign_date)
);
COMMENT ON TABLE sign_in_records IS '用户每日签到记录 - FSD §11.15';
COMMENT ON COLUMN sign_in_records.sign_date IS '签到日期(按组配置时区折算)';
COMMENT ON COLUMN sign_in_records.consecutive_days IS '连续签到天数';
COMMENT ON COLUMN sign_in_records.diamond_reward IS '本次发放钻石数';
COMMENT ON COLUMN sign_in_records.full_team_bonus IS '是否触发双方签到满奖励';
CREATE UNIQUE INDEX uniq_sign_in_group_user_date ON sign_in_records(group_id, user_id, sign_date);
CREATE INDEX idx_sign_in_group_date ON sign_in_records(group_id, sign_date);

-- ================= FOOTPRINTS (§11.16) =================
-- 替代旧的 user_record；统一为 FSD 定义的足迹结构
CREATE TABLE footprints (
    footprint_id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content TEXT,
    location VARCHAR(256),
    images JSONB,
    related_order_id BIGINT REFERENCES orders(order_id) ON DELETE SET NULL,
    related_wish_id BIGINT REFERENCES wishes(wish_id) ON DELETE SET NULL,
    content_check_status content_check_status_enum NOT NULL DEFAULT 'PENDING',
    status VARCHAR(20) NOT NULL DEFAULT 'ACTIVE',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);
COMMENT ON TABLE footprints IS '组内足迹/纪念内容 - FSD §11.16';
COMMENT ON COLUMN footprints.related_order_id IS '关联订单ID（订单完成自动生成）';
COMMENT ON COLUMN footprints.related_wish_id IS '关联心愿ID（心愿完成自动生成）';
COMMENT ON COLUMN footprints.content_check_status IS 'PENDING/PASS/REJECTED';
COMMENT ON COLUMN footprints.status IS 'ACTIVE/DELETED';
CREATE INDEX idx_footprints_group_created ON footprints(group_id, created_at DESC);
CREATE INDEX idx_footprints_user ON footprints(user_id);

-- ================= GROUP FOOTPRINT CAPACITY (§11.17) =================
CREATE TABLE group_footprint_capacity (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL UNIQUE REFERENCES association_groups(group_id) ON DELETE CASCADE,
    base_capacity INT NOT NULL DEFAULT 20,
    expanded_capacity INT NOT NULL DEFAULT 0,
    total_capacity INT NOT NULL DEFAULT 20,
    diamond_spent BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE group_footprint_capacity IS '组足迹容量配置 - FSD §11.17';
COMMENT ON COLUMN group_footprint_capacity.base_capacity IS '等级基础容量';
COMMENT ON COLUMN group_footprint_capacity.expanded_capacity IS '钻石扩容累加';
COMMENT ON COLUMN group_footprint_capacity.total_capacity IS '实际容量=基础+扩容';

-- ================= NOTIFICATIONS (§11.18) =================
-- 替代旧的 messages；统一为 FSD 定义的 4 类通知
CREATE TABLE notifications (
    notification_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    type notification_type_enum NOT NULL,
    title VARCHAR(128) NOT NULL,
    content TEXT NOT NULL,
    data JSONB,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE notifications IS '系统通知 - FSD §11.18';
COMMENT ON COLUMN notifications.type IS 'ORDER/WISH/SIGN_IN/SYSTEM (FSD §10.1)';
COMMENT ON COLUMN notifications.data IS '关联业务数据 JSON';
CREATE INDEX idx_notifications_user_read ON notifications(user_id, is_read);
CREATE INDEX idx_notifications_user_created ON notifications(user_id, created_at DESC);

-- ================= UPLOAD FILES (§11.19) =================
-- 七牛云直传文件登记表
CREATE TABLE upload_files (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    file_key VARCHAR(512) NOT NULL UNIQUE,
    original_filename VARCHAR(256),
    content_type VARCHAR(64),
    size BIGINT,
    cdn_url VARCHAR(512),
    content_check_status content_check_status_enum NOT NULL DEFAULT 'PENDING',
    business_ref_type upload_business_ref_enum,
    business_ref_id BIGINT,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    upload_token_hash VARCHAR(128),
    qiniu_hash VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);
COMMENT ON TABLE upload_files IS '七牛云上传文件登记表 - FSD §11.19';
COMMENT ON COLUMN upload_files.file_key IS '对象存储 key (七牛唯一)';
COMMENT ON COLUMN upload_files.cdn_url IS 'CDN 访问 URL';
COMMENT ON COLUMN upload_files.business_ref_type IS 'food/footprint/checkin/avatar';
COMMENT ON COLUMN upload_files.status IS 'PENDING/ACTIVE/DELETED';
COMMENT ON COLUMN upload_files.upload_token_hash IS '颁发 token 时的 hash (防伪造)';
COMMENT ON COLUMN upload_files.qiniu_hash IS '七牛返回的文件 etag/hash';
CREATE UNIQUE INDEX uniq_upload_file_key ON upload_files(file_key);
CREATE INDEX idx_upload_user_created ON upload_files(user_id, created_at DESC);
CREATE INDEX idx_upload_business ON upload_files(business_ref_type, business_ref_id);

-- ================= AUDIT LOGS (§11.20) =================
CREATE TABLE audit_logs (
    id BIGSERIAL PRIMARY KEY,
    operator_id BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    operator_type VARCHAR(20) NOT NULL DEFAULT 'ADMIN',
    action_type VARCHAR(64) NOT NULL,
    target_type VARCHAR(32),
    target_id BIGINT,
    detail JSONB,
    ip VARCHAR(64),
    user_agent VARCHAR(256),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE audit_logs IS '审计日志 - FSD §11.20';
COMMENT ON COLUMN audit_logs.operator_type IS 'ADMIN/SYSTEM';
COMMENT ON COLUMN audit_logs.action_type IS 'USER_BAN/CONFIG_UPDATE/POINT_COMPENSATE 等';
COMMENT ON COLUMN audit_logs.detail IS '操作详情 (前后值/原因)';
CREATE INDEX idx_audit_operator_created ON audit_logs(operator_id, created_at DESC);
CREATE INDEX idx_audit_action_created ON audit_logs(action_type, created_at DESC);

-- ================= ACHIEVEMENTS (§11.21) =================
-- 替代 achievement_definitions；增加 rule_config JSONB
CREATE TABLE achievements (
    achievement_id BIGSERIAL PRIMARY KEY,
    code VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    category achievement_category_enum NOT NULL,
    rule_type VARCHAR(64) NOT NULL,
    rule_config JSONB,
    icon VARCHAR(256),
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE achievements IS '成就定义 - FSD §11.21';
COMMENT ON COLUMN achievements.code IS '业务唯一编码 (FIRST_ORDER 等)';
COMMENT ON COLUMN achievements.category IS 'USER/GROUP';
COMMENT ON COLUMN achievements.rule_type IS '判定类型';
COMMENT ON COLUMN achievements.rule_config IS '判定规则配置 JSON';

CREATE TABLE user_achievements (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    achievement_id BIGINT NOT NULL REFERENCES achievements(achievement_id) ON DELETE CASCADE,
    progress INT NOT NULL DEFAULT 0,
    unlocked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, achievement_id)
);
COMMENT ON TABLE user_achievements IS '用户成就解锁记录 - FSD §11.21';
CREATE UNIQUE INDEX uniq_user_achievement ON user_achievements(user_id, achievement_id);

CREATE TABLE group_achievements (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    achievement_id BIGINT NOT NULL REFERENCES achievements(achievement_id) ON DELETE CASCADE,
    progress INT NOT NULL DEFAULT 0,
    unlocked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(group_id, achievement_id)
);
COMMENT ON TABLE group_achievements IS '组成就解锁记录 - FSD §11.21';
CREATE UNIQUE INDEX uniq_group_achievement ON group_achievements(group_id, achievement_id);

-- ================= GLOBAL CONFIGS (§11.22) =================
CREATE TABLE global_configs (
    config_id BIGSERIAL PRIMARY KEY,
    config_key VARCHAR(128) NOT NULL UNIQUE,
    config_value JSONB,
    category config_category_enum NOT NULL DEFAULT 'GENERAL',
    description TEXT,
    updated_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE global_configs IS '全局配置 - FSD §11.22';
COMMENT ON COLUMN global_configs.config_key IS 'sign_in_diamond_reward 等';
COMMENT ON COLUMN global_configs.category IS 'REWARDS/SIGN_IN/WISH/ORDER/RISK/UPLOAD/GENERAL';
CREATE UNIQUE INDEX uniq_global_config_key ON global_configs(config_key);
CREATE INDEX idx_global_config_category ON global_configs(category);

-- ================= GROUP CONFIGS (§11.23) =================
-- 替代 group_point_configs；统一为 FSD 定义的 9 个字段
CREATE TABLE group_configs (
    config_id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL UNIQUE REFERENCES association_groups(group_id) ON DELETE CASCADE,
    normal_order_love_point INT NOT NULL DEFAULT 10,
    guest_order_love_point INT NOT NULL DEFAULT 10,
    normal_order_group_exp INT NOT NULL DEFAULT 5,
    guest_order_group_exp INT NOT NULL DEFAULT 5,
    daily_love_point_limit INT NOT NULL DEFAULT 100,
    daily_group_exp_limit INT NOT NULL DEFAULT 200,
    order_timeout_hours INT NOT NULL DEFAULT 24,
    food_capacity INT NOT NULL DEFAULT 20,
    tag_capacity INT NOT NULL DEFAULT 10,
    footprint_capacity INT NOT NULL DEFAULT 20,
    updated_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE group_configs IS '组级配置覆盖 - FSD §11.23';
COMMENT ON COLUMN group_configs.normal_order_love_point IS '本组普通订单完成默认爱心积分';
COMMENT ON COLUMN group_configs.guest_order_love_point IS '本组做客订单完成默认爱心积分';
COMMENT ON COLUMN group_configs.daily_love_point_limit IS '本组用户每日爱心积分上限';
COMMENT ON COLUMN group_configs.order_timeout_hours IS '本组订单超时时间(小时)';
CREATE UNIQUE INDEX uniq_group_config_group_id ON group_configs(group_id);

-- ================= ADMIN USERS (§11.24) =================
CREATE TABLE admin_users (
    admin_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL UNIQUE REFERENCES users(user_id) ON DELETE CASCADE,
    role admin_role_enum NOT NULL DEFAULT 'OPS',
    permissions JSONB,
    status VARCHAR(20) NOT NULL DEFAULT 'ACTIVE',
    mfa_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ
);
COMMENT ON TABLE admin_users IS '管理员账号 - FSD §11.24';
COMMENT ON COLUMN admin_users.role IS 'SUPER_ADMIN/OPS/RISK_REVIEWER';
COMMENT ON COLUMN admin_users.mfa_enabled IS '是否启用二次验证';
CREATE UNIQUE INDEX uniq_admin_user_id ON admin_users(user_id);
CREATE INDEX idx_admin_role_status ON admin_users(role, status);

-- ================= SUPPORT TICKETS (§15.3) =================
-- 客服工单表（FSD §15.3）
CREATE TABLE support_tickets (
    ticket_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT REFERENCES association_groups(group_id) ON DELETE SET NULL,
    order_id BIGINT REFERENCES orders(order_id) ON DELETE SET NULL,
    wish_id BIGINT REFERENCES wishes(wish_id) ON DELETE SET NULL,
    category VARCHAR(32) NOT NULL DEFAULT 'OTHER',
    content TEXT NOT NULL,
    images JSONB,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    handler_id BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    resolution TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);
COMMENT ON TABLE support_tickets IS '客服工单 - FSD §15.3';
COMMENT ON COLUMN support_tickets.category IS 'BUG/COMPLAINT/SUGGESTION/OTHER';
COMMENT ON COLUMN support_tickets.status IS 'PENDING/PROCESSING/RESOLVED/CLOSED';
CREATE INDEX idx_support_user_status ON support_tickets(user_id, status);
CREATE INDEX idx_support_handler_status ON support_tickets(handler_id, status);

-- ============================================
-- ================= DEPRECATED TABLES =================
-- ============================================
-- 以下表为旧版本遗留，FSD §11 已废弃。保留以防旧代码引用，未来版本将移除。
-- 关键字段不与 FSD 对齐，新代码不应使用。

-- 旧版做客邀请（已被 group_invites 替代，但 guest_invitations 仍有做客订单引用）
-- 保留供历史数据
-- 已重构为支持做客订单
-- 旧版签到（已被 sign_in_records 替代）
-- 旧版足迹/记录（已被 footprints 替代）
-- 旧版消息（已被 notifications 替代）
-- 旧版配置（已被 global_configs + group_configs 替代）
-- 旧版成就（已被 achievements 替代）
-- 抽奖、购物车、情话、评分、纪念日等业务模块：FSD 未要求，保留供未来扩展

-- ============================================
-- ================= TEMPLATE DATA =================

INSERT INTO wx_subscription_templates (template_code, template_name, wx_template_id, description, is_active, created_at, updated_at)
VALUES (
    'ORDER_CREATED',
    '订单创建通知',
    'UmBKohC4s3sni-E5fmpDp_xL_S6uhQ1yTaTl6LOtftI',
    '当用户成功创建订单时发送通知',
    1,
    NOW(),
    NOW()
);

INSERT INTO wx_subscription_templates (template_code, template_name, wx_template_id, description, is_active, created_at, updated_at)
VALUES (
    'ORDER_STATUS_UPDATED',
    '订单状态更新通知',
    '3rkE-wK9z6Rxc_ffMx_fS4woy7iIDsxMcBUPsMWuFKI',
    '当订单状态发生变化时发送通知',
    1,
    NOW(),
    NOW()
);