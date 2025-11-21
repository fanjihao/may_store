-- Migration: Change wishes.created_by from user_id to group_id
-- Date: 2025-11-20
-- Description: 心愿应该属于团队而不是个人用户

-- Step 1: 删除原有外键约束
ALTER TABLE wishes DROP CONSTRAINT IF EXISTS wishes_created_by_fkey;

-- Step 2: 添加新的外键约束指向 association_groups
ALTER TABLE wishes 
  ADD CONSTRAINT wishes_created_by_fkey 
  FOREIGN KEY (created_by) 
  REFERENCES association_groups(group_id) 
  ON DELETE RESTRICT;

-- Step 3: 更新注释
COMMENT ON COLUMN wishes.created_by IS '创建者团队ID（关联组ID）';

-- Step 4: 添加索引优化查询
CREATE INDEX IF NOT EXISTS idx_wish_created_by ON wishes(created_by);
