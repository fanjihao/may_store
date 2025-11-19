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
    'PENDING',
    'ACCEPTED',
    'FINISHED',
    'CANCELLED',
    'EXPIRED',
    'REJECTED',
    'SYSTEM_CLOSED'
);
CREATE TYPE point_tx_type_enum AS ENUM (
    'ORDER_REWARD',
    'FINISH_REWARD',
    'WISH_COST',
    'ORDER_RATING',
    'ADMIN_ADJUST',
    'LOTTERY_REWARD',
    'OTHER'
);
CREATE TYPE wish_status_enum AS ENUM ('ON', 'OFF');
CREATE TYPE wish_claim_status_enum AS ENUM ('PROCESSING', 'DONE', 'CANCELLED');
CREATE TYPE lottery_success_enum AS ENUM ('SUCCESS', 'FAIL');
CREATE TYPE message_status_enum AS ENUM ('ACTIVE', 'REVOKED');
CREATE TYPE feedback_status_enum AS ENUM ('NEW', 'PROCESSING', 'CLOSED');
CREATE TYPE cart_status_enum AS ENUM ('ACTIVE', 'SETTLED', 'CLEARED');
CREATE TYPE mark_type_enum AS ENUM ('LIKE', 'NOT_RECOMMEND');
CREATE TYPE gender_enum AS ENUM ('MALE', 'FEMALE', 'OTHER', 'UNKNOWN');
CREATE TYPE login_method_enum AS ENUM ('PASSWORD', 'PHONE_CODE', 'OAUTH', 'MIXED');
-- ================= USERS =================
CREATE TABLE users (
    user_id BIGSERIAL PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    nick_name VARCHAR(64),
    email VARCHAR(128),
    role user_role_enum NOT NULL DEFAULT 'ORDERING',
    love_point INT NOT NULL DEFAULT 0,
    avatar VARCHAR(256) NOT NULL DEFAULT 'https://img95.699pic.com/xsj/0f/d0/fo.jpg',
    phone VARCHAR(32),
    associate_id BIGINT,
    status SMALLINT NOT NULL DEFAULT 1,
    -- 1正常 0禁用
    password_hash VARCHAR(255),
    password_algo VARCHAR(32),
    gender gender_enum NOT NULL DEFAULT 'UNKNOWN',
    birthday DATE,
    phone_verified BOOLEAN NOT NULL DEFAULT FALSE,
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
COMMENT ON COLUMN users.love_point IS '爱心积分(可奖励与兑换心愿)';
COMMENT ON COLUMN users.avatar IS '头像URL';
COMMENT ON COLUMN users.phone IS '手机号';
COMMENT ON COLUMN users.associate_id IS '关联ID（预留扩展绑定）';
COMMENT ON COLUMN users.status IS '状态：1正常 0禁用';
COMMENT ON COLUMN users.password_hash IS '哈希后的密码（永不存明文）';
COMMENT ON COLUMN users.password_algo IS '密码哈希算法标识';
COMMENT ON COLUMN users.gender IS '性别';
COMMENT ON COLUMN users.birthday IS '生日';
COMMENT ON COLUMN users.phone_verified IS '手机号是否已验证';
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
    -- 1活跃 0关闭
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE association_groups IS '用户关联组（绑定关系/团队）';
COMMENT ON COLUMN association_groups.group_id IS '组ID主键';
COMMENT ON COLUMN association_groups.group_name IS '组名称';
COMMENT ON COLUMN association_groups.group_type IS '组类型：PAIR/FAMILY/TEAM';
COMMENT ON COLUMN association_groups.status IS '状态：1活跃 0关闭';
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
COMMENT ON COLUMN association_group_requests.status IS '申请状态';
COMMENT ON COLUMN association_group_requests.remark IS '备注/理由';
COMMENT ON COLUMN association_group_requests.created_at IS '创建时间';
COMMENT ON COLUMN association_group_requests.handled_at IS '处理时间';
CREATE INDEX idx_agr_target_status ON association_group_requests(target_user_id, status);
-- ================= FOODS =================
CREATE TABLE foods (
    food_id BIGSERIAL PRIMARY KEY,
    food_name VARCHAR(128) NOT NULL,
    food_photo VARCHAR(256),
    food_types SMALLINT NOT NULL,
    -- 1早餐 2午餐 3下午茶 4晚餐
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
COMMENT ON COLUMN foods.food_types IS '类型：1早餐 2午餐 3下午茶 4晚餐';
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
CREATE INDEX idx_food_types ON foods(food_types);
CREATE TABLE tags (
    tag_id BIGSERIAL PRIMARY KEY,
    tag_name VARCHAR(64) NOT NULL UNIQUE,
    sort INT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE tags IS '菜品标签';
COMMENT ON COLUMN tags.tag_id IS '标签主键ID';
COMMENT ON COLUMN tags.tag_name IS '标签名称唯一';
COMMENT ON COLUMN tags.sort IS '排序值-越大越靠前';
COMMENT ON COLUMN tags.created_at IS '创建时间';
CREATE TABLE food_tags_map (
    food_id BIGINT NOT NULL REFERENCES foods(food_id) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tags(tag_id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (food_id, tag_id)
);
COMMENT ON TABLE food_tags_map IS '菜品与标签多对多映射';
COMMENT ON COLUMN food_tags_map.food_id IS '菜品ID';
COMMENT ON COLUMN food_tags_map.tag_id IS '标签ID';
COMMENT ON COLUMN food_tags_map.created_at IS '映射创建时间';
CREATE INDEX idx_ft_tag ON food_tags_map(tag_id);
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
    receiver_id BIGINT REFERENCES users(user_id) ON DELETE
    SET NULL,
        group_id BIGINT REFERENCES association_groups(group_id) ON DELETE
    SET NULL,
        status order_status_enum NOT NULL DEFAULT 'PENDING',
        goal_time TIMESTAMPTZ,
        points_cost INT NOT NULL DEFAULT 0,
        points_reward INT NOT NULL DEFAULT 0,
        cancel_reason VARCHAR(255),
        reject_reason VARCHAR(255),
        last_status_change_at TIMESTAMPTZ,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE orders IS '订单主表';
COMMENT ON COLUMN orders.order_id IS '订单主键ID';
COMMENT ON COLUMN orders.user_id IS '下单用户ID';
COMMENT ON COLUMN orders.receiver_id IS '接单用户ID';
COMMENT ON COLUMN orders.group_id IS '所属关联组ID';
COMMENT ON COLUMN orders.status IS '订单状态';
COMMENT ON COLUMN orders.goal_time IS '期望完成/消费时间';
COMMENT ON COLUMN orders.points_cost IS '积分成本（预留）';
COMMENT ON COLUMN orders.points_reward IS '奖励积分（完成时可能发放）';
COMMENT ON COLUMN orders.cancel_reason IS '取消原因';
COMMENT ON COLUMN orders.reject_reason IS '拒绝原因';
COMMENT ON COLUMN orders.last_status_change_at IS '最后状态变更时间';
COMMENT ON COLUMN orders.created_at IS '创建时间';
COMMENT ON COLUMN orders.updated_at IS '更新时间';
CREATE INDEX idx_order_user ON orders(user_id);
CREATE INDEX idx_order_receiver ON orders(receiver_id);
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
-- ================= WISHES =================
CREATE TABLE wishes (
    wish_id BIGSERIAL PRIMARY KEY,
    wish_name VARCHAR(128) NOT NULL,
    wish_cost INT NOT NULL,
    status wish_status_enum NOT NULL DEFAULT 'ON',
    created_by BIGINT NOT NULL REFERENCES users(user_id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wishes IS '心愿模板';
COMMENT ON COLUMN wishes.wish_id IS '心愿ID';
COMMENT ON COLUMN wishes.wish_name IS '心愿名称';
COMMENT ON COLUMN wishes.wish_cost IS '心愿所需积分';
COMMENT ON COLUMN wishes.status IS '心愿状态';
COMMENT ON COLUMN wishes.created_by IS '创建者用户ID';
COMMENT ON COLUMN wishes.created_at IS '创建时间';
COMMENT ON COLUMN wishes.updated_at IS '更新时间';
CREATE INDEX idx_wish_status ON wishes(status);
CREATE TABLE wish_claims (
    id BIGSERIAL PRIMARY KEY,
    wish_id BIGINT NOT NULL REFERENCES wishes(wish_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    cost INT NOT NULL,
    status wish_claim_status_enum NOT NULL DEFAULT 'PROCESSING',
    remark VARCHAR(255),
    fulfill_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wish_claims IS '心愿兑换记录';
COMMENT ON COLUMN wish_claims.id IS '兑换记录主键';
COMMENT ON COLUMN wish_claims.wish_id IS '心愿ID';
COMMENT ON COLUMN wish_claims.user_id IS '兑换用户ID';
COMMENT ON COLUMN wish_claims.cost IS '消耗积分';
COMMENT ON COLUMN wish_claims.status IS '兑换状态';
COMMENT ON COLUMN wish_claims.remark IS '备注';
COMMENT ON COLUMN wish_claims.fulfill_at IS '完成时间';
COMMENT ON COLUMN wish_claims.created_at IS '创建时间';
COMMENT ON COLUMN wish_claims.updated_at IS '更新时间';
CREATE INDEX idx_wc_user ON wish_claims(user_id);
CREATE INDEX idx_wc_status ON wish_claims(status);
CREATE TABLE wish_claim_checkins (
    id BIGSERIAL PRIMARY KEY,
    claim_id BIGINT NOT NULL REFERENCES wish_claims(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    photo_url VARCHAR(512),
    location_text VARCHAR(255),
    mood_text VARCHAR(128),
    feeling_text VARCHAR(255),
    checkin_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE wish_claim_checkins IS '心愿兑换后的打卡反馈记录';
COMMENT ON COLUMN wish_claim_checkins.id IS '打卡主键';
COMMENT ON COLUMN wish_claim_checkins.claim_id IS '兑换记录ID';
COMMENT ON COLUMN wish_claim_checkins.user_id IS '打卡用户ID';
COMMENT ON COLUMN wish_claim_checkins.photo_url IS '图片URL';
COMMENT ON COLUMN wish_claim_checkins.location_text IS '地点描述';
COMMENT ON COLUMN wish_claim_checkins.mood_text IS '心情标签/短语';
COMMENT ON COLUMN wish_claim_checkins.feeling_text IS '感受描述';
COMMENT ON COLUMN wish_claim_checkins.checkin_time IS '打卡时间';
COMMENT ON COLUMN wish_claim_checkins.created_at IS '记录创建时间';
CREATE INDEX idx_wcc_claim ON wish_claim_checkins(claim_id);
CREATE INDEX idx_wcc_user_time ON wish_claim_checkins(user_id, checkin_time);
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
-- ========= OPTIONAL TRIGGERS (COMMENTED OUT) =========
-- CREATE OR REPLACE FUNCTION touch_updated_at()
-- RETURNS trigger AS $$
-- BEGIN
--   NEW.updated_at = NOW();
--   RETURN NEW;
-- END; $$ LANGUAGE plpgsql;
-- Example: CREATE TRIGGER trg_touch_users BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
-- End of unified schema v2