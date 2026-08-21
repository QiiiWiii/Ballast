ALTER TABLE outbox_commands
    DROP CONSTRAINT outbox_commands_task_id_fkey;

ALTER TABLE outbox_commands
    ADD CONSTRAINT outbox_commands_task_id_fkey
        FOREIGN KEY (task_id) REFERENCES execution_tasks(id) ON DELETE SET NULL;
