-- 爱心积分不可为负数。
-- 历史负余额统一归零，并同步 users / user_group_points 两套兼容字段。

UPDATE users
SET love_point = 0,
    updated_at = NOW()
WHERE love_point < 0;

UPDATE user_group_points ugp
SET available_love_point = GREATEST(u.love_point, 0),
    love_point = GREATEST(u.love_point, 0),
    frozen_love_point = GREATEST(ugp.frozen_love_point, 0),
    updated_at = NOW()
FROM users u
WHERE u.user_id = ugp.user_id
  AND (
      ugp.available_love_point < 0
      OR ugp.love_point < 0
      OR ugp.frozen_love_point < 0
  );

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'users_love_point_nonnegative_check'
          AND conrelid = 'users'::regclass
    ) THEN
        ALTER TABLE users
            ADD CONSTRAINT users_love_point_nonnegative_check
            CHECK (love_point >= 0);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ugp_available_love_point_nonnegative_check'
          AND conrelid = 'user_group_points'::regclass
    ) THEN
        ALTER TABLE user_group_points
            ADD CONSTRAINT ugp_available_love_point_nonnegative_check
            CHECK (available_love_point >= 0);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ugp_love_point_nonnegative_check'
          AND conrelid = 'user_group_points'::regclass
    ) THEN
        ALTER TABLE user_group_points
            ADD CONSTRAINT ugp_love_point_nonnegative_check
            CHECK (love_point >= 0);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ugp_frozen_love_point_nonnegative_check'
          AND conrelid = 'user_group_points'::regclass
    ) THEN
        ALTER TABLE user_group_points
            ADD CONSTRAINT ugp_frozen_love_point_nonnegative_check
            CHECK (frozen_love_point >= 0);
    END IF;
END
$$;
