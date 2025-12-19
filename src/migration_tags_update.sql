-- =========================================================
-- Migration: Update Food Tags Logic & Add Ingredients/Steps
-- Date: 2025-12-19
-- Description: 
-- 1. Ensure `tags` has `group_id` and remove `ingredients`/`steps` from `tags`.
-- 2. Change `foods` to have a single `tag_id` (Many-to-One).
-- 3. Add `ingredients` and `steps` to `foods`.
-- 4. Remove `food_types` and `food_tags_map`.
-- =========================================================

BEGIN;

-- 1. Update `tags` table
-- Ensure group_id exists (it should, but just in case)
ALTER TABLE tags ADD COLUMN IF NOT EXISTS group_id BIGINT REFERENCES association_groups(group_id) ON DELETE CASCADE;
-- Remove ingredients/steps from tags if they exist (as per user request)
ALTER TABLE tags DROP COLUMN IF EXISTS ingredients;
ALTER TABLE tags DROP COLUMN IF EXISTS steps;

-- 2. Update `foods` table
ALTER TABLE foods ADD COLUMN IF NOT EXISTS tag_id BIGINT REFERENCES tags(tag_id) ON DELETE SET NULL;
ALTER TABLE foods ADD COLUMN IF NOT EXISTS ingredients TEXT;
ALTER TABLE foods ADD COLUMN IF NOT EXISTS steps TEXT;

-- 3. Data Migration (Best Effort)
-- Try to migrate existing tags from the map table to the new column.
-- We pick the first tag found for each food as the primary tag.
DO $$
BEGIN
    IF EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'food_tags_map') THEN
        UPDATE foods f
        SET tag_id = (
            SELECT tag_id 
            FROM food_tags_map m 
            WHERE m.food_id = f.food_id 
            ORDER BY m.tag_id ASC 
            LIMIT 1
        )
        WHERE f.tag_id IS NULL;
    END IF;
END $$;

-- 4. Drop old structures
DROP TABLE IF EXISTS food_tags_map;
ALTER TABLE foods DROP COLUMN IF EXISTS food_types;

COMMIT;
