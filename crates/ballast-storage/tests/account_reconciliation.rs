use ballast_storage::{LocalOpenOrderSummary, NewAccountSnapshot};
use chrono::Utc;
use serde_json::json;

async fn create_account(
    pool: &ballast_storage::DatabasePool,
    suffix: i64,
    enabled: bool,
) -> String {
    let account_id = format!("reconciliation-account-{suffix}");
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, exchange, label, environment, secret_name, enabled,
            withdrawals_disabled, ip_restricted, created_by
        ) VALUES ($1, 'okx', $2, 'demo', $3, $4, TRUE, TRUE, 'integration-test')
        "#,
    )
    .bind(&account_id)
    .bind(format!("Reconciliation integration {suffix}"))
    .bind(format!("reconciliation-secret-{suffix}"))
    .bind(enabled)
    .execute(pool)
    .await
    .unwrap();
    account_id
}

fn snapshot(account_id: &str, exchange: &str) -> NewAccountSnapshot {
    NewAccountSnapshot {
        account_id: account_id.to_owned(),
        exchange: exchange.to_owned(),
        balances: json!([{ "asset": "USDT", "free": "1000.00000000", "total": "1000.00000000" }]),
        positions: json!([]),
        open_orders: json!([
            { "client_order_id": "external-only", "state": "open", "quantity": "1.25000000" },
            { "client_order_id": "state-mismatch", "state": "partially_filled", "filled_quantity": "0.50000000" },
            { "client_order_id": "matched", "state": "open", "quantity": "2.00000000" }
        ]),
        observed_at: Utc::now(),
    }
}

fn local_orders() -> Vec<LocalOpenOrderSummary> {
    vec![
        LocalOpenOrderSummary {
            client_order_id: "local-only".to_owned(),
            state: "submission_unknown".to_owned(),
        },
        LocalOpenOrderSummary {
            client_order_id: "state-mismatch".to_owned(),
            state: "open".to_owned(),
        },
        LocalOpenOrderSummary {
            client_order_id: "matched".to_owned(),
            state: "open".to_owned(),
        },
    ]
}

#[tokio::test]
async fn account_reconciliation_is_atomic_historical_and_queryable() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let account_id = create_account(&pool, suffix, true).await;

    let first = ballast_storage::record_account_reconciliation(
        &pool,
        snapshot(&account_id, "okx"),
        &local_orders(),
    )
    .await
    .unwrap();
    assert_eq!(first.run.status, "differences");
    assert_eq!(first.run.difference_count, 3);
    assert_eq!(first.differences.len(), 3);
    let difference_order: Vec<_> = first
        .differences
        .iter()
        .map(|difference| difference.external_reference.as_deref().unwrap())
        .collect();
    assert_eq!(
        difference_order,
        vec!["external-only", "local-only", "state-mismatch"]
    );
    let difference_types: Vec<_> = first
        .differences
        .iter()
        .map(|difference| difference.difference_type.as_str())
        .collect();
    assert!(difference_types.contains(&"external_order_missing_locally"));
    assert!(difference_types.contains(&"local_open_order_missing_externally"));
    assert!(difference_types.contains(&"order_state_mismatch"));
    let mismatch = first
        .differences
        .iter()
        .find(|difference| difference.difference_type == "order_state_mismatch")
        .unwrap();
    assert_eq!(
        mismatch.external_reference.as_deref(),
        Some("state-mismatch")
    );
    assert_eq!(mismatch.expected["state"], "open");
    assert_eq!(mismatch.observed["state"], "partially_filled");
    assert_eq!(first.snapshot.balances[0]["total"], "1000.00000000");

    let repeated = ballast_storage::record_account_reconciliation(
        &pool,
        snapshot(&account_id, "okx"),
        &local_orders(),
    )
    .await
    .unwrap();
    assert_ne!(first.run.id, repeated.run.id);
    assert_ne!(first.snapshot.id, repeated.snapshot.id);
    assert_eq!(repeated.run.difference_count, 3);

    let runs = ballast_storage::list_recent_reconciliation_runs(&pool, &account_id, 10)
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, repeated.run.id);
    let differences = ballast_storage::list_reconciliation_differences(&pool, first.run.id)
        .await
        .unwrap();
    assert_eq!(differences.len(), 3);
    let unique_keys: std::collections::BTreeSet<_> = differences
        .iter()
        .map(|difference| {
            (
                difference.difference_type.clone(),
                difference.external_reference.clone(),
            )
        })
        .collect();
    assert_eq!(unique_keys.len(), differences.len());

    let matched = ballast_storage::record_account_reconciliation(
        &pool,
        NewAccountSnapshot {
            open_orders: json!([{ "client_order_id": "matched", "state": "open", "quantity": "2.00000000" }]),
            ..snapshot(&account_id, "okx")
        },
        &[LocalOpenOrderSummary {
            client_order_id: "matched".to_owned(),
            state: "open".to_owned(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(matched.run.status, "matched");
    assert_eq!(matched.run.difference_count, 0);
    assert!(matched.differences.is_empty());

    let failed = ballast_storage::record_failed_account_reconciliation(
        &pool,
        &account_id,
        "account_gateway_rate_limited",
    )
    .await
    .unwrap();
    assert_eq!(failed.status, "failed");
    assert_eq!(failed.difference_count, 0);
    assert_eq!(
        failed.failure_code.as_deref(),
        Some("account_gateway_rate_limited")
    );
    assert!(failed.completed_at.is_some());
    let latest = ballast_storage::list_recent_reconciliation_runs(&pool, &account_id, 1)
        .await
        .unwrap();
    assert_eq!(latest[0].id, failed.id);
}

#[tokio::test]
async fn account_reconciliation_rejects_invalid_scope_and_rolls_back() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let disabled_account_id = create_account(&pool, suffix, false).await;

    let disabled_error = ballast_storage::record_account_reconciliation(
        &pool,
        snapshot(&disabled_account_id, "okx"),
        &local_orders(),
    )
    .await
    .unwrap_err();
    assert!(
        disabled_error
            .to_string()
            .contains("reconciliation_account_disabled")
    );

    sqlx::query("UPDATE accounts SET enabled = TRUE WHERE id = $1")
        .bind(&disabled_account_id)
        .execute(&pool)
        .await
        .unwrap();
    let exchange_error = ballast_storage::record_account_reconciliation(
        &pool,
        snapshot(&disabled_account_id, "binance"),
        &local_orders(),
    )
    .await
    .unwrap_err();
    assert!(
        exchange_error
            .to_string()
            .contains("reconciliation_exchange_mismatch")
    );

    let duplicate_error = ballast_storage::record_account_reconciliation(
        &pool,
        NewAccountSnapshot {
            open_orders: json!([
                { "client_order_id": "duplicate", "state": "open" },
                { "client_order_id": "duplicate", "state": "partially_filled" }
            ]),
            ..snapshot(&disabled_account_id, "okx")
        },
        &[],
    )
    .await
    .unwrap_err();
    assert!(
        duplicate_error
            .to_string()
            .contains("reconciliation_external_client_order_id_duplicate")
    );

    let decimal_error = ballast_storage::record_account_reconciliation(
        &pool,
        NewAccountSnapshot {
            balances: json!([{ "asset": "USDT", "total": 1000 }]),
            ..snapshot(&disabled_account_id, "okx")
        },
        &local_orders(),
    )
    .await
    .unwrap_err();
    assert!(
        decimal_error
            .to_string()
            .contains("reconciliation_decimal_string_required")
    );

    let invalid_decimal_error = ballast_storage::record_account_reconciliation(
        &pool,
        NewAccountSnapshot {
            balances: json!([{ "asset": "USDT", "total": "NaN" }]),
            ..snapshot(&disabled_account_id, "okx")
        },
        &local_orders(),
    )
    .await
    .unwrap_err();
    assert!(
        invalid_decimal_error
            .to_string()
            .contains("reconciliation_decimal_string_invalid")
    );

    let snapshot_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM account_snapshots WHERE account_id = $1")
            .bind(&disabled_account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let run_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM reconciliation_runs WHERE account_id = $1")
            .bind(&disabled_account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(snapshot_count, 0);
    assert_eq!(run_count, 0);

    let disabled_failure = ballast_storage::record_failed_account_reconciliation(
        &pool,
        "missing-account",
        "account_gateway_snapshot_failed",
    )
    .await
    .unwrap_err();
    assert!(
        disabled_failure
            .to_string()
            .contains("reconciliation_account_not_enabled")
    );
}
