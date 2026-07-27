-- association_groups.member_count 是兼容字段，以 ACTIVE 成员表为权威来源。

UPDATE association_groups g
SET member_count = (
    SELECT COUNT(*)::INT
    FROM association_group_members m
    WHERE m.group_id = g.group_id
      AND m.member_status = 'ACTIVE'::group_member_status_enum
);

CREATE OR REPLACE FUNCTION sync_group_member_count()
RETURNS TRIGGER AS $$
DECLARE
    affected_group_id BIGINT;
BEGIN
    affected_group_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.group_id ELSE NEW.group_id END;

    UPDATE association_groups g
    SET member_count = (
        SELECT COUNT(*)::INT
        FROM association_group_members m
        WHERE m.group_id = affected_group_id
          AND m.member_status = 'ACTIVE'::group_member_status_enum
    )
    WHERE g.group_id = affected_group_id;

    IF TG_OP = 'UPDATE' AND OLD.group_id <> NEW.group_id THEN
        UPDATE association_groups g
        SET member_count = (
            SELECT COUNT(*)::INT
            FROM association_group_members m
            WHERE m.group_id = OLD.group_id
              AND m.member_status = 'ACTIVE'::group_member_status_enum
        )
        WHERE g.group_id = OLD.group_id;
    END IF;

    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_group_member_count ON association_group_members;
CREATE TRIGGER trg_sync_group_member_count
AFTER INSERT OR UPDATE OF group_id, member_status OR DELETE
ON association_group_members
FOR EACH ROW EXECUTE FUNCTION sync_group_member_count();
