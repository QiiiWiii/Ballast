ALTER TABLE historical_trades
    ADD COLUMN ingestion_id BIGINT GENERATED ALWAYS AS IDENTITY;

CREATE UNIQUE INDEX historical_trades_ingestion_id_idx
    ON historical_trades (ingestion_id);

ALTER TABLE replay_runs
    ADD COLUMN data_snapshot JSONB,
    ADD COLUMN coverage_snapshot JSONB;

UPDATE replay_runs
SET data_snapshot = jsonb_build_object(
        'version', 0,
        'status', 'unverifiable_pre_0008'
    ),
    coverage_snapshot = jsonb_build_object(
        'version', 0,
        'status', 'unverifiable_pre_0008'
    )
WHERE data_snapshot IS NULL OR coverage_snapshot IS NULL;

ALTER TABLE replay_runs
    ALTER COLUMN data_snapshot SET NOT NULL,
    ALTER COLUMN coverage_snapshot SET NOT NULL,
    ADD CONSTRAINT replay_runs_data_snapshot_object
        CHECK (jsonb_typeof(data_snapshot) = 'object'),
    ADD CONSTRAINT replay_runs_coverage_snapshot_object
        CHECK (jsonb_typeof(coverage_snapshot) = 'object');
