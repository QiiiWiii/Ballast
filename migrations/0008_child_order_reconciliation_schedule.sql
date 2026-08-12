ALTER TABLE child_orders
    ADD COLUMN reconciliation_due_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN reconciliation_claim_token UUID,
    ADD COLUMN reconciliation_claimed_until TIMESTAMPTZ,
    ADD COLUMN reconciliation_failure_count INTEGER NOT NULL DEFAULT 0
        CHECK (reconciliation_failure_count >= 0),
    ADD COLUMN last_reconciliation_error TEXT,
    ADD CONSTRAINT child_orders_reconciliation_claim_shape_check CHECK (
        (reconciliation_claim_token IS NULL) = (reconciliation_claimed_until IS NULL)
    );

CREATE INDEX child_orders_reconciliation_due_idx
    ON child_orders (reconciliation_due_at, created_at)
    WHERE status IN (
        'submission_unknown', 'open', 'partially_filled', 'cancel_pending'
    );
