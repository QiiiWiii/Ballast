CREATE TABLE historical_backfill_jobs (
    id UUID PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    data_type TEXT NOT NULL CHECK (data_type IN ('ohlcv', 'trades')),
    timeframe TEXT,
    start_at TIMESTAMPTZ NOT NULL,
    end_at TIMESTAMPTZ NOT NULL,
    cursor_at TIMESTAMPTZ NOT NULL,
    covered_from TIMESTAMPTZ,
    covered_to TIMESTAMPTZ,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    rows_written BIGINT NOT NULL DEFAULT 0 CHECK (rows_written >= 0),
    last_error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (start_at < end_at),
    CHECK (
        (data_type = 'ohlcv' AND timeframe IS NOT NULL)
        OR (data_type = 'trades' AND timeframe IS NULL)
    )
);

CREATE INDEX historical_backfill_jobs_status_idx
    ON historical_backfill_jobs (status, updated_at);

CREATE TABLE historical_candles (
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    timeframe TEXT NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open NUMERIC(38, 18) NOT NULL CHECK (open > 0),
    high NUMERIC(38, 18) NOT NULL CHECK (high > 0),
    low NUMERIC(38, 18) NOT NULL CHECK (low > 0),
    close NUMERIC(38, 18) NOT NULL CHECK (close > 0),
    volume NUMERIC(38, 18) NOT NULL CHECK (volume >= 0),
    source TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (instrument_id, timeframe, open_time),
    CHECK (high >= low)
);

CREATE INDEX historical_candles_range_idx
    ON historical_candles (instrument_id, timeframe, open_time);

CREATE TABLE historical_trades (
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    exchange_trade_id TEXT NOT NULL,
    trade_time TIMESTAMPTZ NOT NULL,
    price NUMERIC(38, 18) NOT NULL CHECK (price > 0),
    quantity NUMERIC(38, 18) NOT NULL CHECK (quantity > 0),
    taker_side TEXT NOT NULL CHECK (taker_side IN ('buy', 'sell', 'unknown')),
    source TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (instrument_id, exchange_trade_id)
);

CREATE INDEX historical_trades_range_idx
    ON historical_trades (instrument_id, trade_time, exchange_trade_id);
