ALTER TABLE child_orders
    ADD COLUMN average_price NUMERIC(38, 18),
    ADD CONSTRAINT child_orders_average_price_positive_check
        CHECK (average_price IS NULL OR average_price > 0);
