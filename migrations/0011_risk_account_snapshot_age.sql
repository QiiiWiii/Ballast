ALTER TABLE risk_limits
    ADD COLUMN max_account_age_ms BIGINT NOT NULL DEFAULT 0
        CHECK (max_account_age_ms >= 0);
