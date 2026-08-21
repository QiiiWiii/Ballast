ALTER TABLE fills
    ADD COLUMN fee_status TEXT NOT NULL DEFAULT 'unavailable'
        CHECK (fee_status IN ('calculated', 'unavailable'));
