use ballast_core::{
    Exchange, Instrument, InstrumentId, MarketKind, QuantityUnit, Side, StrategyKind,
};
use ballast_execution::OrderState;
use ballast_storage::{
    ChildOrderStateUpdate, NewChildOrder, NewExecutionTask, NewStrategyTemplate,
};
use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

#[tokio::test]
async fn child_order_intent_and_state_convergence_are_atomic() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let test_suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let symbol = format!("ORDER/USDT-TEST-{test_suffix}");
    let instrument = Instrument {
        id: InstrumentId::new(Exchange::Okx, MarketKind::Spot, &symbol).unwrap(),
        exchange_symbol: format!("ORDERUSDTTEST{test_suffix}"),
        base_asset: "ORDER".to_owned(),
        quote_asset: "USDT".to_owned(),
        settle_asset: None,
        contract_kind: None,
        contract_size: None,
        price_tick: Decimal::new(1, 1),
        quantity_step: Decimal::new(1, 3),
        minimum_quantity: Some(Decimal::new(1, 3)),
        minimum_notional: Some(Decimal::new(5, 0)),
        maker_fee_rate: None,
        taker_fee_rate: None,
        active: true,
    };
    let stored_instrument = ballast_storage::upsert_instruments(&pool, &[instrument])
        .await
        .unwrap()
        .remove(0);
    let (template, template_version) = ballast_storage::create_strategy_template(
        &pool,
        NewStrategyTemplate {
            name: format!("Order repository integration {test_suffix}"),
            description: "order repository integration fixture".to_owned(),
            strategy_kind: "twap".to_owned(),
            quantity_unit: "base_quantity".to_owned(),
            duration_seconds: 60,
            slice_interval_ms: 1_000,
            max_slippage_bps: 20,
            participation_rate: None,
            max_slice_amount: None,
            change_note: "initial".to_owned(),
            execution_backend: "managed_ioc".to_owned(),
            venue_exchange: None,
            venue_market_kind: None,
            native_algorithm: None,
            native_params: None,
        },
    )
    .await
    .unwrap();
    let start_at = Utc::now();
    let task = ballast_storage::create_task(
        &pool,
        NewExecutionTask {
            id: Uuid::now_v7(),
            account_id: "paper".to_owned(),
            execution_mode: "paper".to_owned(),
            instrument_id: stored_instrument.id,
            template_version_id: template_version.id,
            idempotency_key: format!("order-repository-task-{test_suffix}"),
            side: Side::Sell,
            strategy_kind: StrategyKind::Twap,
            strategy_params: serde_json::json!({ "max_slice_amount": null }),
            requested_amount: Decimal::ONE,
            quantity_unit: QuantityUnit::BaseQuantity,
            max_slippage_bps: 20,
            slice_interval_ms: 1_000,
            start_at,
            deadline_at: start_at + Duration::minutes(1),
            requested_by: None,
        },
    )
    .await
    .unwrap();
    let account_id = format!("integration-account-{test_suffix}");
    sqlx::query(
        r#"
        INSERT INTO accounts (
            id, exchange, label, environment, secret_name, enabled,
            withdrawals_disabled, ip_restricted, created_by
        ) VALUES ($1, 'okx', $2, 'demo', $3, TRUE, TRUE, TRUE, 'integration-test')
        "#,
    )
    .bind(&account_id)
    .bind(format!("Order integration {test_suffix}"))
    .bind(format!("order-integration-secret-{test_suffix}"))
    .execute(&pool)
    .await
    .unwrap();

    let child_order = NewChildOrder {
        task_id: task.id,
        exchange: "okx".to_owned(),
        account_id: account_id.clone(),
        request_id: Uuid::now_v7(),
        client_order_id: format!("b1{:030x}", test_suffix.unsigned_abs()),
        side: "sell".to_owned(),
        order_type: "limit_ioc".to_owned(),
        order_backend: "managed_ioc".to_owned(),
        quantity: Decimal::ONE,
        limit_price: Some(Decimal::new(100, 0)),
    };
    let created_order = ballast_storage::create_or_get_child_order(&pool, &child_order)
        .await
        .unwrap();
    let duplicate_order = ballast_storage::create_or_get_child_order(&pool, &child_order)
        .await
        .unwrap();
    assert_eq!(created_order.id, duplicate_order.id);
    assert_eq!(created_order.status, OrderState::SubmissionPending);
    let duplicate_with_new_request = NewChildOrder {
        request_id: Uuid::now_v7(),
        ..child_order.clone()
    };
    assert_eq!(
        ballast_storage::create_or_get_child_order(&pool, &duplicate_with_new_request)
            .await
            .unwrap()
            .id,
        created_order.id
    );
    assert_eq!(
        ballast_storage::get_child_order_by_client_order_id(
            &pool,
            &child_order.exchange,
            &child_order.account_id,
            &child_order.client_order_id,
        )
        .await
        .unwrap()
        .unwrap()
        .id,
        created_order.id
    );

    let conflicting_intent = NewChildOrder {
        quantity: Decimal::new(2, 0),
        ..child_order.clone()
    };
    let conflict = ballast_storage::create_or_get_child_order(&pool, &conflicting_intent)
        .await
        .unwrap_err();
    assert!(conflict.to_string().contains("child_order_intent_conflict"));

    let cancelled_ioc = NewChildOrder {
        request_id: Uuid::now_v7(),
        client_order_id: format!("b2{:030x}", test_suffix.unsigned_abs()),
        ..child_order.clone()
    };
    ballast_storage::create_or_get_child_order(&pool, &cancelled_ioc)
        .await
        .unwrap();
    let cancelled_at = Utc::now();
    let cancelled_order = ballast_storage::compare_and_set_child_order_state(
        &pool,
        &cancelled_ioc.exchange,
        &cancelled_ioc.account_id,
        &cancelled_ioc.client_order_id,
        &[OrderState::SubmissionPending],
        &ChildOrderStateUpdate {
            status: OrderState::Cancelled,
            exchange_order_id: Some(format!("cancelled-order-{test_suffix}")),
            filled_quantity: Decimal::ZERO,
            average_price: None,
            limit_price: cancelled_ioc.limit_price,
            raw_status: Some("canceled".to_owned()),
            state_reason: Some("ioc_unfilled".to_owned()),
            submitted_at: Some(cancelled_at),
            last_reconciled_at: Some(cancelled_at),
        },
    )
    .await
    .unwrap();
    assert_eq!(cancelled_order.status, OrderState::Cancelled);

    let unknown_at = Utc::now();
    let unknown_order = ballast_storage::compare_and_set_child_order_state(
        &pool,
        &child_order.exchange,
        &child_order.account_id,
        &child_order.client_order_id,
        &[OrderState::SubmissionPending],
        &ChildOrderStateUpdate {
            status: OrderState::SubmissionUnknown,
            exchange_order_id: None,
            filled_quantity: Decimal::ZERO,
            average_price: None,
            limit_price: child_order.limit_price,
            raw_status: None,
            state_reason: Some("submit_timeout".to_owned()),
            submitted_at: None,
            last_reconciled_at: Some(unknown_at),
        },
    )
    .await
    .unwrap();
    assert_eq!(unknown_order.status, OrderState::SubmissionUnknown);
    let account_open_orders = ballast_storage::list_account_open_orders(&pool, &account_id, "okx")
        .await
        .unwrap();
    assert_eq!(account_open_orders.len(), 1);
    assert_eq!(account_open_orders[0].id, created_order.id);
    sqlx::query("UPDATE child_orders SET exchange = 'binance' WHERE id = $1")
        .bind(created_order.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        ballast_storage::list_account_open_orders(&pool, &account_id, "okx")
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query("UPDATE child_orders SET exchange = 'okx' WHERE id = $1")
        .bind(created_order.id)
        .execute(&pool)
        .await
        .unwrap();
    let claim = ballast_storage::claim_child_orders_for_reconciliation(
        &pool,
        10,
        Utc::now() + Duration::seconds(30),
    )
    .await
    .unwrap()
    .into_iter()
    .find(|claimed| claimed.order.id == created_order.id)
    .unwrap();
    assert_eq!(claim.account_environment, "demo");
    assert_eq!(claim.instrument_id, stored_instrument.id);
    assert!(
        ballast_storage::claim_child_orders_for_reconciliation(
            &pool,
            10,
            Utc::now() + Duration::seconds(30),
        )
        .await
        .unwrap()
        .into_iter()
        .all(|claimed| claimed.order.id != created_order.id)
    );
    assert!(
        ballast_storage::complete_child_order_reconciliation(
            &pool,
            created_order.id,
            claim.claim_token,
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap()
    );
    let retry_claim = ballast_storage::claim_child_orders_for_reconciliation(
        &pool,
        10,
        Utc::now() + Duration::seconds(30),
    )
    .await
    .unwrap()
    .into_iter()
    .find(|claimed| claimed.order.id == created_order.id)
    .unwrap();
    assert!(
        ballast_storage::fail_child_order_reconciliation(
            &pool,
            created_order.id,
            retry_claim.claim_token,
            Utc::now() + Duration::minutes(1),
            "gateway_unavailable",
        )
        .await
        .unwrap()
    );
    sqlx::query(
        "UPDATE child_orders SET reconciliation_due_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(created_order.id)
    .execute(&pool)
    .await
    .unwrap();
    let suspend_claim = ballast_storage::claim_child_orders_for_reconciliation(
        &pool,
        10,
        Utc::now() + Duration::seconds(30),
    )
    .await
    .unwrap()
    .into_iter()
    .find(|claimed| claimed.order.id == created_order.id)
    .unwrap();
    assert_eq!(suspend_claim.failure_count, 1);
    assert!(
        ballast_storage::suspend_child_order_reconciliation(
            &pool,
            created_order.id,
            suspend_claim.claim_token,
            "protocol_invalid",
        )
        .await
        .unwrap()
    );
    assert!(
        ballast_storage::claim_child_orders_for_reconciliation(
            &pool,
            10,
            Utc::now() + Duration::seconds(30),
        )
        .await
        .unwrap()
        .into_iter()
        .all(|claimed| claimed.order.id != created_order.id)
    );
    sqlx::query(
        "UPDATE child_orders SET reconciliation_due_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(created_order.id)
    .execute(&pool)
    .await
    .unwrap();

    let opened_at = unknown_at + Duration::seconds(1);
    let open_update = ChildOrderStateUpdate {
        status: OrderState::Open,
        exchange_order_id: Some(format!("exchange-order-{test_suffix}")),
        filled_quantity: Decimal::ZERO,
        average_price: None,
        limit_price: child_order.limit_price,
        raw_status: Some("live".to_owned()),
        state_reason: None,
        submitted_at: Some(opened_at),
        last_reconciled_at: Some(opened_at),
    };
    let open_order = ballast_storage::compare_and_set_child_order_state(
        &pool,
        &child_order.exchange,
        &child_order.account_id,
        &child_order.client_order_id,
        &[OrderState::SubmissionUnknown],
        &open_update,
    )
    .await
    .unwrap();
    assert_eq!(open_order.status, OrderState::Open);
    assert_eq!(open_order.exchange_order_id, open_update.exchange_order_id);
    assert!(open_order.submitted_at.is_some());

    let repeated_open = ChildOrderStateUpdate {
        average_price: None,
        last_reconciled_at: Some(opened_at + Duration::seconds(1)),
        ..open_update.clone()
    };
    let repeated_open_order = ballast_storage::compare_and_set_child_order_state(
        &pool,
        &child_order.exchange,
        &child_order.account_id,
        &child_order.client_order_id,
        &[OrderState::Open],
        &repeated_open,
    )
    .await
    .unwrap();
    assert!(repeated_open_order.average_price.is_none());

    let filled_at = opened_at + Duration::seconds(2);
    let filled_order = ballast_storage::compare_and_set_child_order_state(
        &pool,
        &child_order.exchange,
        &child_order.account_id,
        &child_order.client_order_id,
        &[OrderState::Open],
        &ChildOrderStateUpdate {
            status: OrderState::Filled,
            exchange_order_id: open_update.exchange_order_id.clone(),
            filled_quantity: Decimal::ONE,
            average_price: Some(Decimal::new(995, 1)),
            limit_price: child_order.limit_price,
            raw_status: Some("filled".to_owned()),
            state_reason: None,
            submitted_at: Some(opened_at),
            last_reconciled_at: Some(filled_at),
        },
    )
    .await
    .unwrap();
    assert_eq!(filled_order.status, OrderState::Filled);
    assert_eq!(filled_order.filled_quantity, Decimal::ONE);
    assert_eq!(filled_order.average_price, Some(Decimal::new(995, 1)));
    sqlx::query(
        r#"
        UPDATE child_orders
        SET reconciliation_due_at = now() - interval '1 second',
            reconciliation_claim_token = $2,
            reconciliation_claimed_until = now() + interval '30 seconds'
        WHERE id = $1
        "#,
    )
    .bind(filled_order.id)
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await
    .unwrap();
    let terminal_claim_token: Uuid =
        sqlx::query_scalar("SELECT reconciliation_claim_token FROM child_orders WHERE id = $1")
            .bind(filled_order.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        ballast_storage::complete_child_order_reconciliation(
            &pool,
            filled_order.id,
            terminal_claim_token,
            Utc::now() - Duration::seconds(1),
        )
        .await
        .unwrap()
    );
    assert!(
        ballast_storage::claim_child_orders_for_reconciliation(
            &pool,
            10,
            Utc::now() + Duration::seconds(30),
        )
        .await
        .unwrap()
        .into_iter()
        .all(|claimed| claimed.order.id != filled_order.id)
    );

    let order_state_events: Vec<_> = ballast_storage::list_events_after(&pool, 0, 10_000)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| {
            event.task_id == Some(task.id)
                && event.event_type == "child_order_state_changed"
                && event.payload["child_order_id"] == created_order.id.to_string()
        })
        .collect();
    assert_eq!(order_state_events.len(), 3);
    assert_eq!(
        order_state_events[0].payload["previous"]["status"],
        "submission_pending"
    );
    assert_eq!(order_state_events[2].payload["current"]["status"], "filled");
    assert_eq!(
        order_state_events[2].payload["current"]["filled_quantity"],
        "1"
    );

    let illegal_reopen = ballast_storage::compare_and_set_child_order_state(
        &pool,
        &child_order.exchange,
        &child_order.account_id,
        &child_order.client_order_id,
        &[OrderState::Filled],
        &ChildOrderStateUpdate {
            status: OrderState::Open,
            exchange_order_id: open_update.exchange_order_id,
            filled_quantity: Decimal::ONE,
            average_price: Some(Decimal::new(995, 1)),
            limit_price: child_order.limit_price,
            raw_status: Some("live".to_owned()),
            state_reason: None,
            submitted_at: Some(opened_at),
            last_reconciled_at: Some(filled_at + Duration::seconds(1)),
        },
    )
    .await
    .unwrap_err();
    assert!(
        illegal_reopen
            .to_string()
            .contains("child_order_state_transition_invalid")
    );

    sqlx::query("DELETE FROM execution_events WHERE task_id = $1")
        .bind(task.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM child_orders WHERE task_id = $1")
        .bind(task.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM execution_tasks WHERE id = $1")
        .bind(task.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM strategy_template_versions WHERE template_id = $1")
        .bind(template.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM strategy_templates WHERE id = $1")
        .bind(template.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM instruments WHERE id = $1")
        .bind(stored_instrument.id)
        .execute(&pool)
        .await
        .unwrap();
}
