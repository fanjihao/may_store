-- =========================================================
-- File: schema_v2_full_pg.sql
-- DB: PostgreSQL 15+
-- Date: 2025-11-05
-- Description: Unified schema (v1 + plus) with extended user fields,
--              order ratings, wish check-ins, and full commentary.
-- =========================================================
-- ================= ENUM TYPE DEFINITIONS =================
CREATE TYPE user_role_enum AS ENUM ('ORDERING', 'RECEIVING', 'ADMIN');
CREATE TYPE group_type_enum AS ENUM ('PAIR', 'FAMILY', 'TEAM');
CREATE TYPE group_member_role_enum AS ENUM ('ORDERING', 'RECEIVING', 'ADMIN');
CREATE TYPE food_status_enum AS ENUM ('NORMAL', 'OFF', 'AUDITING', 'REJECTED');
CREATE TYPE submit_role_enum AS ENUM ('ORDERING_APPLY', 'RECEIVING_CREATE');
CREATE TYPE apply_status_enum AS ENUM ('PENDING', 'APPROVED', 'REJECTED');
CREATE TYPE order_status_enum AS ENUM (
    'PENDING_ACCEPT',
    'IN_PROGRESS',
    'REJECTED',
    'BREEDER_FINISHED',
    'BREEDER_CLOSED',
    'CONFIRMED_FINISHED',
    'CONFIRMED_UNFINISHED',
    'TIMEOUT',
    'CANCELLED',
    'SYSTEM_CLOSED'
);
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
CREATE TYPE wish_status_enum AS ENUM ('CREATED', 'CLAIMED', 'FINISHED', 'CLOSED');
CREATE TYPE lottery_success_enum AS ENUM ('SUCCESS', 'FAIL');
CREATE TYPE message_status_enum AS ENUM ('ACTIVE', 'REVOKED');
CREATE TYPE feedback_status_enum AS ENUM ('NEW', 'PROCESSING', 'CLOSED');
CREATE TYPE cart_status_enum AS ENUM ('ACTIVE', 'SETTLED', 'CLEARED');
CREATE TYPE mark_type_enum AS ENUM ('LIKE', 'NOT_RECOMMEND');
CREATE TYPE gender_enum AS ENUM ('MALE', 'FEMALE', 'OTHER', 'UNKNOWN');
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
    open_id VARCHAR(128),
    status SMALLINT NOT NULL DEFAULT 1,
    -- 1正常 0禁用
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
    -- 最近一次角色互换时间（半年冷却）
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
COMMENT ON COLUMN users.status IS '状态：1正常 0禁用';
COMMENT ON COLUMN users.password_hash IS '哈希后的密码（永不存明文）';
COMMENT ON COLUMN users.password_algo IS '密码哈希算法标识';
COMMENT ON COLUMN users.gender IS '性别';
COMMENT ON COLUMN users.birthday IS '生日';
COMMENT ON COLUMN users.username_change IS '用户名是否修改过';
COMMENT ON COLUMN users.login_method IS '最近登录方式';
COMMENT ON COLUMN users.last_login_at IS '最近登录时间';
COMMENT ON COLUMN users.password_updated_at IS '最近密码更新时间';
COMMENT ON COLUMN users.is_temp_password IS '是否临时密码需修改';
COMMENT ON COLUMN users.push_id IS '推送ID用于消息通知';
COMMENT ON COLUMN users.last_role_switch_at IS '最近一次下单/接单角色对换时间（半年冷却）';
COMMENT ON COLUMN users.created_at IS '创建时间';
COMMENT ON COLUMN users.updated_at IS '更新时间';
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_users_status ON users(status);
CREATE INDEX idx_users_phone ON users(phone);
CREATE INDEX idx_users_login_method ON users(login_method);
CREATE INDEX idx_users_last_login ON users(last_login_at);
-- ================= ASSOCIATION GROUPS =================
CREATE TABLE association_groups (
    group_id BIGSERIAL PRIMARY KEY,
    group_name VARCHAR(128),
    group_type group_type_enum NOT NULL DEFAULT 'PAIR',
    status SMALLINT NOT NULL DEFAULT 1,
    invite_code VARCHAR(32),
    -- 1活跃 0关闭
    footprint_capacity INT NOT NULL DEFAULT 10,
    footprint_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE association_groups IS '用户关联组（绑定关系/团队）';
COMMENT ON COLUMN association_groups.group_id IS '组ID主键';
COMMENT ON COLUMN association_groups.group_name IS '组名称';
COMMENT ON COLUMN association_groups.group_type IS '组类型：PAIR/FAMILY/TEAM';
COMMENT ON COLUMN association_groups.status IS '状态：1活跃 0关闭';
COMMENT ON COLUMN association_groups.invite_code IS '做客邀请码';
COMMENT ON COLUMN association_groups.footprint_capacity IS '足迹全局容量';
COMMENT ON COLUMN association_groups.footprint_count IS '当前足迹记录总数';
COMMENT ON COLUMN association_groups.created_at IS '创建时间';
COMMENT ON COLUMN association_groups.updated_at IS '更新时间';
CREATE TABLE association_group_members (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    role_in_group group_member_role_enum NOT NULL,
    is_primary SMALLINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (group_id, user_id)
);
COMMENT ON TABLE association_group_members IS '关联组成员';
COMMENT ON COLUMN association_group_members.id IS '成员记录主键';
COMMENT ON COLUMN association_group_members.group_id IS '关联组ID';
COMMENT ON COLUMN association_group_members.user_id IS '用户ID';
COMMENT ON COLUMN association_group_members.role_in_group IS '组内角色';
COMMENT ON COLUMN association_group_members.is_primary IS '是否主成员标记';
COMMENT ON COLUMN association_group_members.created_at IS '添加时间';
CREATE INDEX idx_agm_user_role ON association_group_members(user_id, role_in_group);
CREATE INDEX idx_agm_group_role ON association_group_members(group_id, role_in_group);
CREATE TABLE association_group_requests (
    request_id BIGSERIAL PRIMARY KEY,
    requester_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    target_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    status SMALLINT NOT NULL DEFAULT 0,
    -- 0待处理 1同意 2拒绝 3过期
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    handled_at TIMESTAMPTZ
);
COMMENT ON TABLE association_group_requests IS '绑定申请记录';
COMMENT ON COLUMN association_group_requests.request_id IS '申请记录主键';
COMMENT ON COLUMN association_group_requests.requester_id IS '发起者用户ID';
COMMENT ON COLUMN association_group_requests.target_user_id IS '目标用户ID';
COMMENT ON COLUMN association_group_requests.status IS '申请状态,默认0,0待处理 1同意 2拒绝 4申请解绑中 5已解绑';
COMMENT ON COLUMN association_group_requests.remark IS '备注/理由';
COMMENT ON COLUMN association_group_requests.created_at IS '创建时间';
COMMENT ON COLUMN association_group_requests.handled_at IS '处理时间';
CREATE INDEX idx_agr_target_status ON association_group_requests(target_user_id, status);
-- ================= FOODS =================
-- 先创建 tags 表（因为 foods 表引用它）
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
COMMENT ON COLUMN tags.sort IS '排序值-越大越靠前';
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
    owner_user_id BIGINT REFERENCES users(user_id) ON DELETE
    SET NULL,
        group_id BIGINT REFERENCES association_groups(group_id) ON DELETE
    SET NULL,
        approved_at TIMESTAMPTZ,
        approved_by BIGINT REFERENCES users(user_id) ON DELETE
    SET NULL,
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
COMMENT ON COLUMN foods.submit_role IS '提交来源：ORDERING_APPLY/RECEIVING_CREATE';
COMMENT ON COLUMN foods.apply_status IS '审核状态：PENDING/APPROVED/REJECTED';
COMMENT ON COLUMN foods.apply_remark IS '审核备注';
COMMENT ON COLUMN foods.created_by IS '创建者用户ID';
COMMENT ON COLUMN foods.owner_user_id IS '拥有者用户ID（通过后归属）';
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
COMMENT ON COLUMN ingredients.ingredient_id IS '食材主键ID';
COMMENT ON COLUMN ingredients.name IS '食材名称（同一组内唯一）';
COMMENT ON COLUMN ingredients.group_id IS '所属组ID（关联association_groups）';
COMMENT ON COLUMN ingredients.unit IS '计量单位（如：克、斤、个）';
COMMENT ON COLUMN ingredients.calories IS '每100g的卡路里';
COMMENT ON COLUMN ingredients.description IS '食材描述/说明';
COMMENT ON COLUMN ingredients.icon IS '食材图标URL';
COMMENT ON COLUMN ingredients.sort IS '排序值-越大越靠前';
COMMENT ON COLUMN ingredients.created_at IS '创建时间';
COMMENT ON COLUMN ingredients.updated_at IS '更新时间';
CREATE INDEX idx_ingredient_group ON ingredients(group_id);

CREATE TABLE user_food_mark (
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE CASCADE,
    mark_type mark_type_enum NOT NULL DEFAULT 'LIKE',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, food_id, mark_type)
);
COMMENT ON TABLE user_food_mark IS '用户菜品标记/收藏';
COMMENT ON COLUMN user_food_mark.user_id IS '用户ID';
COMMENT ON COLUMN user_food_mark.food_id IS '菜品ID';
COMMENT ON COLUMN user_food_mark.mark_type IS '标记类型：LIKE/NOT_RECOMMEND';
COMMENT ON COLUMN user_food_mark.created_at IS '标记创建时间';
CREATE INDEX idx_ufm_food ON user_food_mark(food_id);
CREATE TABLE food_audit_logs (
    id BIGSERIAL PRIMARY KEY,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE CASCADE,
    action SMALLINT NOT NULL,
    -- 1提交 2通过 3拒绝 4修改
    from_status apply_status_enum,
    to_status apply_status_enum NOT NULL,
    acted_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE food_audit_logs IS '菜品审核历史';
COMMENT ON COLUMN food_audit_logs.id IS '审核记录主键';
COMMENT ON COLUMN food_audit_logs.food_id IS '菜品ID';
COMMENT ON COLUMN food_audit_logs.action IS '动作：1提交 2通过 3拒绝 4修改';
COMMENT ON COLUMN food_audit_logs.from_status IS '变更前状态';
COMMENT ON COLUMN food_audit_logs.to_status IS '变更后状态';
COMMENT ON COLUMN food_audit_logs.acted_by IS '操作人用户ID';
COMMENT ON COLUMN food_audit_logs.remark IS '审核备注';
COMMENT ON COLUMN food_audit_logs.created_at IS '记录创建时间';
CREATE INDEX idx_fal_food ON food_audit_logs(food_id);
CREATE INDEX idx_fal_actor ON food_audit_logs(acted_by);
-- ================= ORDERS =================
CREATE TABLE orders (
    order_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    guest_id BIGINT REFERENCES users(user_id) ON DELETE
    SET NULL,
        group_id BIGINT REFERENCES association_groups(group_id) ON DELETE
    SET NULL,
        status order_status_enum NOT NULL DEFAULT 'PENDING_ACCEPT',
        goal_time TIMESTAMPTZ,
        remark VARCHAR(255),
        points_reward INT NOT NULL DEFAULT 0,
        cancel_reason VARCHAR(255),
        reject_reason VARCHAR(255),
        last_status_change_at TIMESTAMPTZ,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_guest BOOLEAN NOT NULL DEFAULT FALSE
);
COMMENT ON TABLE orders IS '订单主表';
COMMENT ON COLUMN orders.order_id IS '订单主键ID';
COMMENT ON COLUMN orders.user_id IS '下单用户ID';
COMMENT ON COLUMN orders.guest_id IS '下单客人ID';
COMMENT ON COLUMN orders.group_id IS '所属关联组ID';
COMMENT ON COLUMN orders.status IS '订单状态';
COMMENT ON COLUMN orders.goal_time IS '期望完成/消费时间';
COMMENT ON COLUMN orders.remark IS '下单备注';
COMMENT ON COLUMN orders.points_reward IS '奖励积分（完成时可能发放）';
COMMENT ON COLUMN orders.cancel_reason IS '取消原因';
COMMENT ON COLUMN orders.reject_reason IS '拒绝原因';
COMMENT ON COLUMN orders.last_status_change_at IS '最后状态变更时间';
COMMENT ON COLUMN orders.created_at IS '创建时间';
COMMENT ON COLUMN orders.updated_at IS '更新时间';
COMMENT ON COLUMN orders.is_guest IS '是否是客人下单';
CREATE INDEX idx_order_user ON orders(user_id);
CREATE INDEX idx_order_guest ON orders(guest_id);
CREATE INDEX idx_order_group_status ON orders(group_id, status);
CREATE INDEX idx_order_status_goal ON orders(status, goal_time);
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
COMMENT ON COLUMN order_items.id IS '明细主键ID';
COMMENT ON COLUMN order_items.order_id IS '所属订单ID';
COMMENT ON COLUMN order_items.food_id IS '菜品ID';
COMMENT ON COLUMN order_items.quantity IS '数量';
COMMENT ON COLUMN order_items.price IS '单价(快照)';
COMMENT ON COLUMN order_items.snapshot_json IS '菜品快照数据JSON';
COMMENT ON COLUMN order_items.created_at IS '明细创建时间';
CREATE INDEX idx_oi_order ON order_items(order_id);
CREATE INDEX idx_oi_food ON order_items(food_id);
CREATE TABLE order_status_history (
    id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
    from_status order_status_enum,
    to_status order_status_enum NOT NULL,
    changed_by BIGINT REFERENCES users(user_id) ON DELETE
    SET NULL,
        remark VARCHAR(255),
        changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE order_status_history IS '订单状态变更历史';
COMMENT ON COLUMN order_status_history.id IS '历史主键ID';
COMMENT ON COLUMN order_status_history.order_id IS '订单ID';
COMMENT ON COLUMN order_status_history.from_status IS '原状态';
COMMENT ON COLUMN order_status_history.to_status IS '目标状态';
COMMENT ON COLUMN order_status_history.changed_by IS '变更操作者ID';
COMMENT ON COLUMN order_status_history.remark IS '变更备注';
COMMENT ON COLUMN order_status_history.changed_at IS '变更时间';
CREATE INDEX idx_osh_order ON order_status_history(order_id);
CREATE INDEX idx_osh_changed ON order_status_history(changed_at);
-- ================= POINT TRANSACTIONS =================
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
COMMENT ON TABLE point_transactions IS '积分变动流水';
COMMENT ON COLUMN point_transactions.id IS '流水主键ID';
COMMENT ON COLUMN point_transactions.user_id IS '用户ID';
COMMENT ON COLUMN point_transactions.amount IS '变动积分(正增负减)';
COMMENT ON COLUMN point_transactions.type IS '类型（奖励/扣减等）';
COMMENT ON COLUMN point_transactions.ref_type IS '参考来源类型(业务自定义)';
COMMENT ON COLUMN point_transactions.ref_id IS '参考来源ID';
COMMENT ON COLUMN point_transactions.balance_after IS '变动后余额';
COMMENT ON COLUMN point_transactions.created_at IS '记录创建时间';
CREATE INDEX idx_pt_user_created ON point_transactions(user_id, created_at);
CREATE INDEX idx_pt_ref ON point_transactions(ref_type, ref_id);
CREATE INDEX idx_pt_type ON point_transactions(type);
-- ================= SIGN IN =================
CREATE TABLE sign_records (
    sign_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    sign_date DATE NOT NULL,
    consecutive_days INT NOT NULL DEFAULT 1,
    diamonds_earned INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, sign_date)
);
COMMENT ON TABLE sign_records IS '用户签到记录';
COMMENT ON COLUMN sign_records.sign_id IS '签到记录主键ID';
COMMENT ON COLUMN sign_records.user_id IS '用户ID';
COMMENT ON COLUMN sign_records.sign_date IS '签到日期';
COMMENT ON COLUMN sign_records.consecutive_days IS '连续签到天数';
COMMENT ON COLUMN sign_records.diamonds_earned IS '本次签到获得钻石';
COMMENT ON COLUMN sign_records.created_at IS '签到时间';
CREATE INDEX idx_sr_user_date ON sign_records(user_id, sign_date DESC);
-- ================= SWEET TALKS =================
CREATE TABLE sweet_talks (
    talk_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE sweet_talks IS '每日情话记录';
COMMENT ON COLUMN sweet_talks.talk_id IS '情话主键ID';
COMMENT ON COLUMN sweet_talks.user_id IS '发送者用户ID';
COMMENT ON COLUMN sweet_talks.group_id IS '所属关联组ID';
COMMENT ON COLUMN sweet_talks.content IS '情话内容';
COMMENT ON COLUMN sweet_talks.created_at IS '发送时间';
CREATE INDEX idx_st_group_time ON sweet_talks(group_id, created_at DESC);
CREATE INDEX idx_st_user_today ON sweet_talks(user_id, (CAST(created_at AT TIME ZONE 'Asia/Shanghai' AS DATE)));
-- ================= WISHES =================
CREATE TABLE wishes (
    wish_id BIGSERIAL PRIMARY KEY,
    wish_name VARCHAR(128) NOT NULL,
    wish_cost INT NOT NULL,
    status wish_status_enum NOT NULL DEFAULT 'CREATED',
    created_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    claimed_by BIGINT REFERENCES users(user_id) ON DELETE SET NULL,
    claimed_at TIMESTAMPTZ,
    claim_cost INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wishes IS '心愿模板及状态';
COMMENT ON COLUMN wishes.wish_id IS '心愿ID';
COMMENT ON COLUMN wishes.wish_name IS '心愿名称';
COMMENT ON COLUMN wishes.wish_cost IS '心愿所需积分';
COMMENT ON COLUMN wishes.status IS '心愿状态: CREATED/CLAIMED/FINISHED/CLOSED';
COMMENT ON COLUMN wishes.created_by IS '创建者用户ID';
COMMENT ON COLUMN wishes.group_id IS '所属关联组ID';
COMMENT ON COLUMN wishes.claimed_by IS '兑换者用户ID';
COMMENT ON COLUMN wishes.claimed_at IS '兑换时间';
COMMENT ON COLUMN wishes.claim_cost IS '兑换时消耗积分';
COMMENT ON COLUMN wishes.created_at IS '创建时间';
COMMENT ON COLUMN wishes.updated_at IS '更新时间';
CREATE INDEX idx_wish_status ON wishes(status);
CREATE INDEX idx_wish_created_by ON wishes(created_by);
CREATE INDEX idx_wish_group ON wishes(group_id);
CREATE INDEX idx_wish_claimed_by ON wishes(claimed_by);

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
COMMENT ON COLUMN wish_feedbacks.feedback_id IS '反馈主键ID';
COMMENT ON COLUMN wish_feedbacks.wish_id IS '心愿ID';
COMMENT ON COLUMN wish_feedbacks.user_id IS '反馈用户ID(通常是兑换者)';
COMMENT ON COLUMN wish_feedbacks.content IS '反馈内容';
COMMENT ON COLUMN wish_feedbacks.images IS '反馈图片列表JSON';
COMMENT ON COLUMN wish_feedbacks.created_at IS '创建时间';
COMMENT ON COLUMN wish_feedbacks.updated_at IS '更新时间';
CREATE INDEX idx_wf_wish ON wish_feedbacks(wish_id);


-- ================= ORDER RATINGS =================
CREATE TABLE order_ratings (
    rating_id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
    rater_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    target_user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    delta INT NOT NULL,
    -- -5..5
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(order_id)
);
COMMENT ON TABLE order_ratings IS '订单完成后的评分加减分记录（唯一一次）';
COMMENT ON COLUMN order_ratings.rating_id IS '评分记录主键';
COMMENT ON COLUMN order_ratings.order_id IS '订单ID';
COMMENT ON COLUMN order_ratings.rater_user_id IS '评分发起者(下单用户)';
COMMENT ON COLUMN order_ratings.target_user_id IS '被评分用户(接单用户)';
COMMENT ON COLUMN order_ratings.delta IS '积分增减（-5..5，不为0）';
COMMENT ON COLUMN order_ratings.remark IS '评分备注';
COMMENT ON COLUMN order_ratings.created_at IS '评分时间';
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
COMMENT ON COLUMN lottery_draws.draw_id IS '抽奖记录主键';
COMMENT ON COLUMN lottery_draws.user_id IS '抽奖用户ID';
COMMENT ON COLUMN lottery_draws.food_types_requested IS '请求的菜品类型集合';
COMMENT ON COLUMN lottery_draws.request_payload IS '请求参数快照JSON';
COMMENT ON COLUMN lottery_draws.is_success IS '成功/失败标记';
COMMENT ON COLUMN lottery_draws.fail_reason IS '失败原因';
COMMENT ON COLUMN lottery_draws.created_at IS '创建时间';
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
COMMENT ON COLUMN lottery_draw_results.id IS '结果明细主键';
COMMENT ON COLUMN lottery_draw_results.draw_id IS '所属抽奖记录ID';
COMMENT ON COLUMN lottery_draw_results.food_type IS '菜品类型编号';
COMMENT ON COLUMN lottery_draw_results.food_id IS '菜品ID';
COMMENT ON COLUMN lottery_draw_results.food_name_snapshot IS '菜品名称快照';
COMMENT ON COLUMN lottery_draw_results.food_photo_snapshot IS '菜品图片快照';
COMMENT ON COLUMN lottery_draw_results.allocated_at IS '分配时间';
CREATE INDEX idx_ldr_draw ON lottery_draw_results(draw_id);
-- ================= MESSAGES =================
CREATE TABLE message_categories (
    category_id BIGSERIAL PRIMARY KEY,
    type_name VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE message_categories IS '消息类别';
COMMENT ON COLUMN message_categories.category_id IS '类别主键ID';
COMMENT ON COLUMN message_categories.type_name IS '类别类型唯一名称';
COMMENT ON COLUMN message_categories.display_name IS '显示名称';
COMMENT ON COLUMN message_categories.created_at IS '创建时间';
CREATE TABLE messages (
    message_id BIGSERIAL PRIMARY KEY,
    category_id BIGINT NOT NULL REFERENCES message_categories(category_id) ON DELETE CASCADE,
    sender_id BIGINT REFERENCES users(user_id) ON DELETE
    SET NULL,
        target_user_id BIGINT REFERENCES users(user_id) ON DELETE CASCADE,
        content TEXT NOT NULL,
        status message_status_enum NOT NULL DEFAULT 'ACTIVE',
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE messages IS '消息记录';
COMMENT ON COLUMN messages.message_id IS '消息主键ID';
COMMENT ON COLUMN messages.category_id IS '消息类别ID';
COMMENT ON COLUMN messages.sender_id IS '发送者用户ID';
COMMENT ON COLUMN messages.target_user_id IS '接收者用户ID';
COMMENT ON COLUMN messages.content IS '消息内容文本';
COMMENT ON COLUMN messages.status IS '消息状态';
COMMENT ON COLUMN messages.created_at IS '发送时间';
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
COMMENT ON COLUMN user_message_state.id IS '阅读状态主键';
COMMENT ON COLUMN user_message_state.user_id IS '用户ID';
COMMENT ON COLUMN user_message_state.category_id IS '消息类别ID';
COMMENT ON COLUMN user_message_state.last_read_at IS '最后阅读时间';
COMMENT ON COLUMN user_message_state.unread_count IS '未读数量';
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
COMMENT ON COLUMN feedback.feedback_id IS '反馈主键ID';
COMMENT ON COLUMN feedback.user_id IS '反馈用户ID';
COMMENT ON COLUMN feedback.content IS '反馈内容';
COMMENT ON COLUMN feedback.status IS '反馈处理状态';
COMMENT ON COLUMN feedback.reply IS '处理回复内容';
COMMENT ON COLUMN feedback.created_at IS '创建时间';
COMMENT ON COLUMN feedback.updated_at IS '更新时间';
CREATE INDEX idx_fb_status ON feedback(status);
-- ================= CART =================
CREATE TABLE carts (
    cart_id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    status cart_status_enum NOT NULL DEFAULT 'ACTIVE',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, status)
);
COMMENT ON TABLE carts IS '购物车主表';
COMMENT ON COLUMN carts.cart_id IS '购物车主键ID';
COMMENT ON COLUMN carts.user_id IS '用户ID';
COMMENT ON COLUMN carts.status IS '购物车状态';
COMMENT ON COLUMN carts.updated_at IS '更新时间';
CREATE TABLE cart_items (
    id BIGSERIAL PRIMARY KEY,
    cart_id BIGINT NOT NULL REFERENCES carts(cart_id) ON DELETE CASCADE,
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE RESTRICT,
    quantity INT NOT NULL DEFAULT 1,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE cart_items IS '购物车明细';
COMMENT ON COLUMN cart_items.id IS '明细主键ID';
COMMENT ON COLUMN cart_items.cart_id IS '购物车ID';
COMMENT ON COLUMN cart_items.food_id IS '菜品ID';
COMMENT ON COLUMN cart_items.quantity IS '数量';
COMMENT ON COLUMN cart_items.added_at IS '添加时间';
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
COMMENT ON TABLE food_stats IS '菜品统计宽表（缓存排行榜）';
COMMENT ON COLUMN food_stats.food_id IS '菜品ID';
COMMENT ON COLUMN food_stats.total_order_count IS '总下单次数';
COMMENT ON COLUMN food_stats.completed_order_count IS '完成订单次数';
COMMENT ON COLUMN food_stats.last_order_time IS '最近下单时间';
COMMENT ON COLUMN food_stats.last_complete_time IS '最近完成时间';
COMMENT ON COLUMN food_stats.updated_at IS '统计更新时间';
CREATE INDEX idx_fs_order_count ON food_stats(total_order_count);
CREATE INDEX idx_fs_complete_count ON food_stats(completed_order_count);
-- ================= WECHAT MINI-PROGRAM SUBSCRIPTION TEMPLATES =================
CREATE TABLE wx_subscription_templates (
    template_id BIGSERIAL PRIMARY KEY,
    template_code VARCHAR(128) NOT NULL UNIQUE,
    template_name VARCHAR(128) NOT NULL,
    wx_template_id VARCHAR(256) NOT NULL UNIQUE,
    -- 微信官方返回的模板ID
    description VARCHAR(255),
    -- 模板说明/用途描述
    is_active SMALLINT NOT NULL DEFAULT 1,
    -- 1激活 0停用
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wx_subscription_templates IS '微信小程序订阅消息模板';
COMMENT ON COLUMN wx_subscription_templates.template_id IS '本地模板主键ID';
COMMENT ON COLUMN wx_subscription_templates.template_code IS '模板编码(业务内部唯一标识,例如ORDER_REMIND)';
COMMENT ON COLUMN wx_subscription_templates.template_name IS '模板名称(用于管理界面显示)';
COMMENT ON COLUMN wx_subscription_templates.wx_template_id IS '微信官方返回的模板ID(用于API调用)';
COMMENT ON COLUMN wx_subscription_templates.description IS '模板说明/用途描述';
COMMENT ON COLUMN wx_subscription_templates.is_active IS '是否激活：1激活 0停用';
COMMENT ON COLUMN wx_subscription_templates.created_at IS '创建时间';
COMMENT ON COLUMN wx_subscription_templates.updated_at IS '更新时间';
CREATE INDEX idx_wst_code ON wx_subscription_templates(template_code);
CREATE INDEX idx_wst_active ON wx_subscription_templates(is_active);

-- ================= 模板数据 =================
-- -- 插入订单创建通知模板
-- INSERT INTO wx_subscription_templates (template_code, template_name, wx_template_id, description, is_active, created_at, updated_at)
-- VALUES (
--     'ORDER_CREATED',
--     '订单创建通知',
--     'UmBKohC4s3sni-E5fmpDp_xL_S6uhQ1yTaTl6LOtftI',
--     '当用户成功创建订单时发送通知',
--     1,
--     NOW(),
--     NOW()
-- );

-- -- 插入订单状态更新通知模板
-- INSERT INTO wx_subscription_templates (template_code, template_name, wx_template_id, description, is_active, created_at, updated_at)
-- VALUES (
--     'ORDER_STATUS_UPDATED',
--     '订单状态更新通知',
--     '3rkE-wK9z6Rxc_ffMx_fS4woy7iIDsxMcBUPsMWuFKI',
--     '当订单状态发生变化时发送通知（已接受、已完成、已取消等）',
--     1,
--     NOW(),
--     NOW()
-- );
-- ============================================
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
COMMENT ON COLUMN group_point_configs.group_id IS '关联组ID';
COMMENT ON COLUMN group_point_configs.breeder_closed_points IS '接单方主动关闭扣分(通常为负数)';
COMMENT ON COLUMN group_point_configs.confirmed_finished_points IS '下单方确认完成默认奖励(通常为正数)';
COMMENT ON COLUMN group_point_configs.confirmed_unfinished_points IS '下单方确认未完成扣分(通常为负数)';
COMMENT ON COLUMN group_point_configs.timeout_points IS '接单超时未接单扣分(通常为负数)';
COMMENT ON COLUMN group_point_configs.overdue_unfinished_points IS '逾期未完成扣分(通常为负数)';
COMMENT ON COLUMN group_point_configs.unlock_card_diamond_cost IS '解锁足迹容量消耗钻石数量';
COMMENT ON COLUMN group_point_configs.daily_checkin_rewards IS '每日签到奖励配置(仅管理员)';

-- ================= FOOTPRINT & DIAMONDS =================
CREATE TABLE user_diamond (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL UNIQUE REFERENCES users(user_id) ON DELETE CASCADE,
    diamond_balance INT NOT NULL DEFAULT 0,
    total_get INT NOT NULL DEFAULT 0,
    total_consume INT NOT NULL DEFAULT 0,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE user_diamond IS '用户钻石余额表';
COMMENT ON COLUMN user_diamond.user_id IS '用户ID';
COMMENT ON COLUMN user_diamond.diamond_balance IS '当前钻石余额';

CREATE TABLE diamond_flow (
    id BIGSERIAL PRIMARY KEY,
    flow_no VARCHAR(64) NOT NULL UNIQUE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    type SMALLINT NOT NULL, -- 1: 获取, 2: 消耗
    scene VARCHAR(32) NOT NULL, -- sign: 签到, record: 记录, share: 分享, expand: 扩容
    diamond_num INT NOT NULL,
    balance_after INT NOT NULL,
    relation_id BIGINT, -- 关联业务ID（如订单ID、记录ID等）
    remark VARCHAR(255),
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE diamond_flow IS '钻石流水记录表';
CREATE INDEX idx_diamond_flow_user_id ON diamond_flow(user_id);

CREATE TABLE record_group (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    group_name VARCHAR(50) NOT NULL,
    group_type SMALLINT NOT NULL, -- 1: 免费默认
    max_capacity INT NOT NULL DEFAULT 50,
    current_count INT NOT NULL DEFAULT 0,
    status SMALLINT NOT NULL DEFAULT 1, -- 0: 禁用, 1: 正常
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(group_id, group_name)
);
COMMENT ON TABLE record_group IS '足迹记录分组表（如：干饭日常）';
CREATE INDEX idx_record_group_group_id ON record_group(group_id);

CREATE TABLE user_record (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    record_group_id BIGINT NOT NULL REFERENCES record_group(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    order_id BIGINT UNIQUE REFERENCES orders(order_id) ON DELETE SET NULL,
    title VARCHAR(128),
    images TEXT NOT NULL, -- 图片URL，逗号分隔
    content TEXT,
    address VARCHAR(255),
    record_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    like_count INT NOT NULL DEFAULT 0,
    comment_count INT NOT NULL DEFAULT 0,
    is_draft SMALLINT NOT NULL DEFAULT 0, -- 0: 正式记录, 1: 草稿
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE user_record IS '用户足迹记录表';
CREATE INDEX idx_user_record_group_id ON user_record(group_id);
CREATE INDEX idx_user_record_rg_id ON user_record(record_group_id);

CREATE TABLE record_comment (
    id BIGSERIAL PRIMARY KEY,
    record_id BIGINT NOT NULL REFERENCES user_record(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content VARCHAR(500) NOT NULL,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE record_comment IS '记录评论表';

CREATE TABLE record_like (
    id BIGSERIAL PRIMARY KEY,
    record_id BIGINT NOT NULL REFERENCES user_record(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(record_id, user_id)
);
COMMENT ON TABLE record_like IS '记录点赞表';

-- ================= ACHIEVEMENTS =================
CREATE TABLE achievement_definitions (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(128) NOT NULL,
    icon VARCHAR(256),
    description TEXT,
    requirement_type VARCHAR(64) NOT NULL, -- e.g., 'FOOTPRINT_COUNT'
    requirement_value INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE achievement_definitions IS '成就/勋章定义表';

CREATE TABLE user_achievements (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    achievement_id BIGINT NOT NULL REFERENCES achievement_definitions(id) ON DELETE CASCADE,
    unlocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, achievement_id)
);
COMMENT ON TABLE user_achievements IS '用户成就解锁记录表';


-- ================== MEMORIAL DAY ==================
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
COMMENT ON COLUMN memorial_day.id IS '纪念日主键ID';
COMMENT ON COLUMN memorial_day.group_id IS '所属关联组ID';
COMMENT ON COLUMN memorial_day.name IS '纪念日名称';
COMMENT ON COLUMN memorial_day.description IS '纪念日描述';
COMMENT ON COLUMN memorial_day.memorial_date IS '纪念日日期';
COMMENT ON COLUMN memorial_day.is_default IS '是否默认纪念日';
COMMENT ON COLUMN memorial_day.calendar_type IS '日历类型: SOLAR(公历), LUNAR(农历)';
COMMENT ON COLUMN memorial_day.lunar_month IS '农历月(1-12)';
COMMENT ON COLUMN memorial_day.lunar_day IS '农历日(1-30)';
COMMENT ON COLUMN memorial_day.is_leap_month IS '是否是闰月';
COMMENT ON COLUMN memorial_day.created_at IS '创建时间';
COMMENT ON COLUMN memorial_day.updated_at IS '更新时间';