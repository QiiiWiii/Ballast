#!/bin/sh
set -eu

until pg_isready -d "$DATABASE_URL" >/dev/null 2>&1; do
    sleep 1
done

psql "$DATABASE_URL" <<'SQL'
INSERT INTO accounts (
    id,
    exchange,
    label,
    environment,
    secret_name,
    enabled,
    withdrawals_disabled,
    ip_restricted,
    created_by
)
VALUES (
    'acceptance-okx-demo',
    'okx',
    'Acceptance OKX Demo',
    'demo',
    'acceptance-okx-demo-secret',
    TRUE,
    TRUE,
    TRUE,
    'acceptance-seed'
)
ON CONFLICT (id) DO UPDATE SET
    enabled = EXCLUDED.enabled,
    withdrawals_disabled = EXCLUDED.withdrawals_disabled,
    ip_restricted = EXCLUDED.ip_restricted,
    updated_at = now();

INSERT INTO account_snapshots (
    id,
    account_id,
    balances,
    positions,
    open_orders,
    observed_at
)
VALUES (
    '00000000-0000-7000-8000-000000000001',
    'acceptance-okx-demo',
    '[{"asset":"USDT","free":"100000","total":"100000"}]'::jsonb,
    '[]'::jsonb,
    '[]'::jsonb,
    now()
)
ON CONFLICT (id) DO UPDATE SET
    observed_at = now();
SQL
