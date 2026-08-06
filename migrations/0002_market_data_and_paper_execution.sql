CREATE TABLE instruments (
    id UUID PRIMARY KEY,
    exchange TEXT NOT NULL,
    market_kind TEXT NOT NULL CHECK (market_kind IN ('spot', 'perpetual')),
    symbol TEXT NOT NULL,
    exchange_symbol TEXT NOT NULL,
    base_asset TEXT NOT NULL,
    quote_asset TEXT NOT NULL,
    settle_asset TEXT,
    contract_kind TEXT CHECK (contract_kind IN ('linear', 'inverse')),
    contract_size NUMERIC(38, 18),
    price_tick NUMERIC(38, 18) NOT NULL CHECK (price_tick > 0),
    quantity_step NUMERIC(38, 18) NOT NULL CHECK (quantity_step > 0),
    minimum_quantity NUMERIC(38, 18),
    minimum_notional NUMERIC(38, 18),
    maker_fee_rate NUMERIC(38, 18),
    taker_fee_rate NUMERIC(38, 18),
    active BOOLEAN NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (exchange, market_kind, symbol),
    CHECK (
        (market_kind = 'spot' AND contract_kind IS NULL AND contract_size IS NULL)
        OR
        (market_kind = 'perpetual' AND contract_kind IS NOT NULL AND contract_size > 0)
    )
);

CREATE INDEX instruments_lookup_idx
    ON instruments (exchange, market_kind, active, base_asset, quote_asset);

CREATE TABLE exchange_health (
    exchange TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('idle', 'ready', 'degraded')),
    last_error_code TEXT,
    last_success_at TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL
);

ALTER TABLE execution_tasks
    DROP COLUMN exchange,
    DROP COLUMN market_kind,
    DROP COLUMN symbol,
    DROP COLUMN target_quantity,
    ADD COLUMN instrument_id UUID NOT NULL REFERENCES instruments(id),
    ADD COLUMN idempotency_key TEXT NOT NULL UNIQUE,
    ADD COLUMN execution_mode TEXT NOT NULL DEFAULT 'paper'
        CHECK (execution_mode IN ('paper', 'live')),
    ADD COLUMN requested_amount NUMERIC(38, 18) NOT NULL CHECK (requested_amount > 0),
    ADD COLUMN quantity_unit TEXT NOT NULL
        CHECK (quantity_unit IN ('base_quantity', 'quote_notional', 'contracts')),
    ADD COLUMN executed_amount NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (executed_amount >= 0),
    ADD COLUMN residual_amount NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (residual_amount >= 0),
    ADD COLUMN max_slippage_bps INTEGER NOT NULL CHECK (max_slippage_bps BETWEEN 0 AND 10000),
    ADD COLUMN slice_interval_ms BIGINT NOT NULL CHECK (slice_interval_ms > 0),
    ADD COLUMN next_tick_at TIMESTAMPTZ NOT NULL,
    ADD COLUMN last_tick_at TIMESTAMPTZ,
    ADD COLUMN paused_reason TEXT,
    ADD COLUMN failure_code TEXT,
    ADD CONSTRAINT execution_tasks_status_check CHECK (
        status IN (
            'scheduled', 'running', 'paused', 'cancelling',
            'completed', 'cancelled', 'expired', 'failed'
        )
    );

ALTER TABLE execution_tasks
    DROP COLUMN executed_quantity;

CREATE INDEX execution_tasks_scheduler_idx
    ON execution_tasks (status, next_tick_at, deadline_at);

CREATE TABLE execution_slices (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES execution_tasks(id),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    requested_amount NUMERIC(38, 18) NOT NULL CHECK (requested_amount > 0),
    native_quantity NUMERIC(38, 18) NOT NULL CHECK (native_quantity > 0),
    filled_native_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (filled_native_quantity >= 0),
    filled_base_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (filled_base_quantity >= 0),
    filled_quote_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (filled_quote_quantity >= 0),
    average_price NUMERIC(38, 18),
    worst_price NUMERIC(38, 18),
    slippage_bps NUMERIC(20, 8),
    fee_amount NUMERIC(38, 18),
    fee_asset TEXT,
    fee_status TEXT NOT NULL CHECK (fee_status IN ('calculated', 'unavailable')),
    status TEXT NOT NULL CHECK (status IN ('filled', 'partial', 'unfilled', 'rejected')),
    market_snapshot JSONB NOT NULL,
    decision_input JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (task_id, sequence)
);

CREATE INDEX execution_slices_task_sequence_idx
    ON execution_slices (task_id, sequence);
