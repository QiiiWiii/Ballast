ALTER TABLE execution_slices
    ADD COLUMN child_order_id UUID REFERENCES child_orders(id);

CREATE UNIQUE INDEX execution_slices_child_order_unique
    ON execution_slices (child_order_id)
    WHERE child_order_id IS NOT NULL;
