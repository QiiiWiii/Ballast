use ballast_core::{
    Exchange, Instrument, InstrumentId, MarketKind, QuantityUnit, Side, StrategyKind,
};
use ballast_storage::{
    CreateReplayError, HistoricalCandle, HistoricalTrade, NewExecutionTask, NewHistoricalBackfill,
    NewReplayMetrics, NewReplayRun, NewReplaySlice, NewStrategyTemplate,
    NewStrategyTemplateVersion, SliceRecord,
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
async fn replay_results_are_atomic_and_idempotent() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let instrument = Instrument {
        id: InstrumentId::new(
            Exchange::Okx,
            MarketKind::Spot,
            format!("BTC/USDT-REPLAY-{suffix}"),
        )
        .unwrap(),
        exchange_symbol: format!("BTCUSDTREPLAY{suffix}"),
        base_asset: "BTC".to_owned(),
        quote_asset: "USDT".to_owned(),
        settle_asset: None,
        contract_kind: None,
        contract_size: None,
        price_tick: Decimal::new(1, 1),
        quantity_step: Decimal::new(1, 3),
        minimum_quantity: None,
        minimum_notional: None,
        maker_fee_rate: None,
        taker_fee_rate: Some(Decimal::new(1, 3)),
        active: true,
    };
    let instrument = ballast_storage::upsert_instruments(&pool, &[instrument])
        .await
        .unwrap()
        .remove(0);
    let (_, template) = ballast_storage::create_strategy_template(
        &pool,
        NewStrategyTemplate {
            name: format!("Replay TWAP {suffix}"),
            description: "replay integration".to_owned(),
            strategy_kind: "twap".to_owned(),
            quantity_unit: "base_quantity".to_owned(),
            duration_seconds: 60,
            slice_interval_ms: 30_000,
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
    let start = Utc::now();
    let key = format!("replay-{suffix}");
    let run = NewReplayRun {
        idempotency_key: key.clone(),
        instrument_id: instrument.id,
        template_version_id: template.id,
        strategy_kind: "twap".to_owned(),
        side: "buy".to_owned(),
        requested_amount: Decimal::ONE,
        quantity_unit: "base_quantity".to_owned(),
        start_at: start,
        end_at: start + Duration::minutes(1),
        execution_model: "trade_vwap_proxy".to_owned(),
        model_version: "trade_vwap_proxy/v1".to_owned(),
        fee_rate: Decimal::new(1, 3),
        extra_slippage_bps: Decimal::new(5, 0),
        gap_threshold_seconds: 60,
        strategy_snapshot: serde_json::json!({ "version": 1 }),
        instrument_snapshot: serde_json::json!({ "version": 1, "instrument": instrument.instrument }),
        data_snapshot: serde_json::json!({ "version": 1, "maximum_ingestion_id": 0 }),
        coverage_snapshot: serde_json::json!({ "version": 1, "jobs": [] }),
        request_fingerprint: "stable-fingerprint".to_owned(),
        status: "completed".to_owned(),
        failure_code: None,
        data_first_at: Some(start),
        data_last_at: Some(start + Duration::seconds(59)),
        trade_count: 2,
        gap_count: 0,
        data_gaps: serde_json::json!([]),
        confidence: "high".to_owned(),
        limitations: serde_json::json!(["proxy"]),
    };
    let slices = [NewReplaySlice {
        sequence: 1,
        window_start: start,
        window_end: start + Duration::seconds(30),
        status: "filled".to_owned(),
        trade_count: 1,
        market_volume: Decimal::new(10, 0),
        requested_amount: Decimal::ONE,
        filled_amount: Decimal::ONE,
        filled_native_quantity: Decimal::ONE,
        market_vwap: Some(Decimal::new(100, 0)),
        simulated_price: Some(Decimal::new(10005, 2)),
        fee_amount: Decimal::new(10005, 5),
        decision_input: serde_json::json!({ "tick": 0 }),
    }];
    let metrics = NewReplayMetrics {
        arrival_price: Some(Decimal::new(100, 0)),
        market_vwap: Some(Decimal::new(100, 0)),
        simulated_execution_vwap: Some(Decimal::new(10005, 2)),
        implementation_shortfall_bps: Some(Decimal::new(15, 0)),
        implementation_shortfall_amount: Some(Decimal::new(15, 2)),
        requested_amount: Decimal::ONE,
        filled_amount: Decimal::ONE,
        fill_rate: Decimal::ONE,
        residual_amount: Decimal::ZERO,
        actual_participation_rate: Some(Decimal::new(1, 1)),
        target_participation_rate: None,
        participation_rate_deviation: None,
        slice_count: 1,
        empty_window_count: 0,
        fee_amount: Decimal::new(10005, 5),
        explicit_slippage_amount: Decimal::new(5, 2),
    };

    let (first, duplicate) = tokio::join!(
        ballast_storage::create_replay_result(&pool, &run, &slices, &metrics),
        ballast_storage::create_replay_result(&pool, &run, &slices, &metrics),
    );
    let first = first.unwrap();
    let duplicate = duplicate.unwrap();
    assert_eq!(first.id, duplicate.id);
    assert_eq!(
        ballast_storage::list_replay_slices(&pool, first.id)
            .await
            .unwrap()
            .len(),
        1
    );
    let stored_metrics = ballast_storage::get_replay_metrics(&pool, first.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_metrics.fill_rate, Decimal::ONE);

    let mut conflicting = run.clone();
    conflicting.request_fingerprint = "different-fingerprint".to_owned();
    let error = ballast_storage::create_replay_result(&pool, &conflicting, &slices, &metrics)
        .await
        .unwrap_err();
    assert!(matches!(error, CreateReplayError::IdempotencyConflict));
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

#[tokio::test]
async fn historical_backfill_transitions_are_monotonic_under_concurrency() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let symbol = format!("MONOTONIC/USDT-{suffix}");
    let instrument = Instrument {
        id: InstrumentId::new(Exchange::Okx, MarketKind::Spot, &symbol).unwrap(),
        exchange_symbol: format!("MONOTONICUSDT{suffix}"),
        base_asset: "MONOTONIC".to_owned(),
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
    let instrument = ballast_storage::upsert_instruments(&pool, &[instrument])
        .await
        .unwrap()
        .remove(0);
    let start = chrono::DateTime::from_timestamp_micros(
        (Utc::now() - Duration::hours(2)).timestamp_micros(),
    )
    .unwrap();
    let middle = start + Duration::minutes(1);
    let end = start + Duration::minutes(2);
    let trade = |id: &str, seconds: i64| HistoricalTrade {
        exchange_trade_id: format!("{id}-{suffix}"),
        trade_time: start + Duration::seconds(seconds),
        price: Decimal::new(100, 0),
        quantity: Decimal::ONE,
        taker_side: "buy".to_owned(),
        source: "okx".to_owned(),
        observed_at: Utc::now(),
    };

    let pending_job = ballast_storage::create_or_get_historical_backfill(
        &pool,
        &NewHistoricalBackfill {
            idempotency_key: format!("parallel-pending-{suffix}"),
            instrument_id: instrument.id,
            data_type: "trades".to_owned(),
            timeframe: None,
            start_at: start,
            end_at: end,
        },
    )
    .await
    .unwrap();
    let first_page = vec![trade("same-page", 10)];
    let (first, second) = tokio::join!(
        ballast_storage::persist_trade_batch(
            &pool,
            pending_job.id,
            instrument.id,
            &first_page,
            middle,
            false,
        ),
        ballast_storage::persist_trade_batch(
            &pool,
            pending_job.id,
            instrument.id,
            &first_page,
            middle,
            false,
        )
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.cursor_at, middle);
    assert_eq!(second.cursor_at, middle);
    let pending = ballast_storage::get_historical_backfill(&pool, pending_job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.rows_written, 1);

    let completion_page = vec![trade("completion", 70)];
    let stale_page = vec![trade("stale", 20)];
    let (completed, continued) = tokio::join!(
        ballast_storage::persist_trade_batch(
            &pool,
            pending_job.id,
            instrument.id,
            &completion_page,
            end,
            true,
        ),
        ballast_storage::persist_trade_batch(
            &pool,
            pending_job.id,
            instrument.id,
            &stale_page,
            middle,
            false,
        )
    );
    assert_eq!(completed.unwrap().status, "completed");
    assert!(matches!(
        continued.unwrap().status.as_str(),
        "pending" | "completed"
    ));
    let completed_job = ballast_storage::get_historical_backfill(&pool, pending_job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed_job.status, "completed");
    assert_eq!(completed_job.cursor_at, end);

    let failure_job = ballast_storage::create_or_get_historical_backfill(
        &pool,
        &NewHistoricalBackfill {
            idempotency_key: format!("parallel-failure-{suffix}"),
            instrument_id: instrument.id,
            data_type: "trades".to_owned(),
            timeframe: None,
            start_at: start,
            end_at: end,
        },
    )
    .await
    .unwrap();
    let failure_completion = vec![trade("failure-completion", 80)];
    let (completed, failed) = tokio::join!(
        ballast_storage::persist_trade_batch(
            &pool,
            failure_job.id,
            instrument.id,
            &failure_completion,
            end,
            true,
        ),
        ballast_storage::mark_historical_backfill_failed(
            &pool,
            failure_job.id,
            "gateway_unavailable",
        )
    );
    assert_eq!(completed.unwrap().status, "completed");
    let failed = failed.unwrap();
    assert!(failed.status == "failed" || failed.status == "completed");
    let final_job = ballast_storage::mark_historical_backfill_running(&pool, failure_job.id)
        .await
        .unwrap();
    assert_eq!(final_job.status, "completed");
    assert_eq!(final_job.cursor_at, end);
    assert_eq!(final_job.last_error_code, None);

    let repeated = ballast_storage::persist_trade_batch(
        &pool,
        failure_job.id,
        instrument.id,
        &[trade("after-completion", 90)],
        middle,
        false,
    )
    .await
    .unwrap();
    assert_eq!(repeated.status, "completed");
    assert_eq!(repeated.rows_written, final_job.rows_written);
    let regression =
        sqlx::query("UPDATE historical_backfill_jobs SET status = 'failed' WHERE id = $1")
            .bind(failure_job.id)
            .execute(&pool)
            .await;
    assert!(regression.is_err());
}

#[tokio::test]
async fn historical_trade_snapshot_excludes_concurrent_earlier_inserts() {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL is not set; skipping PostgreSQL integration test");
        return;
    };
    let pool = ballast_storage::connect(&database_url).await.unwrap();
    ballast_storage::migrate(&pool).await.unwrap();
    let suffix = Utc::now().timestamp_nanos_opt().unwrap();
    let instrument = Instrument {
        id: InstrumentId::new(
            Exchange::Okx,
            MarketKind::Spot,
            format!("SNAPSHOT/USDT-{suffix}"),
        )
        .unwrap(),
        exchange_symbol: format!("SNAPSHOTUSDT{suffix}"),
        base_asset: "SNAPSHOT".to_owned(),
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
    let instrument = ballast_storage::upsert_instruments(&pool, &[instrument])
        .await
        .unwrap()
        .remove(0);
    let start = Utc::now() - Duration::minutes(10);
    let end = start + Duration::minutes(1);
    let job = ballast_storage::create_or_get_historical_backfill(
        &pool,
        &NewHistoricalBackfill {
            idempotency_key: format!("snapshot-history-{suffix}"),
            instrument_id: instrument.id,
            data_type: "trades".to_owned(),
            timeframe: None,
            start_at: start,
            end_at: end,
        },
    )
    .await
    .unwrap();
    let trade = |id: &str, seconds: i64| HistoricalTrade {
        exchange_trade_id: id.to_owned(),
        trade_time: start + Duration::seconds(seconds),
        price: Decimal::new(100, 0),
        quantity: Decimal::ONE,
        taker_side: "buy".to_owned(),
        source: "okx".to_owned(),
        observed_at: Utc::now(),
    };
    ballast_storage::persist_trade_batch(
        &pool,
        job.id,
        instrument.id,
        &[trade("a", 10), trade("c", 30)],
        end,
        true,
    )
    .await
    .unwrap();
    let watermark =
        ballast_storage::historical_trade_snapshot_high_watermark(&pool, instrument.id, start, end)
            .await
            .unwrap();
    ballast_storage::persist_trade_batch(
        &pool,
        job.id,
        instrument.id,
        &[trade("b-late", 20)],
        end,
        true,
    )
    .await
    .unwrap();

    let first = ballast_storage::list_historical_trade_page(
        &pool,
        instrument.id,
        start,
        end,
        watermark,
        None,
        1,
    )
    .await
    .unwrap();
    let second = ballast_storage::list_historical_trade_page(
        &pool,
        instrument.id,
        start,
        end,
        watermark,
        Some((first[0].trade_time, first[0].exchange_trade_id.clone())),
        10,
    )
    .await
    .unwrap();
    assert_eq!(first[0].exchange_trade_id, "a");
    assert_eq!(
        second
            .iter()
            .map(|trade| trade.exchange_trade_id.as_str())
            .collect::<Vec<_>>(),
        vec!["c"]
    );
}
