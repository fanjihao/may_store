-- 历史订单的奖励快照字段可能未同步，但经济流水已经真实发放。
-- 以 ORDER / ORDER_REWARD_REVIEW 的 EARN 流水为权威值回填展示字段。

WITH point_rewards AS (
    SELECT biz_id AS order_id, SUM(amount)::INT AS reward
    FROM love_point_transactions
    WHERE biz_id IS NOT NULL
      AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW')
      AND type = 'EARN'::love_point_tx_type_enum
    GROUP BY biz_id
)
UPDATE orders o
SET points_reward = rewards.reward,
    point_grant_status = CASE
        WHEN o.point_grant_status = 'NONE'::point_grant_status_enum
            THEN 'GRANTED'::point_grant_status_enum
        ELSE o.point_grant_status
    END,
    updated_at = NOW()
FROM point_rewards rewards
WHERE o.order_id = rewards.order_id
  AND (
      o.points_reward IS DISTINCT FROM rewards.reward
      OR o.point_grant_status = 'NONE'::point_grant_status_enum
  );

WITH exp_rewards AS (
    SELECT biz_id AS order_id, SUM(amount)::INT AS reward
    FROM group_exp_transactions
    WHERE biz_id IS NOT NULL
      AND UPPER(biz_type) IN ('ORDER', 'ORDER_REWARD_REVIEW')
      AND type = 'EARN'::group_exp_tx_type_enum
    GROUP BY biz_id
)
UPDATE orders o
SET group_exp_reward = rewards.reward,
    exp_grant_status = CASE
        WHEN o.exp_grant_status = 'NONE'::exp_grant_status_enum
            THEN 'GRANTED'::exp_grant_status_enum
        ELSE o.exp_grant_status
    END,
    updated_at = NOW()
FROM exp_rewards rewards
WHERE o.order_id = rewards.order_id
  AND (
      o.group_exp_reward IS DISTINCT FROM rewards.reward
      OR o.exp_grant_status = 'NONE'::exp_grant_status_enum
  );
