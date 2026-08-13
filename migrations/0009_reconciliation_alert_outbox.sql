ALTER TABLE outbox_commands
    ADD COLUMN claim_token UUID,
    ADD COLUMN claimed_until TIMESTAMPTZ;

CREATE INDEX outbox_commands_claimable_idx
    ON outbox_commands (available_at, created_at)
    WHERE status IN ('pending', 'processing');

CREATE TABLE reconciliation_alert_states (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id),
    fingerprint TEXT NOT NULL,
    last_run_id UUID NOT NULL REFERENCES reconciliation_runs(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
