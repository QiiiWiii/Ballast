UPDATE historical_backfill_jobs
SET cursor_at = end_at
WHERE status = 'completed'
  AND cursor_at <> end_at;

ALTER TABLE historical_backfill_jobs
    ADD CONSTRAINT historical_backfill_completed_cursor_at_end
        CHECK (status <> 'completed' OR cursor_at = end_at);

ALTER TABLE replay_runs
    ADD COLUMN instrument_snapshot JSONB;

UPDATE replay_runs
SET instrument_snapshot = jsonb_build_object(
        'version', 0,
        'status', 'unverifiable_pre_0009'
    )
WHERE instrument_snapshot IS NULL;

ALTER TABLE replay_runs
    ALTER COLUMN instrument_snapshot SET NOT NULL,
    ADD CONSTRAINT replay_runs_instrument_snapshot_object
        CHECK (jsonb_typeof(instrument_snapshot) = 'object');
