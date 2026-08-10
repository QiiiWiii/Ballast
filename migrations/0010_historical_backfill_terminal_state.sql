CREATE FUNCTION reject_historical_backfill_completed_regression()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'completed' AND NEW.status <> 'completed' THEN
        RAISE EXCEPTION 'completed historical backfill jobs cannot transition to %', NEW.status
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER historical_backfill_completed_is_terminal
BEFORE UPDATE ON historical_backfill_jobs
FOR EACH ROW
EXECUTE FUNCTION reject_historical_backfill_completed_regression();
