-- Enforce the canonical 1..=1_000_000 range for every persisted wish price.
-- NOT VALID avoids a blocking table scan while each constraint is installed;
-- VALIDATE then checks historical rows without silently rewriting business data.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'wishes_wish_cost_range_check'
          AND conrelid = 'wishes'::regclass
    ) THEN
        ALTER TABLE wishes ADD CONSTRAINT wishes_wish_cost_range_check
            CHECK (wish_cost BETWEEN 1 AND 1000000) NOT VALID;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'wishes_initial_cost_range_check'
          AND conrelid = 'wishes'::regclass
    ) THEN
        ALTER TABLE wishes ADD CONSTRAINT wishes_initial_cost_range_check
            CHECK (initial_cost BETWEEN 1 AND 1000000) NOT VALID;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'wishes_final_cost_range_check'
          AND conrelid = 'wishes'::regclass
    ) THEN
        ALTER TABLE wishes ADD CONSTRAINT wishes_final_cost_range_check
            CHECK (final_cost BETWEEN 1 AND 1000000) NOT VALID;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'wishes_claim_cost_range_check'
          AND conrelid = 'wishes'::regclass
    ) THEN
        ALTER TABLE wishes ADD CONSTRAINT wishes_claim_cost_range_check
            CHECK (claim_cost BETWEEN 1 AND 1000000) NOT VALID;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'wish_negotiations_cost_range_check'
          AND conrelid = 'wish_negotiations'::regclass
    ) THEN
        ALTER TABLE wish_negotiations ADD CONSTRAINT wish_negotiations_cost_range_check
            CHECK (cost BETWEEN 1 AND 1000000) NOT VALID;
    END IF;
END
$$;

ALTER TABLE wishes
    VALIDATE CONSTRAINT wishes_wish_cost_range_check;
ALTER TABLE wishes
    VALIDATE CONSTRAINT wishes_initial_cost_range_check;
ALTER TABLE wishes
    VALIDATE CONSTRAINT wishes_final_cost_range_check;
ALTER TABLE wishes
    VALIDATE CONSTRAINT wishes_claim_cost_range_check;
ALTER TABLE wish_negotiations
    VALIDATE CONSTRAINT wish_negotiations_cost_range_check;
