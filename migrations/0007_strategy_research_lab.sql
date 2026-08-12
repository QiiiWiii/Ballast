ALTER TABLE instruments
    ADD COLUMN asset_class TEXT NOT NULL DEFAULT 'crypto'
        CHECK (asset_class IN ('crypto', 'equity')),
    ADD COLUMN instrument_kind TEXT,
    ADD COLUMN market_data_provider TEXT NOT NULL DEFAULT 'ccxt',
    ADD COLUMN market_data_dataset TEXT,
    ADD COLUMN execution_venue TEXT;

UPDATE instruments
SET instrument_kind = market_kind,
    execution_venue = exchange;

ALTER TABLE instruments
    ALTER COLUMN instrument_kind SET NOT NULL,
    ADD CONSTRAINT instruments_asset_shape_check CHECK (
        (asset_class = 'crypto' AND instrument_kind IN ('spot', 'perpetual') AND execution_venue IS NOT NULL)
        OR
        (asset_class = 'equity' AND instrument_kind = 'common_stock')
    );

ALTER TABLE strategy_templates
    ADD COLUMN scope TEXT NOT NULL DEFAULT 'paper_execution'
        CHECK (scope IN ('paper_execution', 'research'));

ALTER TABLE strategy_template_versions
    DROP CONSTRAINT strategy_template_versions_strategy_kind_check,
    DROP CONSTRAINT strategy_template_versions_check,
    ADD COLUMN algorithm_config JSONB;

UPDATE strategy_template_versions
SET algorithm_config = CASE strategy_kind
    WHEN 'twap' THEN jsonb_build_object(
        'algorithm', 'twap',
        'slice_interval_seconds', slice_interval_ms / 1000
    )
    WHEN 'pov' THEN jsonb_build_object(
        'algorithm', 'pov',
        'participation_rate', participation_rate::TEXT
    )
    ELSE NULL
END;

ALTER TABLE strategy_template_versions
    ALTER COLUMN algorithm_config SET NOT NULL,
    ADD CONSTRAINT strategy_template_versions_strategy_kind_check
        CHECK (strategy_kind IN ('immediate', 'twap', 'pov', 'vwap')),
    ADD CONSTRAINT strategy_template_versions_algorithm_config_check CHECK (
        algorithm_config->>'algorithm' = strategy_kind
    ),
    ADD CONSTRAINT strategy_template_versions_participation_check CHECK (
        (strategy_kind = 'pov' AND participation_rate > 0 AND participation_rate <= 1)
        OR
        (strategy_kind IN ('immediate', 'twap', 'vwap') AND participation_rate IS NULL)
    );

CREATE TABLE research_instruments (
    id UUID PRIMARY KEY,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL CHECK (asset_class IN ('equity')),
    instrument_kind TEXT NOT NULL CHECK (instrument_kind IN ('common_stock')),
    currency TEXT NOT NULL,
    market_data_provider TEXT NOT NULL,
    market_data_dataset TEXT NOT NULL,
    execution_venue TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (market_data_provider, market_data_dataset, symbol),
    CHECK (execution_venue IS NULL)
);

INSERT INTO research_instruments (
    id, symbol, asset_class, instrument_kind, currency,
    market_data_provider, market_data_dataset, execution_venue
) VALUES (
    '00000000-0000-7000-8000-000000000601', 'TSLA', 'equity', 'common_stock', 'USD',
    'alpaca', 'iex_1min_bars', NULL
);

CREATE TABLE research_download_jobs (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL CHECK (provider = 'alpaca'),
    dataset TEXT NOT NULL CHECK (dataset = 'iex_1min_bars'),
    symbol TEXT NOT NULL,
    start_at TIMESTAMPTZ NOT NULL,
    end_at TIMESTAMPTZ NOT NULL,
    requested_sessions INTEGER NOT NULL CHECK (requested_sessions > 0),
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'downloading', 'verifying', 'completed', 'failed', 'cancelled')
    ),
    downloaded_records BIGINT NOT NULL DEFAULT 0 CHECK (downloaded_records >= 0),
    verified_sessions INTEGER NOT NULL DEFAULT 0 CHECK (verified_sessions >= 0),
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (end_at > start_at)
);

CREATE UNIQUE INDEX research_download_jobs_active_request_unique
    ON research_download_jobs (provider, dataset, symbol, start_at, end_at)
    WHERE status IN ('queued', 'downloading', 'verifying', 'completed');

CREATE TABLE research_daily_manifests (
    id UUID PRIMARY KEY,
    download_job_id UUID NOT NULL REFERENCES research_download_jobs(id),
    provider TEXT NOT NULL,
    dataset TEXT NOT NULL,
    symbol TEXT NOT NULL,
    session_date DATE NOT NULL,
    schema_name TEXT NOT NULL,
    record_count INTEGER NOT NULL CHECK (record_count > 0),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    storage_path TEXT NOT NULL,
    first_bar_at TIMESTAMPTZ NOT NULL,
    last_bar_at TIMESTAMPTZ NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, dataset, symbol, session_date),
    CHECK (last_bar_at >= first_bar_at)
);

CREATE TABLE validation_cases (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    instrument_id UUID NOT NULL REFERENCES research_instruments(id),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    side TEXT NOT NULL CHECK (side = 'sell'),
    target_notional_usd NUMERIC(38, 18) NOT NULL CHECK (target_notional_usd > 0),
    timezone TEXT NOT NULL CHECK (timezone = 'America/New_York'),
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    max_participation_rate NUMERIC(38, 18) NOT NULL CHECK (
        max_participation_rate > 0 AND max_participation_rate <= 1
    ),
    warmup_sessions INTEGER NOT NULL CHECK (warmup_sessions = 20),
    evaluation_sessions INTEGER NOT NULL CHECK (evaluation_sessions > 0),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (start_time = TIME '10:00:00' AND end_time = TIME '11:30:00')
);

INSERT INTO validation_cases (
    id, name, instrument_id, side, target_notional_usd, timezone,
    start_time, end_time, max_participation_rate, warmup_sessions, evaluation_sessions
) VALUES (
    '00000000-0000-7000-8000-000000000602',
    'TSLA $10M sell / 10% participation',
    '00000000-0000-7000-8000-000000000601',
    'sell', 10000000, 'America/New_York', TIME '10:00:00', TIME '11:30:00', 0.10, 20, 100
);

INSERT INTO strategy_templates (id, name, description, scope)
VALUES
    ('00000000-0000-7000-8000-000000000611', 'Immediate benchmark', 'Sell as quickly as the case participation cap permits.', 'research'),
    ('00000000-0000-7000-8000-000000000612', 'TWAP 5 minute', 'Evenly schedules the parent order across the validation window.', 'research'),
    ('00000000-0000-7000-8000-000000000613', 'POV 10 percent', 'Follows IEX proxy volume at a fixed ten percent participation rate.', 'research'),
    ('00000000-0000-7000-8000-000000000614', 'VWAP 20 session curve', 'Uses only the preceding twenty sessions to build a five-minute volume curve.', 'research');

INSERT INTO strategy_template_versions (
    id, template_id, version, strategy_kind, quantity_unit,
    duration_seconds, slice_interval_ms, max_slippage_bps,
    participation_rate, max_slice_amount, change_note, execution_backend,
    algorithm_config
) VALUES
    ('00000000-0000-7000-8000-000000000621', '00000000-0000-7000-8000-000000000611', 1, 'immediate', 'base_quantity', 5400, 300000, 0, NULL, NULL, 'Initial immutable research benchmark.', 'managed_ioc', '{"algorithm":"immediate"}'),
    ('00000000-0000-7000-8000-000000000622', '00000000-0000-7000-8000-000000000612', 1, 'twap', 'base_quantity', 5400, 300000, 0, NULL, NULL, 'Initial immutable five-minute TWAP.', 'managed_ioc', '{"algorithm":"twap","slice_interval_seconds":300}'),
    ('00000000-0000-7000-8000-000000000623', '00000000-0000-7000-8000-000000000613', 1, 'pov', 'base_quantity', 5400, 300000, 0, 0.10, NULL, 'Initial immutable ten percent POV.', 'managed_ioc', '{"algorithm":"pov","participation_rate":"0.10"}'),
    ('00000000-0000-7000-8000-000000000624', '00000000-0000-7000-8000-000000000614', 1, 'vwap', 'base_quantity', 5400, 300000, 0, NULL, NULL, 'Initial immutable walk-forward VWAP.', 'managed_ioc', '{"algorithm":"vwap","lookback_sessions":20,"bucket_interval_seconds":300}');

CREATE TABLE validation_runs (
    id UUID PRIMARY KEY,
    case_id UUID NOT NULL REFERENCES validation_cases(id),
    case_snapshot JSONB NOT NULL,
    strategy_version_ids UUID[] NOT NULL CHECK (cardinality(strategy_version_ids) > 0),
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'preparing', 'running', 'succeeded', 'failed', 'cancelled')
    ),
    data_quality TEXT NOT NULL CHECK (data_quality = 'iex_proxy'),
    report JSONB,
    report_sha256 TEXT,
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'succeeded') = (report IS NOT NULL AND report_sha256 IS NOT NULL))
);

CREATE TABLE validation_session_results (
    run_id UUID NOT NULL REFERENCES validation_runs(id),
    session_date DATE NOT NULL,
    candidate TEXT NOT NULL,
    result JSONB NOT NULL,
    PRIMARY KEY (run_id, session_date, candidate)
);

CREATE TABLE validation_decisions (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES validation_runs(id),
    decision TEXT NOT NULL CHECK (decision IN ('validated', 'rejected')),
    note TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX research_daily_manifests_lookup_idx
    ON research_daily_manifests (provider, dataset, symbol, session_date);
CREATE INDEX validation_runs_case_created_idx
    ON validation_runs (case_id, created_at DESC);
CREATE INDEX validation_decisions_run_sequence_idx
    ON validation_decisions (run_id, sequence DESC);
