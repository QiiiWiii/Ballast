CREATE TABLE execution_tasks (
    id UUID PRIMARY KEY,
    account_id TEXT NOT NULL,
    exchange TEXT NOT NULL,
    market_kind TEXT NOT NULL,
    symbol TEXT NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    strategy_kind TEXT NOT NULL CHECK (strategy_kind IN ('twap', 'pov')),
    strategy_params JSONB NOT NULL DEFAULT '{}'::jsonb,
    target_quantity NUMERIC(38, 18) NOT NULL CHECK (target_quantity > 0),
    executed_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (executed_quantity >= 0),
    status TEXT NOT NULL,
    version BIGINT NOT NULL DEFAULT 0,
    started_at TIMESTAMPTZ,
    deadline_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE child_orders (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES execution_tasks(id),
    -- Kept on the order itself so idempotency scope never depends on a mutable join.
    exchange TEXT NOT NULL,
    account_id TEXT NOT NULL,
    request_id UUID NOT NULL UNIQUE,
    client_order_id TEXT NOT NULL,
    exchange_order_id TEXT,
    side TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    order_type TEXT NOT NULL CHECK (order_type IN ('market', 'limit')),
    quantity NUMERIC(38, 18) NOT NULL CHECK (quantity > 0),
    filled_quantity NUMERIC(38, 18) NOT NULL DEFAULT 0 CHECK (filled_quantity >= 0),
    price NUMERIC(38, 18),
    status TEXT NOT NULL,
    raw_status TEXT,
    submitted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (exchange, account_id, client_order_id)
);

CREATE UNIQUE INDEX child_orders_exchange_order_id_unique
    ON child_orders (exchange, account_id, exchange_order_id)
    WHERE exchange_order_id IS NOT NULL;

CREATE INDEX child_orders_task_id_idx ON child_orders (task_id);

CREATE TABLE fills (
    id UUID PRIMARY KEY,
    child_order_id UUID NOT NULL REFERENCES child_orders(id),
    exchange TEXT NOT NULL,
    account_id TEXT NOT NULL,
    exchange_trade_id TEXT NOT NULL,
    price NUMERIC(38, 18) NOT NULL CHECK (price > 0),
    quantity NUMERIC(38, 18) NOT NULL CHECK (quantity > 0),
    fee NUMERIC(38, 18) NOT NULL DEFAULT 0,
    fee_asset TEXT,
    exchange_time TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (exchange, account_id, exchange_trade_id)
);

CREATE INDEX fills_child_order_id_idx ON fills (child_order_id);

CREATE TABLE execution_events (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    task_id UUID REFERENCES execution_tasks(id),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX execution_events_task_sequence_idx
    ON execution_events (task_id, sequence);

CREATE TABLE outbox_commands (
    id UUID PRIMARY KEY,
    task_id UUID REFERENCES execution_tasks(id),
    command_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX outbox_commands_pending_idx
    ON outbox_commands (available_at, created_at)
    WHERE status = 'pending';
