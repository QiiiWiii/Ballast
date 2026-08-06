ALTER TABLE strategy_template_versions
    ADD COLUMN execution_backend TEXT NOT NULL DEFAULT 'managed_ioc'
        CHECK (execution_backend IN ('managed_ioc', 'venue_native_algo')),
    ADD COLUMN venue_exchange TEXT,
    ADD COLUMN venue_market_kind TEXT,
    ADD COLUMN native_algorithm TEXT,
    ADD COLUMN native_params JSONB;

ALTER TABLE strategy_template_versions
    ADD CONSTRAINT strategy_template_backend_shape_check CHECK (
        (
            execution_backend = 'managed_ioc'
            AND venue_exchange IS NULL
            AND venue_market_kind IS NULL
            AND native_algorithm IS NULL
            AND native_params IS NULL
        ) OR (
            execution_backend = 'venue_native_algo'
            AND venue_exchange IN ('binance', 'okx', 'bybit', 'gate_io', 'bitget')
            AND venue_market_kind IN ('spot', 'perpetual')
            AND native_algorithm IS NOT NULL
            AND native_params IS NOT NULL
        )
    );

ALTER TABLE execution_tasks DROP CONSTRAINT execution_tasks_status_check;
ALTER TABLE execution_tasks
    ADD COLUMN execution_backend TEXT NOT NULL DEFAULT 'managed_ioc'
        CHECK (execution_backend IN ('managed_ioc', 'venue_native_algo')),
    ADD COLUMN requested_by TEXT,
    ADD COLUMN approved_by TEXT,
    ADD COLUMN approved_at TIMESTAMPTZ,
    ADD COLUMN rejected_by TEXT,
    ADD COLUMN rejected_at TIMESTAMPTZ,
    ADD COLUMN rejection_reason TEXT,
    ADD CONSTRAINT execution_tasks_status_check CHECK (
        status IN (
            'pending_approval', 'scheduled', 'running', 'paused', 'cancelling',
            'completed', 'cancelled', 'expired', 'failed', 'rejected'
        )
    ),
    ADD CONSTRAINT execution_tasks_approval_check CHECK (
        (status <> 'pending_approval' OR requested_by IS NOT NULL)
        AND (status <> 'rejected' OR (rejected_by IS NOT NULL AND rejected_at IS NOT NULL))
        AND ((approved_at IS NULL) = (approved_by IS NULL))
        AND ((rejected_at IS NULL) = (rejected_by IS NULL))
        AND NOT (approved_by IS NOT NULL AND rejected_by IS NOT NULL)
        AND (approved_by IS NULL OR requested_by IS NULL OR approved_by <> requested_by)
    );

CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    exchange TEXT NOT NULL CHECK (exchange IN ('binance', 'okx', 'bybit', 'gate_io', 'bitget')),
    label TEXT NOT NULL,
    environment TEXT NOT NULL CHECK (environment IN ('demo', 'production')),
    secret_name TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    withdrawals_disabled BOOLEAN NOT NULL DEFAULT TRUE,
    ip_restricted BOOLEAN NOT NULL DEFAULT FALSE,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (exchange, label),
    CHECK (NOT enabled OR (withdrawals_disabled AND ip_restricted))
);

CREATE TABLE account_snapshots (
    id UUID PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    balances JSONB NOT NULL,
    positions JSONB NOT NULL,
    open_orders JSONB NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX account_snapshots_account_time_idx
    ON account_snapshots (account_id, observed_at DESC);

CREATE TABLE reconciliation_runs (
    id UUID PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    status TEXT NOT NULL CHECK (status IN ('running', 'matched', 'differences', 'failed')),
    difference_count INTEGER NOT NULL DEFAULT 0 CHECK (difference_count >= 0),
    differences JSONB NOT NULL DEFAULT '[]'::jsonb,
    failure_code TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE TABLE reconciliation_differences (
    id UUID PRIMARY KEY,
    reconciliation_run_id UUID NOT NULL REFERENCES reconciliation_runs(id),
    difference_type TEXT NOT NULL,
    external_reference TEXT,
    expected JSONB NOT NULL,
    observed JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE task_approvals (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES execution_tasks(id),
    action TEXT NOT NULL CHECK (action IN ('approved', 'rejected')),
    actor_id TEXT NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (task_id, action)
);

ALTER TABLE child_orders
    DROP CONSTRAINT child_orders_order_type_check,
    ADD COLUMN order_backend TEXT NOT NULL DEFAULT 'managed_ioc'
        CHECK (order_backend IN ('managed_ioc', 'venue_native_algo')),
    ADD COLUMN limit_price NUMERIC(38, 18),
    ADD COLUMN state_reason TEXT,
    ADD COLUMN last_reconciled_at TIMESTAMPTZ,
    ADD CONSTRAINT child_orders_order_type_check CHECK (order_type IN ('market', 'limit', 'limit_ioc')),
    ADD CONSTRAINT child_orders_status_check CHECK (
        status IN (
            'submission_pending', 'submission_unknown', 'open', 'partially_filled',
            'filled', 'cancel_pending', 'cancelled', 'rejected', 'expired', 'failed'
        )
    );

CREATE TABLE native_algo_orders (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES execution_tasks(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    exchange TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    request_id UUID NOT NULL UNIQUE,
    client_algo_id TEXT NOT NULL,
    exchange_algo_id TEXT,
    status TEXT NOT NULL CHECK (
        status IN (
            'submission_pending', 'submission_unknown', 'acknowledged', 'running',
            'cancelling', 'completed', 'cancelled', 'expired', 'failed'
        )
    ),
    requested_quantity NUMERIC(38, 18) NOT NULL CHECK (requested_quantity > 0),
    executed_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (executed_quantity >= 0),
    limit_price NUMERIC(38, 18) NOT NULL CHECK (limit_price > 0),
    submitted_config JSONB NOT NULL,
    state_reason TEXT,
    submitted_at TIMESTAMPTZ,
    last_reconciled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (exchange, account_id, client_algo_id)
);

CREATE UNIQUE INDEX native_algo_exchange_id_unique
    ON native_algo_orders (exchange, account_id, exchange_algo_id)
    WHERE exchange_algo_id IS NOT NULL;

CREATE TABLE native_algo_sub_orders (
    id UUID PRIMARY KEY,
    algo_order_id UUID NOT NULL REFERENCES native_algo_orders(id),
    exchange_sub_order_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN (
            'submission_pending', 'submission_unknown', 'open', 'partially_filled',
            'filled', 'cancel_pending', 'cancelled', 'rejected', 'expired', 'failed'
        )
    ),
    quantity NUMERIC(38, 18) NOT NULL CHECK (quantity > 0),
    filled_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (filled_quantity >= 0),
    average_price NUMERIC(38, 18),
    observed_at TIMESTAMPTZ NOT NULL,
    UNIQUE (algo_order_id, exchange_sub_order_id)
);

ALTER TABLE fills
    ALTER COLUMN child_order_id DROP NOT NULL,
    ADD COLUMN native_algo_sub_order_id UUID REFERENCES native_algo_sub_orders(id),
    ADD CONSTRAINT fills_order_source_check CHECK (
        (child_order_id IS NOT NULL AND native_algo_sub_order_id IS NULL)
        OR (child_order_id IS NULL AND native_algo_sub_order_id IS NOT NULL)
    );

CREATE TABLE kill_switches (
    scope_type TEXT NOT NULL CHECK (scope_type IN ('global', 'exchange', 'account')),
    scope_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    reason TEXT NOT NULL,
    changed_by TEXT NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (scope_type, scope_id)
);

CREATE TABLE risk_limits (
    id UUID PRIMARY KEY,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('global', 'exchange', 'account')),
    scope_id TEXT NOT NULL,
    allowed_exchanges JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_accounts JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_instruments JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_backends JSONB NOT NULL DEFAULT '[]'::jsonb,
    max_order_notional NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (max_order_notional >= 0),
    max_task_notional NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (max_task_notional >= 0),
    max_daily_notional NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (max_daily_notional >= 0),
    max_active_tasks INTEGER NOT NULL DEFAULT 0 CHECK (max_active_tasks >= 0),
    max_slippage_bps INTEGER NOT NULL DEFAULT 0 CHECK (max_slippage_bps >= 0),
    max_market_age_ms BIGINT NOT NULL DEFAULT 0 CHECK (max_market_age_ms >= 0),
    max_net_exposure NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (max_net_exposure >= 0),
    max_native_algo_orders INTEGER NOT NULL DEFAULT 0 CHECK (max_native_algo_orders >= 0),
    max_native_duration_seconds BIGINT NOT NULL DEFAULT 0 CHECK (max_native_duration_seconds >= 0),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (scope_type, scope_id)
);

CREATE TABLE risk_decisions (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    task_id UUID REFERENCES execution_tasks(id),
    account_id TEXT,
    stage TEXT NOT NULL CHECK (stage IN ('create', 'approve', 'submit', 'hedge')),
    decision TEXT NOT NULL CHECK (decision IN ('allowed', 'denied')),
    code TEXT NOT NULL,
    inputs JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE hedge_configs (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('active', 'archived')),
    primary_account_id TEXT NOT NULL REFERENCES accounts(id),
    primary_instrument_id UUID NOT NULL REFERENCES instruments(id),
    primary_template_version_id UUID NOT NULL REFERENCES strategy_template_versions(id),
    hedge_account_id TEXT NOT NULL REFERENCES accounts(id),
    hedge_instrument_id UUID NOT NULL REFERENCES instruments(id),
    hedge_template_version_id UUID NOT NULL REFERENCES strategy_template_versions(id),
    hedge_ratio NUMERIC(38, 18) NOT NULL CHECK (hedge_ratio > 0),
    max_residual NUMERIC(38, 18) NOT NULL CHECK (max_residual >= 0),
    max_naked_ms BIGINT NOT NULL CHECK (max_naked_ms > 0),
    native_batch_min_notional NUMERIC(38, 18) NOT NULL CHECK (native_batch_min_notional >= 0),
    native_batch_interval_ms BIGINT NOT NULL CHECK (native_batch_interval_ms > 0),
    max_active_native_algos INTEGER NOT NULL CHECK (max_active_native_algos >= 0),
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE hedge_runs (
    id UUID PRIMARY KEY,
    config_id UUID NOT NULL REFERENCES hedge_configs(id),
    primary_task_id UUID REFERENCES execution_tasks(id),
    status TEXT NOT NULL CHECK (
        status IN ('pending_approval', 'running', 'paused', 'completed', 'failed', 'manual_intervention', 'rejected')
    ),
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    accumulated_exposure NUMERIC(38, 18) NOT NULL DEFAULT 0,
    hedged_exposure NUMERIC(38, 18) NOT NULL DEFAULT 0,
    failure_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE hedge_exposure_batches (
    id UUID PRIMARY KEY,
    hedge_run_id UUID NOT NULL REFERENCES hedge_runs(id),
    hedge_task_id UUID REFERENCES execution_tasks(id),
    exposure_amount NUMERIC(38, 18) NOT NULL CHECK (exposure_amount > 0),
    status TEXT NOT NULL CHECK (status IN ('accumulating', 'submitted', 'hedged', 'failed')),
    first_fill_at TIMESTAMPTZ NOT NULL,
    submitted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE hedge_interventions (
    id UUID PRIMARY KEY,
    hedge_run_id UUID NOT NULL REFERENCES hedge_runs(id),
    reason TEXT NOT NULL,
    context JSONB NOT NULL,
    acknowledged_by TEXT,
    acknowledged_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ws_tickets (
    ticket_hash TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    roles JSONB NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX ws_tickets_expiry_idx ON ws_tickets (expires_at);
