-- =========================================================
-- 一键迁移脚本：同步生产环境至 schema_v2 最新状态
-- 目标：增加足迹、钻石、成就、纪念日增强等功能，不删除现有数据
-- =========================================================

BEGIN;

-- 1. [表结构增强] 为现有表增加新字段
-- 增加关联组的容量统计
ALTER TABLE association_groups ADD COLUMN IF NOT EXISTS footprint_capacity INT NOT NULL DEFAULT 10;
ALTER TABLE association_groups ADD COLUMN IF NOT EXISTS footprint_count INT NOT NULL DEFAULT 0;
COMMENT ON COLUMN association_groups.footprint_capacity IS '足迹全局容量';
COMMENT ON COLUMN association_groups.footprint_count IS '当前足迹记录总数';

-- 增加积分配置项
ALTER TABLE group_point_configs ADD COLUMN IF NOT EXISTS unlock_card_diamond_cost INT NOT NULL DEFAULT 100;
ALTER TABLE group_point_configs ADD COLUMN IF NOT EXISTS daily_checkin_rewards INT[] NOT NULL DEFAULT '{5,6,7,8,9,10,20}';
COMMENT ON COLUMN group_point_configs.unlock_card_diamond_cost IS '解锁足迹容量消耗钻石数量';
COMMENT ON COLUMN group_point_configs.daily_checkin_rewards IS '每日签到奖励配置(仅管理员)';


-- 2. [钻石系统] 创建钻石余额与流水表
CREATE TABLE IF NOT EXISTS user_diamond (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL UNIQUE REFERENCES users(user_id) ON DELETE CASCADE,
    diamond_balance INT NOT NULL DEFAULT 0,
    total_get INT NOT NULL DEFAULT 0,
    total_consume INT NOT NULL DEFAULT 0,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
COMMENT ON TABLE user_diamond IS '用户钻石余额表';

CREATE TABLE IF NOT EXISTS diamond_flow (
    id BIGSERIAL PRIMARY KEY,
    flow_no VARCHAR(64) NOT NULL UNIQUE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    type SMALLINT NOT NULL, -- 1: 获取, 2: 消耗
    scene VARCHAR(32) NOT NULL, -- sign: 签到, record: 记录, share: 分享, expand: 扩容
    diamond_num INT NOT NULL,
    balance_after INT NOT NULL,
    relation_id BIGINT,
    remark VARCHAR(255),
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_diamond_flow_user_id ON diamond_flow(user_id);
COMMENT ON TABLE diamond_flow IS '钻石流水记录表';


-- 3. [足迹系统] 创建分组、记录、评论、点赞表
CREATE TABLE IF NOT EXISTS record_group (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES association_groups(group_id) ON DELETE CASCADE,
    group_name VARCHAR(50) NOT NULL,
    group_type SMALLINT NOT NULL, -- 1: 免费默认
    max_capacity INT NOT NULL DEFAULT 50,
    current_count INT NOT NULL DEFAULT 0,
    status SMALLINT NOT NULL DEFAULT 1,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    update_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(group_id, group_name)
);
CREATE INDEX IF NOT EXISTS idx_record_group_group_id ON record_group(group_id);
COMMENT ON TABLE record_group IS '足迹记录分组表';

CREATE TABLE IF NOT EXISTS user_record (
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
CREATE INDEX IF NOT EXISTS idx_user_record_group_id ON user_record(group_id);
CREATE INDEX IF NOT EXISTS idx_user_record_rg_id ON user_record(record_group_id);
COMMENT ON TABLE user_record IS '用户足迹记录表';

CREATE TABLE IF NOT EXISTS record_comment (
    id BIGSERIAL PRIMARY KEY,
    record_id BIGINT NOT NULL REFERENCES user_record(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    content VARCHAR(500) NOT NULL,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS record_like (
    id BIGSERIAL PRIMARY KEY,
    record_id BIGINT NOT NULL REFERENCES user_record(id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    create_time TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(record_id, user_id)
);


-- 4. [成就系统]
CREATE TABLE IF NOT EXISTS achievement_definitions (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(128) NOT NULL,
    icon VARCHAR(256),
    description TEXT,
    requirement_type VARCHAR(64) NOT NULL, 
    requirement_value INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_achievements (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    achievement_id BIGINT NOT NULL REFERENCES achievement_definitions(id) ON DELETE CASCADE,
    unlocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, achievement_id)
);


-- 5. [纪念日增强] 兼容农历字段
-- 注意：如果 memorial_day 表已存在，则只增加缺失列
DO $$ 
BEGIN 
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name='memorial_day') THEN
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
            is_default SMALLINT NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(group_id, name)
        );
    ELSE
        -- 如果表已存在，补全字段
        IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='memorial_day' AND column_name='calendar_type') THEN
            ALTER TABLE memorial_day ADD COLUMN calendar_type VARCHAR(16) NOT NULL DEFAULT 'SOLAR';
            ALTER TABLE memorial_day ADD COLUMN lunar_month SMALLINT;
            ALTER TABLE memorial_day ADD COLUMN lunar_day SMALLINT;
            ALTER TABLE memorial_day ADD COLUMN is_leap_month BOOLEAN DEFAULT FALSE;
            ALTER TABLE memorial_day ADD COLUMN is_default SMALLINT NOT NULL DEFAULT 0;
        END IF;
    END IF;
END $$;


-- 6. [数据初始化] 插入缺失的订阅模板（如果不存在）
INSERT INTO wx_subscription_templates (template_code, template_name, wx_template_id, description, is_active)
SELECT 'ORDER_CREATED', '订单创建通知', 'UmBKohC4s3sni-E5fmpDp_xL_S6uhQ1yTaTl6LOtftI', '当用户成功创建订单时发送通知', 1
WHERE NOT EXISTS (SELECT 1 FROM wx_subscription_templates WHERE template_code = 'ORDER_CREATED');

INSERT INTO wx_subscription_templates (template_code, template_name, wx_template_id, description, is_active)
SELECT 'ORDER_STATUS_UPDATED', '订单状态更新通知', '3rkE-wK9z6Rxc_ffMx_fS4woy7iIDsxMcBUPsMWuFKI', '当订单状态发生变化时发送通知', 1
WHERE NOT EXISTS (SELECT 1 FROM wx_subscription_templates WHERE template_code = 'ORDER_STATUS_UPDATED');

COMMIT;

-- 验证语句（执行后可运行查看结果）：
-- SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('user_record', 'user_diamond', 'achievement_definitions');
