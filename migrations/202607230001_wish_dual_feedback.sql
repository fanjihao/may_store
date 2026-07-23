-- 双人心愿闭环:
-- 1. 双方同意进入心愿池时冻结积分
-- 2. 同一履约人在同组同时只能领取一个心愿
-- 3. 每个参与方各自保存一条可编辑反馈
-- 4. 履约方打卡后 48 小时未验收可惰性自动完成

ALTER TABLE wishes
    ADD COLUMN IF NOT EXISTS points_frozen_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS creator_checkin_due_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS auto_completed_at TIMESTAMPTZ;

-- 标记历史上已经产生 FREEZE 流水的心愿，兼容旧版“领取时冻结”数据。
UPDATE wishes w
SET points_frozen_at = frozen.first_frozen_at
FROM (
    SELECT biz_id AS wish_id, MIN(created_at) AS first_frozen_at
    FROM love_point_transactions
    WHERE biz_type = 'wish'
      AND type = 'FREEZE'::love_point_tx_type_enum
      AND biz_id IS NOT NULL
    GROUP BY biz_id
) frozen
WHERE w.wish_id = frozen.wish_id
  AND w.points_frozen_at IS NULL;

-- 旧流程完成心愿时只改变状态，没有把冻结额从 frozen_love_point 正式结算掉。
-- 为每条历史 FINISHED 心愿补一笔幂等 DEDUCT 流水（可用余额不再次减少）。
WITH outstanding AS (
    SELECT w.wish_id,
           COALESCE(w.requester_id, w.created_by) AS requester_id,
           w.group_id,
           COALESCE(
               SUM(CASE
                   WHEN lpt.type = 'FREEZE'::love_point_tx_type_enum THEN lpt.amount
                   WHEN lpt.type IN (
                       'UNFREEZE'::love_point_tx_type_enum,
                       'DEDUCT'::love_point_tx_type_enum
                   ) THEN -lpt.amount
                   ELSE 0
               END),
               0
           )::BIGINT AS amount
    FROM wishes w
    JOIN love_point_transactions lpt
      ON lpt.biz_type = 'wish' AND lpt.biz_id = w.wish_id
    WHERE w.status = 'FINISHED'::wish_status_enum
    GROUP BY w.wish_id, COALESCE(w.requester_id, w.created_by), w.group_id
)
INSERT INTO love_point_transactions
    (user_id, group_id, type, amount, available_before, available_after,
     frozen_before, frozen_after, biz_type, biz_id, idempotency_key, created_at)
SELECT o.requester_id,
       o.group_id,
       'DEDUCT'::love_point_tx_type_enum,
       o.amount,
       u.love_point,
       u.love_point,
       o.amount,
       0,
       'wish',
       o.wish_id,
       'wish_finish_' || o.wish_id,
       NOW()
FROM outstanding o
JOIN users u ON u.user_id = o.requester_id
WHERE o.amount > 0
ON CONFLICT (idempotency_key) WHERE idempotency_key IS NOT NULL
DO NOTHING;

-- 只重算存在心愿冻结流水的组积分行，清理旧版遗留的冻结余额。
UPDATE user_group_points ugp
SET frozen_love_point = GREATEST(
        COALESCE((
            SELECT SUM(CASE
                WHEN lpt.type = 'FREEZE'::love_point_tx_type_enum THEN lpt.amount
                WHEN lpt.type = 'UNFREEZE'::love_point_tx_type_enum THEN -lpt.amount
                WHEN lpt.type = 'DEDUCT'::love_point_tx_type_enum
                     AND LOWER(lpt.biz_type) = 'wish' THEN -lpt.amount
                ELSE 0
            END)
            FROM love_point_transactions lpt
            WHERE lpt.user_id = ugp.user_id
              AND lpt.group_id = ugp.group_id
        ), 0),
        0
    ),
    updated_at = NOW()
WHERE EXISTS (
    SELECT 1 FROM love_point_transactions lpt
    WHERE lpt.user_id = ugp.user_id
      AND lpt.group_id = ugp.group_id
      AND lpt.type = 'FREEZE'::love_point_tx_type_enum
);

ALTER TABLE wish_feedbacks
    ADD COLUMN IF NOT EXISTS role_snapshot VARCHAR(32);

UPDATE wish_feedbacks wf
SET role_snapshot = CASE
    WHEN wf.user_id = COALESCE(w.requester_id, w.created_by) THEN 'REQUESTER'
    WHEN wf.user_id = COALESCE(w.fulfiller_id, w.claimed_by) THEN 'FULFILLER'
    ELSE role_snapshot
END
FROM wishes w
WHERE w.wish_id = wf.wish_id
  AND wf.role_snapshot IS NULL;

ALTER TABLE wish_feedbacks
    DROP CONSTRAINT IF EXISTS wish_feedbacks_wish_id_key;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_wish_feedback_user
    ON wish_feedbacks(wish_id, user_id);

CREATE INDEX IF NOT EXISTS idx_wish_feedback_role
    ON wish_feedbacks(wish_id, role_snapshot);

-- 旧版由创建人执行 select，claimed_by/selected_by 因而记录的是创建人。
-- 新版这两个字段均表示实际领取的履约人，先归一化再建立活跃领取唯一索引。
UPDATE wishes
SET claimed_by = fulfiller_id,
    selected_by = fulfiller_id,
    updated_at = NOW()
WHERE status IN ('CLAIMED'::wish_status_enum, 'FINISHED'::wish_status_enum)
  AND fulfiller_id IS NOT NULL
  AND (
      claimed_by IS NULL
      OR claimed_by = COALESCE(requester_id, created_by)
  );

-- 历史数据若同一履约人在同组有多条 CLAIMED，保留最近一条；
-- 其余回到心愿池，积分继续保持冻结，避免错误退款。
WITH ranked_claims AS (
    SELECT wish_id,
           ROW_NUMBER() OVER (
               PARTITION BY group_id, claimed_by
               ORDER BY COALESCE(selected_at, claimed_at, updated_at) DESC, wish_id DESC
           ) AS rn
    FROM wishes
    WHERE status = 'CLAIMED'::wish_status_enum
      AND claimed_by IS NOT NULL
)
UPDATE wishes w
SET status = 'CREATED'::wish_status_enum,
    claimed_by = NULL,
    claimed_at = NULL,
    selected_by = NULL,
    selected_at = NULL,
    claim_cost = NULL,
    fulfillment_due_at = NULL,
    fulfilled_at = NULL,
    creator_checkin_due_at = NULL,
    updated_at = NOW()
FROM ranked_claims r
WHERE w.wish_id = r.wish_id
  AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_active_claim_per_fulfiller
    ON wishes(group_id, claimed_by)
    WHERE status = 'CLAIMED'::wish_status_enum
      AND claimed_by IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uniq_wish_point_settlement
    ON love_point_transactions(biz_id)
    WHERE biz_type = 'wish'
      AND type = 'DEDUCT'::love_point_tx_type_enum
      AND biz_id IS NOT NULL;
