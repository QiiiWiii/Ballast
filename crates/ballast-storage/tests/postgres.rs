use ballast_core::{
    Exchange, Instrument, InstrumentId, MarketKind, QuantityUnit, Side, StrategyKind,
};
use ballast_storage::{
    HistoricalCandle, NewExecutionTask, NewHistoricalBackfill,
    NewStrategyTemplate, NewStrategyTemplateVersion, SliceRecord,
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
    let has_current_health_latency: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.columns
            WHERE table_name = 'exchange_health'
              AND column_name = 'request_latency_ms'
              AND is_nullable = 'YES'
        )
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(has_current_health_latency);
    let test_suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let symbol = format!("BTC/USDT-TEST-{test_suffix}");
    let instrument = Instrument {
        id: InstrumentId::new(Exchange::Binance, MarketKind::Spot, &symbol).unwrap(),
        exchange_symbol: format!("BTCUSDTTEST{test_suffix}"),
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
    let (instrument_page, total) = ballast_storage::list_instruments_page(
        &pool,
        Some(Exchange::Binance),
        Some(MarketKind::Spot),
        true,
        Some(&symbol),
        None,
        10,
        0,
    )
    .await
    .unwrap();
    assert_eq!(total, 1);
    assert_eq!(instrument_page[0].id, stored[0].id);
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
    let events = ballast_storage::list_events_after(&pool, 0, 10_000)
        .await
        .unwrap();
    let events: Vec<_> = events
        .into_iter()
        .filter(|event| event.task_id == Some(first.id))
        .collect();
    assert_eq!(events.len(), 3);
    assert!(events[0].sequence < events[1].sequence);
    assert!(events[1].sequence < events[2].sequence);
    assert_eq!(events[0].event_type, "task_created");
    assert_eq!(events[1].event_type, "slice_recorded");
    assert_eq!(events[2].event_type, "task_state_changed");
    assert_eq!(events[2].payload["status"], "running");

    let deferred_tick = start_at + Duration::milliseconds(1500);
    ballast_storage::defer_task_tick(
        &pool,
        first.id,
        deferred_tick,
        serde_json::json!({ "reason": "below_minimum_slice" }),
    )
    .await
    .unwrap();
    let latest_events = ballast_storage::list_task_events(&pool, first.id, 2)
        .await
        .unwrap();
    assert_eq!(latest_events.len(), 2);
    assert_eq!(latest_events[0].event_type, "task_state_changed");
    assert_eq!(latest_events[1].event_type, "slice_deferred");
    assert!(latest_events[0].sequence < latest_events[1].sequence);

    let first_pause_tick = start_at + Duration::seconds(2);
    ballast_storage::mark_task_state(
        &pool,
        first.id,
        "paused",
        Some("order_book_unavailable"),
        first_pause_tick,
    )
    .await
    .unwrap();
    let paused = ballast_storage::get_task(&pool, first.id)
        .await
        .unwrap()
        .unwrap();
    let repeated_pause_tick = start_at + Duration::seconds(3);
    ballast_storage::mark_task_state(
        &pool,
        first.id,
        "paused",
        Some("order_book_unavailable"),
        repeated_pause_tick,
    )
    .await
    .unwrap();
    let repeated = ballast_storage::get_task(&pool, first.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repeated.version, paused.version);
    assert_eq!(
        repeated.next_tick_at.timestamp_micros(),
        repeated_pause_tick.timestamp_micros()
    );
    let pause_events = ballast_storage::list_events_after(&pool, 0, 10_000)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| {
            event.task_id == Some(first.id)
                && event.event_type == "task_state_changed"
                && event.payload["status"] == "paused"
        })
        .count();
    assert_eq!(pause_events, 1);
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

    sqlx::query("DELETE FROM execution_events WHERE task_id = $1")
        .bind(first.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM execution_slices WHERE task_id = $1")
        .bind(first.id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM execution_tasks WHERE id = $1")
        .bind(first.id)
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
        .bind(stored[0].id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn historical_backfill_is_resumable_and_deduplicated() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let symbol = format!("HISTORY/USDT-{suffix}");
    let instrument = Instrument {
        id: InstrumentId::new(Exchange::Okx, MarketKind::Spot, &symbol).unwrap(),
        exchange_symbol: format!("HISTORYUSDT{suffix}"),
        base_asset: "HISTORY".to_owned(),
        quote_asset: "USDT".to_owned(),
        settle_asset: None,
        contract_kind: None,
        contract_size: None,
        price_tick: Decimal::new(1, 2),
        quantity_step: Decimal::new(1, 3),
        minimum_quantity: None,
        minimum_notional: None,
        maker_fee_rate: None,
        taker_fee_rate: None,
        active: true,
    };
    let stored = ballast_storage::upsert_instruments(&pool, &[instrument])
        .await
        .unwrap();
    let start = Utc::now() - Duration::hours(1);
    let end = start + Duration::minutes(5);
    let job = ballast_storage::create_or_get_historical_backfill(
        &pool,
        &NewHistoricalBackfill {
            idempotency_key: format!("history-test-{suffix}"),
            instrument_id: stored[0].id,
            data_type: "ohlcv".to_owned(),
            timeframe: Some("1m".to_owned()),
            start_at: start,
            end_at: end,
        },
    )
    .await
    .unwrap();
    ballast_storage::mark_historical_backfill_running(&pool, job.id)
        .await
        .unwrap();
    let candle = HistoricalCandle {
        open_time: start,
        open: Decimal::new(100, 0),
        high: Decimal::new(110, 0),
        low: Decimal::new(90, 0),
        close: Decimal::new(105, 0),
        volume: Decimal::new(10, 0),
        source: "okx".to_owned(),
        observed_at: Utc::now(),
    };
    ballast_storage::persist_candle_batch(
        &pool,
        job.id,
        stored[0].id,
        "1m",
        std::slice::from_ref(&candle),
        start + Duration::minutes(1),
        false,
    )
    .await
    .unwrap();
    ballast_storage::persist_candle_batch(
        &pool,
        job.id,
        stored[0].id,
        "1m",
        std::slice::from_ref(&candle),
        end,
        true,
    )
    .await
    .unwrap();
    let (candles, total) =
        ballast_storage::list_historical_candles(&pool, stored[0].id, "1m", start, end, 100, 0)
            .await
            .unwrap();
    assert_eq!(total, 1);
    assert_eq!(candles.len(), 1);
    let completed = ballast_storage::get_historical_backfill(&pool, job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.rows_written, 1);
    assert_eq!(
        completed.cursor_at.timestamp_micros(),
        end.timestamp_micros()
    );
}

