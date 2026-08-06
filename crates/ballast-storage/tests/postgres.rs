use ballast_core::{
    Exchange, Instrument, InstrumentId, MarketKind, QuantityUnit, Side, StrategyKind,
};
use ballast_storage::{
    NewExecutionTask, NewStrategyTemplate, NewStrategyTemplateVersion, SliceRecord,
};
use chrono::{Duration, Utc};
use rust_decimal::Decimal;

#[tokio::test]
async fn task_persistence_is_idempotent_and_event_ordered() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let instrument = Instrument {
        id: InstrumentId::new(Exchange::Binance, MarketKind::Spot, "BTC/USDT").unwrap(),
        exchange_symbol: "BTCUSDT".to_owned(),
        base_asset: "BTC".to_owned(),
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
    let stored = ballast_storage::upsert_instruments(&pool, &[instrument])
        .await
        .unwrap();
    let start_at = Utc::now();
    let (template, template_version) = ballast_storage::create_strategy_template(
        &pool,
        NewStrategyTemplate {
            name: format!(
                "Integration TWAP {}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ),
            description: "integration template".to_owned(),
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
    let second_version = ballast_storage::create_strategy_template_version(
        &pool,
        template.id,
        NewStrategyTemplateVersion {
            strategy_kind: "twap".to_owned(),
            quantity_unit: "base_quantity".to_owned(),
            duration_seconds: 120,
            slice_interval_ms: 2_000,
            max_slippage_bps: 25,
            participation_rate: None,
            max_slice_amount: Some(Decimal::new(5, 1)),
            change_note: "slower slices".to_owned(),
            execution_backend: "managed_ioc".to_owned(),
            venue_exchange: None,
            venue_market_kind: None,
            native_algorithm: None,
            native_params: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(second_version.version, 2);
    assert_eq!(
        ballast_storage::list_strategy_template_versions(&pool, template.id)
            .await
            .unwrap()
            .len(),
        2
    );
    let new_task = NewExecutionTask {
        instrument_id: stored[0].id,
        template_version_id: template_version.id,
        idempotency_key: format!(
            "integration-task-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ),
        side: Side::Sell,
        strategy_kind: StrategyKind::Twap,
        strategy_params: serde_json::json!({ "max_slice_amount": null }),
        requested_amount: Decimal::ONE,
        quantity_unit: QuantityUnit::BaseQuantity,
        max_slippage_bps: 20,
        slice_interval_ms: 1_000,
        start_at,
        deadline_at: start_at + Duration::minutes(1),
    };
    let first = ballast_storage::create_task(&pool, new_task.clone())
        .await
        .unwrap();
    let duplicate = ballast_storage::create_task(&pool, new_task).await.unwrap();
    assert_eq!(first.id, duplicate.id);
    assert_eq!(first.template_version_id, template_version.id);
    assert_eq!(first.residual_amount, Decimal::ONE);
    assert!(first.started_at.is_none());

    ballast_storage::record_slice(
        &pool,
        SliceRecord {
            task_id: first.id,
            sequence: 1,
            requested_amount: Decimal::new(5, 1),
            native_quantity: Decimal::new(5, 1),
            filled_native_quantity: Decimal::new(4, 1),
            filled_base_quantity: Decimal::new(4, 1),
            filled_quote_quantity: Decimal::new(40, 0),
            average_price: Some(Decimal::new(100, 0)),
            worst_price: Some(Decimal::new(100, 0)),
            slippage_bps: Some(Decimal::ZERO),
            fee_amount: None,
            fee_asset: None,
            fee_status: "unavailable",
            status: "partial",
            market_snapshot: serde_json::json!({ "sequence": "1" }),
            decision_input: serde_json::json!({ "remaining_amount": "1" }),
            executed_amount_delta: Decimal::new(4, 1),
            residual_amount: Decimal::new(6, 1),
            next_tick_at: start_at + Duration::seconds(1),
            task_status: "running",
        },
    )
    .await
    .unwrap();
    let slices = ballast_storage::list_slices(&pool, first.id).await.unwrap();
    assert_eq!(slices.len(), 1);
    let events = ballast_storage::list_events_after(&pool, 0, 20)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(events[0].sequence < events[1].sequence);
    assert!(ballast_storage::cancel_task(&pool, first.id).await.unwrap());
    assert!(!ballast_storage::cancel_task(&pool, first.id).await.unwrap());
    assert!(
        ballast_storage::archive_strategy_template(&pool, template.id)
            .await
            .unwrap()
    );
    assert!(
        !ballast_storage::archive_strategy_template(&pool, template.id)
            .await
            .unwrap()
    );
}
