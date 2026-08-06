ALTER TABLE exchange_health
    ADD COLUMN request_latency_ms BIGINT
        CHECK (request_latency_ms >= 0);
