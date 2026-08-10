CREATE TABLE replay_runs (
    id UUID PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    template_version_id UUID NOT NULL REFERENCES strategy_template_versions(id),
    strategy_kind TEXT NOT NULL CHECK (strategy_kind IN ('twap', 'pov')),
    side TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    requested_amount NUMERIC(38, 18) NOT NULL CHECK (requested_amount > 0),
    quantity_unit TEXT NOT NULL CHECK (quantity_unit IN ('base_quantity', 'quote_notional', 'contracts')),
    start_at TIMESTAMPTZ NOT NULL,
    end_at TIMESTAMPTZ NOT NULL,
    execution_model TEXT NOT NULL CHECK (execution_model = 'trade_vwap_proxy'),
    model_version TEXT NOT NULL,
    fee_rate NUMERIC(38, 18) NOT NULL CHECK (fee_rate >= 0),
    extra_slippage_bps NUMERIC(38, 18) NOT NULL CHECK (extra_slippage_bps >= 0),
    gap_threshold_seconds BIGINT NOT NULL CHECK (gap_threshold_seconds > 0),
    strategy_snapshot JSONB NOT NULL,
    request_fingerprint TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('completed', 'completed_with_warnings', 'failed')),
    failure_code TEXT,
    data_first_at TIMESTAMPTZ,
    data_last_at TIMESTAMPTZ,
    trade_count BIGINT NOT NULL CHECK (trade_count >= 0),
    gap_count INTEGER NOT NULL CHECK (gap_count >= 0),
    data_gaps JSONB NOT NULL,
    confidence TEXT NOT NULL CHECK (confidence IN ('high', 'limited', 'unusable')),
    limitations JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (start_at < end_at),
    CHECK ((status = 'failed') = (failure_code IS NOT NULL))
);

CREATE INDEX replay_runs_instrument_time_idx
    ON replay_runs (instrument_id, start_at, end_at, created_at DESC);

CREATE TABLE replay_slices (
    run_id UUID NOT NULL REFERENCES replay_runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('filled', 'partial', 'empty', 'skipped')),
    trade_count BIGINT NOT NULL CHECK (trade_count >= 0),
    market_volume NUMERIC(38, 18) NOT NULL CHECK (market_volume >= 0),
    requested_amount NUMERIC(38, 18) NOT NULL CHECK (requested_amount >= 0),
    filled_amount NUMERIC(38, 18) NOT NULL CHECK (filled_amount >= 0),
    filled_native_quantity NUMERIC(38, 18) NOT NULL CHECK (filled_native_quantity >= 0),
    market_vwap NUMERIC(38, 18),
    simulated_price NUMERIC(38, 18),
    fee_amount NUMERIC(38, 18) NOT NULL CHECK (fee_amount >= 0),
    decision_input JSONB NOT NULL,
    PRIMARY KEY (run_id, sequence),
    CHECK (window_start < window_end)
);

CREATE TABLE replay_metrics (
    run_id UUID PRIMARY KEY REFERENCES replay_runs(id) ON DELETE CASCADE,
    arrival_price NUMERIC(38, 18),
    market_vwap NUMERIC(38, 18),
    simulated_execution_vwap NUMERIC(38, 18),
    implementation_shortfall_bps NUMERIC(38, 18),
    implementation_shortfall_amount NUMERIC(38, 18),
    requested_amount NUMERIC(38, 18) NOT NULL CHECK (requested_amount > 0),
    filled_amount NUMERIC(38, 18) NOT NULL CHECK (filled_amount >= 0),
    fill_rate NUMERIC(38, 18) NOT NULL CHECK (fill_rate >= 0),
    residual_amount NUMERIC(38, 18) NOT NULL CHECK (residual_amount >= 0),
    actual_participation_rate NUMERIC(38, 18),
    target_participation_rate NUMERIC(38, 18),
    participation_rate_deviation NUMERIC(38, 18),
    slice_count INTEGER NOT NULL CHECK (slice_count >= 0),
    empty_window_count INTEGER NOT NULL CHECK (empty_window_count >= 0),
    fee_amount NUMERIC(38, 18) NOT NULL CHECK (fee_amount >= 0),
    explicit_slippage_amount NUMERIC(38, 18) NOT NULL CHECK (explicit_slippage_amount >= 0)
);
