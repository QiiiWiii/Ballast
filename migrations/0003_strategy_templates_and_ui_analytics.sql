CREATE TABLE strategy_templates (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX strategy_templates_name_unique
    ON strategy_templates (lower(name));

CREATE TABLE strategy_template_versions (
    id UUID PRIMARY KEY,
    template_id UUID NOT NULL REFERENCES strategy_templates(id),
    version INTEGER NOT NULL CHECK (version > 0),
    strategy_kind TEXT NOT NULL CHECK (strategy_kind IN ('twap', 'pov')),
    quantity_unit TEXT NOT NULL CHECK (
        quantity_unit IN ('base_quantity', 'quote_notional', 'contracts')
    ),
    duration_seconds BIGINT NOT NULL CHECK (duration_seconds BETWEEN 1 AND 86400),
    slice_interval_ms BIGINT NOT NULL CHECK (slice_interval_ms > 0),
    max_slippage_bps INTEGER NOT NULL CHECK (max_slippage_bps BETWEEN 0 AND 10000),
    participation_rate NUMERIC(38, 18),
    max_slice_amount NUMERIC(38, 18),
    change_note TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (template_id, version),
    CHECK (
        (strategy_kind = 'twap' AND participation_rate IS NULL)
        OR
        (strategy_kind = 'pov' AND participation_rate > 0 AND participation_rate <= 1)
    ),
    CHECK (max_slice_amount IS NULL OR max_slice_amount > 0),
    CHECK (slice_interval_ms <= duration_seconds * 1000)
);

ALTER TABLE execution_tasks ADD COLUMN template_version_id UUID;

DO $$
DECLARE
    task_record RECORD;
    migrated_participation NUMERIC(38, 18);
    migrated_max_slice NUMERIC(38, 18);
BEGIN
    FOR task_record IN SELECT * FROM execution_tasks LOOP
        migrated_participation := CASE
            WHEN task_record.strategy_kind = 'pov'
             AND task_record.strategy_params->>'participation_rate' IS NOT NULL
            THEN (task_record.strategy_params->>'participation_rate')::NUMERIC
            WHEN task_record.strategy_kind = 'pov' THEN 1
            ELSE NULL
        END;
        migrated_max_slice := CASE
            WHEN task_record.strategy_params->>'max_slice_amount' IS NOT NULL
            THEN (task_record.strategy_params->>'max_slice_amount')::NUMERIC
            ELSE NULL
        END;

        INSERT INTO strategy_templates (id, name, description, status, created_at, updated_at)
        VALUES (
            task_record.id,
            'Migrated task ' || task_record.id::TEXT,
            'Archived template generated from a task created before strategy templates were required.',
            'archived',
            task_record.created_at,
            task_record.updated_at
        );

        INSERT INTO strategy_template_versions (
            id, template_id, version, strategy_kind, quantity_unit,
            duration_seconds, slice_interval_ms, max_slippage_bps,
            participation_rate, max_slice_amount, change_note, created_at
        ) VALUES (
            task_record.id,
            task_record.id,
            1,
            task_record.strategy_kind,
            task_record.quantity_unit,
            GREATEST(EXTRACT(EPOCH FROM (task_record.deadline_at - task_record.created_at))::BIGINT, 1),
            task_record.slice_interval_ms,
            task_record.max_slippage_bps,
            migrated_participation,
            migrated_max_slice,
            'Automatically migrated from existing task parameters.',
            task_record.created_at
        );

        UPDATE execution_tasks
        SET template_version_id = task_record.id
        WHERE id = task_record.id;
    END LOOP;
END $$;

ALTER TABLE execution_tasks
    ALTER COLUMN template_version_id SET NOT NULL,
    ADD CONSTRAINT execution_tasks_template_version_fk
        FOREIGN KEY (template_version_id) REFERENCES strategy_template_versions(id);

CREATE INDEX execution_tasks_template_version_idx
    ON execution_tasks (template_version_id);

CREATE TABLE exchange_health_events (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    exchange TEXT NOT NULL,
    status TEXT NOT NULL,
    error_code TEXT,
    request_latency_ms BIGINT,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX exchange_health_events_lookup_idx
    ON exchange_health_events (exchange, observed_at DESC);

CREATE TABLE market_subscription_health (
    exchange TEXT NOT NULL,
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    stream_kind TEXT NOT NULL CHECK (stream_kind IN ('order_book', 'trades')),
    status TEXT NOT NULL CHECK (
        status IN ('connecting', 'connected', 'reconnecting', 'stale', 'closed')
    ),
    reconnect_attempt INTEGER NOT NULL DEFAULT 0 CHECK (reconnect_attempt >= 0),
    error_code TEXT,
    last_event_at TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (instrument_id, stream_kind)
);
