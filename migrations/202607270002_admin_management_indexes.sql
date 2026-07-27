-- 管理后台订单、心愿与审计日志游标查询索引。

CREATE INDEX IF NOT EXISTS idx_orders_pending_grant_review
    ON orders(created_at DESC, order_id DESC)
    WHERE point_grant_status = 'PENDING_REVIEW'::point_grant_status_enum
       OR exp_grant_status = 'PENDING_REVIEW'::exp_grant_status_enum;

CREATE INDEX IF NOT EXISTS idx_orders_risk_created
    ON orders(risk_status, created_at DESC, order_id DESC);

CREATE INDEX IF NOT EXISTS idx_wishes_pending_quality
    ON wishes(finished_at DESC, wish_id DESC)
    WHERE status = 'FINISHED'::wish_status_enum
      AND quality_review_status = 'NONE'::wish_quality_status_enum;

CREATE INDEX IF NOT EXISTS idx_audit_created_id
    ON audit_logs(created_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_audit_target_created
    ON audit_logs(target_type, target_id, created_at DESC);
