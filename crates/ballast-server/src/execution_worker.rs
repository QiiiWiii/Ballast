use std::{
    collections::{HashMap, HashSet, VecDeque},
    num::NonZeroU32,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use ballast_core::{Exchange, Instrument, QuantityUnit, Side, StrategyKind};
use ballast_execution::{
    ManagedSliceInput, OrderState, calculate_managed_slice, protected_conversion_price,
    stable_client_order_id,
};
use ballast_gateway_client::{
    GatewayClient, GatewayClientError, execution_order_snapshot_from_proto, proto,
};
use ballast_simulator::{BookLevel, estimate_protected_ioc_fill};
use ballast_storage::{
    ChildOrderStateUpdate, DatabasePool, NewChildOrder, SliceRecord, StoredExecutionTask,
    compare_and_set_child_order_state, create_or_get_child_order, get_account,
};
use chrono::{DateTime, Utc};
use futures_util::stream::{self, StreamExt};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock};
use tonic::Code;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    metrics::AppMetrics,
    order_book_cache::{MAX_BOOK_AGE_MS, OrderBookCache},
    risk::{self, RiskContext, RiskError, RiskStage},
};

#[derive(Clone, Default)]
pub struct TradeVolumeTracker {
    entries: Arc<Mutex<HashMap<Uuid, Arc<TradeEntry>>>>,
}

/// Serializes paper ticks that share an instrument so concurrent claim batches
/// cannot race on residual/slice sequencing for the same market.
#[derive(Clone, Default)]
struct InstrumentTickGate {
    locks: Arc<Mutex<HashMap<Uuid, Arc<Mutex<()>>>>>,
}

impl InstrumentTickGate {
    async fn lock(
        &self,
        instrument_id: Uuid,
        metrics: &AppMetrics,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().await;
            locks
                .entry(instrument_id)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        // Prefer try_lock first so uncontended ticks stay cheap.
        match lock.clone().try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => {
                metrics.instrument_tick_serialized.inc();
                lock.lock_owned().await
            }
        }
    }
}

#[derive(Default)]
struct TradeEntry {
    connected: RwLock<bool>,
    trades: Mutex<VecDeque<TrackedTrade>>,
    seen: Mutex<HashSet<String>>,
}

#[derive(Debug, Clone)]
struct TrackedTrade {
    event_id: String,
    native_quantity: Decimal,
    price: Decimal,
    received_at_ms: i64,
}

impl TradeVolumeTracker {
    pub async fn ensure_subscription(
        &self,
        instrument_id: Uuid,
        instrument: Instrument,
        gateway: GatewayClient,
        database: DatabasePool,
    ) {
        let mut entries = self.entries.lock().await;
        if entries.contains_key(&instrument_id) {
            return;
        }
        let entry = Arc::new(TradeEntry::default());
        entries.insert(instrument_id, entry.clone());
        drop(entries);
        tokio::spawn(run_trade_subscription(
            entry,
            instrument_id,
            instrument,
            gateway,
            database,
        ));
    }

    async fn volume_since(
        &self,
        instrument_id: Uuid,
        instrument: &Instrument,
        unit: QuantityUnit,
        since: DateTime<Utc>,
    ) -> Result<Decimal, &'static str> {
        let entry = self
            .entries
            .lock()
            .await
            .get(&instrument_id)
            .cloned()
            .ok_or("trade_subscription_missing")?;
        if !*entry.connected.read().await {
            return Err("trade_stream_not_connected");
        }
        let since_ms = since.timestamp_millis();
        let trades = entry.trades.lock().await;
        trades
            .iter()
            .filter(|trade| trade.received_at_ms > since_ms)
            .try_fold(Decimal::ZERO, |total, trade| {
                let amount = match unit {
                    QuantityUnit::Contracts => trade.native_quantity,
                    QuantityUnit::BaseQuantity => instrument
                        .native_to_base_quantity(trade.native_quantity, trade.price)
                        .map_err(|_| "trade_volume_conversion_failed")?,
                    QuantityUnit::QuoteNotional => instrument
                        .native_to_quote_quantity(trade.native_quantity, trade.price)
                        .map_err(|_| "trade_volume_conversion_failed")?,
                };
                Ok(total + amount)
            })
    }
}

async fn run_trade_subscription(
    entry: Arc<TradeEntry>,
    instrument_id: Uuid,
    instrument: Instrument,
    gateway: GatewayClient,
    database: DatabasePool,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match gateway.watch_trades(&instrument.id).await {
            Ok(mut stream) => {
                while let Ok(Some(event)) = stream.message().await {
                    match event.payload {
                        Some(proto::trade_stream_event::Payload::Status(status)) => {
                            let connected = status.state == proto::StreamState::Connected as i32;
                            *entry.connected.write().await = connected;
                            let state_text = stream_state_text(status.state);
                            if let Err(error) = upsert_subscription_health(
                                &database,
                                &instrument,
                                instrument_id,
                                state_text,
                                status.reconnect_attempt as i32,
                                status.error_code.as_deref(),
                                None,
                            )
                            .await
                            {
                                warn!(%error, "failed to persist trade subscription status");
                            }
                            if connected {
                                backoff = Duration::from_secs(1);
                            }
                        }
                        Some(proto::trade_stream_event::Payload::Trade(value)) => {
                            match ballast_gateway_client::trade_from_proto(value) {
                                Ok(trade) => {
                                    let mut seen = entry.seen.lock().await;
                                    if !seen.insert(trade.event_id.clone()) {
                                        continue;
                                    }
                                    let mut trades = entry.trades.lock().await;
                                    trades.push_back(TrackedTrade {
                                        event_id: trade.event_id,
                                        native_quantity: trade.quantity,
                                        price: trade.price,
                                        received_at_ms: trade.gateway_received_at_ms,
                                    });
                                    let cutoff = Utc::now().timestamp_millis() - 3_600_000;
                                    while trades
                                        .front()
                                        .is_some_and(|item| item.received_at_ms < cutoff)
                                    {
                                        if let Some(removed) = trades.pop_front() {
                                            seen.remove(&removed.event_id);
                                        }
                                    }
                                    if let Err(error) = upsert_subscription_health(
                                        &database,
                                        &instrument,
                                        instrument_id,
                                        "connected",
                                        0,
                                        None,
                                        DateTime::<Utc>::from_timestamp_millis(
                                            trade.gateway_received_at_ms,
                                        ),
                                    )
                                    .await
                                    {
                                        warn!(%error, "failed to persist trade subscription activity");
                                    }
                                }
                                Err(error) => warn!(%error, "discarding invalid gateway trade"),
                            }
                        }
                        None => {}
                    }
                }
            }
            Err(error) => {
                warn!(%error, instrument = %instrument.id.symbol, "trade subscription failed")
            }
        }
        *entry.connected.write().await = false;
        if let Err(error) = upsert_subscription_health(
            &database,
            &instrument,
            instrument_id,
            "closed",
            0,
            Some("stream_ended"),
            None,
        )
        .await
        {
            warn!(%error, "failed to persist closed trade subscription");
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn upsert_subscription_health(
    database: &DatabasePool,
    instrument: &Instrument,
    instrument_id: Uuid,
    status: &str,
    reconnect_attempt: i32,
    error_code: Option<&str>,
    last_event_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO market_subscription_health (
            exchange, instrument_id, stream_kind, status, reconnect_attempt,
            error_code, last_event_at, observed_at
        ) VALUES ($1, $2, 'trades', $3, $4, $5, $6, now())
        ON CONFLICT (instrument_id, stream_kind) DO UPDATE SET
            status = EXCLUDED.status,
            reconnect_attempt = EXCLUDED.reconnect_attempt,
            error_code = EXCLUDED.error_code,
            last_event_at = COALESCE(EXCLUDED.last_event_at, market_subscription_health.last_event_at),
            observed_at = now()
        "#,
    )
    .bind(exchange_text(instrument.id.exchange))
    .bind(instrument_id)
    .bind(status)
    .bind(reconnect_attempt)
    .bind(error_code)
    .bind(last_event_at)
    .execute(database)
    .await?;
    Ok(())
}

fn stream_state_text(value: i32) -> &'static str {
    match proto::StreamState::try_from(value).unwrap_or(proto::StreamState::Closed) {
        proto::StreamState::Connecting | proto::StreamState::Unspecified => "connecting",
        proto::StreamState::Connected => "connected",
        proto::StreamState::Reconnecting => "reconnecting",
        proto::StreamState::Stale => "stale",
        proto::StreamState::Closed => "closed",
    }
}

const fn exchange_text(value: ballast_core::Exchange) -> &'static str {
    match value {
        ballast_core::Exchange::Binance => "binance",
        ballast_core::Exchange::Okx => "okx",
        ballast_core::Exchange::Bybit => "bybit",
        ballast_core::Exchange::GateIo => "gate_io",
        ballast_core::Exchange::Bitget => "bitget",
    }
}

/// Cap concurrent paper ticks so gateway/db pressure stays bounded while
/// different instruments still make progress in the same claim batch.
const MAX_CONCURRENT_TASKS: usize = 8;

pub fn spawn_worker(
    database: DatabasePool,
    gateway: GatewayClient,
    trade_volume: TradeVolumeTracker,
    order_books: OrderBookCache,
    metrics: AppMetrics,
) {
    let instrument_gate = InstrumentTickGate::default();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let lease_until = Utc::now() + chrono::Duration::seconds(15);
            match ballast_storage::claim_runnable_tasks(&database, 20, lease_until).await {
                Ok(tasks) => {
                    metrics.worker_batch_size.set(tasks.len() as i64);
                    metrics.claimed_tasks.inc_by(tasks.len() as u64);
                    stream::iter(tasks)
                        .for_each_concurrent(MAX_CONCURRENT_TASKS, |task| {
                            let database = database.clone();
                            let gateway = gateway.clone();
                            let trade_volume = trade_volume.clone();
                            let order_books = order_books.clone();
                            let metrics = metrics.clone();
                            let instrument_gate = instrument_gate.clone();
                            async move {
                                let _instrument_guard =
                                    instrument_gate.lock(task.instrument_id, &metrics).await;
                                if let Err(error) = process_task(
                                    &database,
                                    &gateway,
                                    &trade_volume,
                                    &order_books,
                                    task,
                                    &metrics,
                                )
                                .await
                                {
                                    error!(%error, "paper execution tick failed");
                                }
                            }
                        })
                        .await;
                }
                Err(error) => error!(%error, "failed to claim paper execution tasks"),
            }
        }
    });
}

async fn process_task(
    database: &DatabasePool,
    gateway: &GatewayClient,
    trade_volume: &TradeVolumeTracker,
    order_books: &OrderBookCache,
    task: StoredExecutionTask,
    metrics: &AppMetrics,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = Utc::now();
    if now >= task.deadline_at {
        ballast_storage::mark_task_state(database, task.id, "expired", None, task.deadline_at)
            .await?;
        return Ok(());
    }
    let stored_instrument = ballast_storage::get_instrument(database, task.instrument_id)
        .await?
        .ok_or("task instrument no longer exists")?;
    let instrument = stored_instrument.instrument;
    if task.execution_mode == "live" && instrument.id.exchange != Exchange::Okx {
        ballast_storage::mark_task_state(
            database,
            task.id,
            "failed",
            Some("live_order_exchange_unsupported"),
            task.deadline_at,
        )
        .await?;
        return Ok(());
    }
    let side = parse_side(&task.side)?;
    let unit = parse_quantity_unit(&task.quantity_unit)?;
    let strategy_kind = parse_strategy(&task.strategy_kind)?;
    order_books
        .ensure_subscription(
            task.instrument_id,
            instrument.clone(),
            gateway.clone(),
            database.clone(),
        )
        .await;
    if strategy_kind == StrategyKind::Pov {
        trade_volume
            .ensure_subscription(
                task.instrument_id,
                instrument.clone(),
                gateway.clone(),
                database.clone(),
            )
            .await;
    }

    let order_book = match order_books
        .get(task.instrument_id, &instrument, gateway, metrics)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            warn!(%error, task_id = %task.id, "pausing task because order book is unavailable");
            pause_task(database, &task, "order_book_unavailable", metrics).await?;
            return Ok(());
        }
    };
    let age_ms = Utc::now().timestamp_millis() - order_book.gateway_received_at_ms;
    if age_ms > MAX_BOOK_AGE_MS {
        pause_task(database, &task, "order_book_stale", metrics).await?;
        return Ok(());
    }
    let reference_price = match side {
        Side::Buy => order_book.asks.first().map(|level| level.price),
        Side::Sell => order_book.bids.first().map(|level| level.price),
    }
    .ok_or("order book has no executable side")?;
    let remaining = (task.requested_amount - task.executed_amount).max(Decimal::ZERO);
    if remaining.is_zero() {
        ballast_storage::mark_task_state(database, task.id, "completed", None, task.deadline_at)
            .await?;
        return Ok(());
    }
    let market_volume = if strategy_kind == StrategyKind::Pov {
        let since = task
            .last_tick_at
            .unwrap_or_else(|| now - chrono::Duration::milliseconds(task.slice_interval_ms));
        match trade_volume
            .volume_since(task.instrument_id, &instrument, unit, since)
            .await
        {
            Ok(value) => value,
            Err(reason) => {
                pause_task(database, &task, reason, metrics).await?;
                return Ok(());
            }
        }
    } else {
        Decimal::ZERO
    };
    let requested_slice = calculate_managed_slice(ManagedSliceInput {
        strategy: strategy_kind,
        target_amount: task.requested_amount,
        executed_amount: task.executed_amount,
        market_volume,
        remaining_time_ms: (task.deadline_at - now).num_milliseconds().max(1),
        slice_interval_ms: task.slice_interval_ms,
        participation_rate: strategy_decimal(&task.strategy_params, "participation_rate")?,
        max_slice_amount: strategy_decimal(&task.strategy_params, "max_slice_amount")?,
    })?;
    if requested_slice <= Decimal::ZERO {
        ballast_storage::mark_task_state(database, task.id, "running", None, next_tick(&task, now))
            .await?;
        return Ok(());
    }
    let conversion_price =
        protected_conversion_price(side, unit, reference_price, task.max_slippage_bps);
    let native_quantity =
        instrument.target_to_native_quantity(requested_slice, unit, conversion_price)?;
    if !is_native_quantity_executable(&instrument, native_quantity, reference_price)? {
        let residual_native =
            instrument.target_to_native_quantity(remaining, unit, conversion_price)?;
        let task_status =
            task_state_for_non_executable_slice(&instrument, residual_native, reference_price)?;
        if task_status == "running" {
            ballast_storage::defer_task_tick(
                database,
                task.id,
                next_tick(&task, now),
                serde_json::json!({
                    "reason": "below_minimum_slice",
                    "requested_amount": requested_slice.to_string(),
                    "native_quantity": native_quantity.to_string(),
                    "residual_amount": remaining.to_string(),
                    "residual_native_quantity": residual_native.to_string(),
                    "minimum_quantity": instrument.minimum_quantity.map(|value| value.to_string()),
                    "minimum_notional": instrument.minimum_notional.map(|value| value.to_string()),
                    "reference_price": reference_price.to_string(),
                }),
            )
            .await?;
        } else {
            info!(task_id = %task.id, residual = %remaining, "completing task with non-executable residual");
            ballast_storage::mark_task_state(
                database,
                task.id,
                "completed",
                None,
                task.deadline_at,
            )
            .await?;
        }
        return Ok(());
    }
    if task.execution_mode == "live" {
        return process_live_slice(
            database,
            gateway,
            &task,
            &instrument,
            side,
            unit,
            strategy_kind,
            requested_slice,
            remaining,
            market_volume,
            native_quantity,
            reference_price,
            order_book,
            metrics,
        )
        .await;
    }
    let bids: Vec<_> = order_book
        .bids
        .iter()
        .map(|level| BookLevel {
            price: level.price,
            quantity: level.quantity,
        })
        .collect();
    let asks: Vec<_> = order_book
        .asks
        .iter()
        .map(|level| BookLevel {
            price: level.price,
            quantity: level.quantity,
        })
        .collect();
    let fill = estimate_protected_ioc_fill(
        &instrument,
        side,
        native_quantity,
        task.max_slippage_bps as u32,
        &bids,
        &asks,
    )?;
    let executed_delta = match unit {
        QuantityUnit::Contracts => fill.filled_native_quantity,
        QuantityUnit::BaseQuantity => fill.filled_base_quantity,
        QuantityUnit::QuoteNotional => fill.filled_quote_quantity,
    };
    let residual = (remaining - executed_delta).max(Decimal::ZERO);
    let residual_native = instrument.target_to_native_quantity(residual, unit, reference_price)?;
    let task_status = if residual.is_zero()
        || !is_native_quantity_executable(&instrument, residual_native, reference_price)?
    {
        "completed"
    } else {
        "running"
    };
    let slice_status = if fill.filled_native_quantity.is_zero() {
        "unfilled"
    } else if fill.unfilled_native_quantity.is_zero() {
        "filled"
    } else {
        "partial"
    };
    let sequence = ballast_storage::next_slice_sequence(database, task.id).await?;
    let next_tick_at = if task_status == "completed" {
        task.deadline_at
    } else {
        next_tick(&task, now)
    };
    ballast_storage::record_slice(
        database,
        SliceRecord {
            task_id: task.id,
            child_order_id: None,
            sequence,
            requested_amount: requested_slice,
            native_quantity,
            filled_native_quantity: fill.filled_native_quantity,
            filled_base_quantity: fill.filled_base_quantity,
            filled_quote_quantity: fill.filled_quote_quantity,
            average_price: fill.average_price,
            worst_price: fill.worst_price,
            slippage_bps: fill.slippage_bps,
            fee_amount: fill.fee_amount,
            fee_asset: fill.fee_asset,
            fee_status: if fill.fee_amount.is_some() {
                "calculated"
            } else {
                "unavailable"
            },
            status: slice_status,
            market_snapshot: market_snapshot(&order_book),
            decision_input: json!({
                "strategy": task.strategy_kind,
                "remaining_amount": remaining.to_string(),
                "market_volume": market_volume.to_string(),
                "reference_price": reference_price.to_string(),
                "max_slippage_bps": task.max_slippage_bps,
            }),
            executed_amount_delta: executed_delta,
            residual_amount: residual,
            next_tick_at,
            task_status,
        },
    )
    .await?;
    metrics.slices.with_label_values(&[slice_status]).inc();
    if let Some(slippage) = fill.slippage_bps.and_then(|value| value.to_f64()) {
        metrics.slice_slippage_bps.observe(slippage);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn process_live_slice(
    database: &DatabasePool,
    gateway: &GatewayClient,
    task: &StoredExecutionTask,
    instrument: &Instrument,
    side: Side,
    unit: QuantityUnit,
    strategy_kind: StrategyKind,
    requested_slice: Decimal,
    remaining: Decimal,
    market_volume: Decimal,
    native_quantity: Decimal,
    reference_price: Decimal,
    order_book: ballast_gateway_client::OrderBook,
    _metrics: &AppMetrics,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let account = get_account(database, &task.account_id)
        .await?
        .ok_or("live task account no longer exists")?;
    let _submit_lock = risk::acquire_submit_lock(database).await?;
    let limit_price = protected_limit_price(side, reference_price, task.max_slippage_bps);
    if limit_price <= Decimal::ZERO {
        ballast_storage::mark_task_state(
            database,
            task.id,
            "failed",
            Some("protected_price_invalid"),
            task.deadline_at,
        )
        .await?;
        return Ok(());
    }
    let conversion_price =
        protected_conversion_price(side, unit, reference_price, task.max_slippage_bps);
    let full_native_quantity =
        instrument.target_to_native_quantity(task.requested_amount, unit, conversion_price)?;
    let task_notional =
        instrument.native_to_quote_quantity(full_native_quantity, reference_price)?;
    let order_notional = instrument.native_to_quote_quantity(native_quantity, limit_price)?;
    let snapshot = risk::latest_snapshot(database, &task.account_id).await?;
    let current_exposure = risk::snapshot_exposure(snapshot.as_ref(), &instrument.id.symbol)
        .map_err(|code| code.to_owned())?;
    let account_age_ms = risk::snapshot_age_ms(snapshot.as_ref()).map_err(str::to_owned)?;
    let projected_exposure = instrument
        .native_to_base_quantity(native_quantity, limit_price)?
        .abs();
    let market_age_ms = (Utc::now().timestamp_millis() - order_book.gateway_received_at_ms).max(0);
    let exchange = exchange_text(instrument.id.exchange);
    if let Err(error) = risk::check(
        database,
        RiskStage::Submit,
        &RiskContext {
            task_id: Some(task.id),
            account_id: task.account_id.clone(),
            exchange: exchange.to_owned(),
            instrument_id: task.instrument_id,
            backend: task.execution_backend.clone(),
            order_notional,
            task_notional,
            market_age_ms,
            account_age_ms,
            max_slippage_bps: task.max_slippage_bps,
            daily_notional: order_notional,
            net_exposure: current_exposure + projected_exposure,
            native_algo_orders: 0,
            native_duration_seconds: 0,
        },
    )
    .await
    {
        let reason = match error {
            RiskError::Denied(code) => code,
            RiskError::Database(_) => "risk_database_error",
        };
        ballast_storage::mark_task_state(
            database,
            task.id,
            "failed",
            Some(reason),
            task.deadline_at,
        )
        .await?;
        return Ok(());
    }

    let sequence = ballast_storage::next_slice_sequence(database, task.id).await?;
    let sequence = NonZeroU32::new(u32::try_from(sequence).map_err(|_| "slice_sequence_overflow")?)
        .ok_or("slice_sequence_invalid")?;
    let client_order_id = stable_client_order_id(task.id, sequence);
    let child = create_or_get_child_order(
        database,
        &NewChildOrder {
            task_id: task.id,
            exchange: exchange.to_owned(),
            account_id: task.account_id.clone(),
            request_id: Uuid::now_v7(),
            client_order_id: client_order_id.clone(),
            side: side_text(side).to_owned(),
            order_type: "limit_ioc".to_owned(),
            order_backend: "managed_ioc".to_owned(),
            quantity: native_quantity,
            limit_price: Some(limit_price),
        },
    )
    .await?;
    let child_is_pending = child.status == OrderState::SubmissionPending;
    if !child_is_pending && !child.status.is_terminal() {
        pause_live_task(database, task, child.status.as_str()).await?;
        return Ok(());
    }

    let account_ref = proto::AccountRef {
        account_id: account.id.clone(),
        exchange: exchange_proto(instrument.id.exchange),
        environment: account_environment_proto(&account.environment)?,
    };
    let instrument_key = proto::InstrumentKey {
        exchange: exchange_proto(instrument.id.exchange),
        market_kind: market_kind_proto(instrument.id.market_kind),
        symbol: instrument.id.symbol.clone(),
    };
    let lookup = || proto::GetOrderByClientIdRequest {
        account: Some(account_ref.clone()),
        instrument: Some(instrument_key.clone()),
        request_id: Uuid::now_v7().to_string(),
        client_order_id: client_order_id.clone(),
    };
    let response = match gateway.get_order_by_client_id(lookup()).await {
        Ok(response) => response,
        Err(error) if is_not_found(&error) && child_is_pending => {
            match gateway
                .place_ioc(proto::PlaceIocOrderRequest {
                    account: Some(account_ref),
                    instrument: Some(instrument_key),
                    request_id: Uuid::now_v7().to_string(),
                    client_order_id: client_order_id.clone(),
                    side: side_proto(side),
                    quantity: native_quantity.to_string(),
                    limit_price: limit_price.to_string(),
                })
                .await
            {
                Ok(response) => response,
                Err(error) if is_unknown_submission(&error) => {
                    mark_submission_unknown(database, task, &child, "submit_result_unknown")
                        .await?;
                    return Ok(());
                }
                Err(error) => {
                    mark_submission_failed(database, task, &child, gateway_error_code(&error))
                        .await?;
                    return Ok(());
                }
            }
        }
        Err(error) if is_not_found(&error) => {
            ballast_storage::mark_task_state(
                database,
                task.id,
                "failed",
                Some("terminal_order_not_found"),
                task.deadline_at,
            )
            .await?;
            warn!(%error, task_id = %task.id, client_order_id = %client_order_id, "terminal live order disappeared from the venue");
            return Ok(());
        }
        Err(error) => {
            if child_is_pending {
                mark_submission_unknown(database, task, &child, "pre_submit_query_unknown").await?;
            }
            warn!(%error, task_id = %task.id, client_order_id = %client_order_id, "live order query was inconclusive before submission");
            return Ok(());
        }
    };
    validate_live_response(
        task,
        instrument,
        side,
        native_quantity,
        &client_order_id,
        &response,
    )?;
    let snapshot = execution_order_snapshot_from_proto(response.clone())?;
    compare_and_set_child_order_state(
        database,
        &child.exchange,
        &child.account_id,
        &child.client_order_id,
        &[child.status],
        &ChildOrderStateUpdate {
            status: snapshot.state,
            exchange_order_id: snapshot.exchange_order_id.clone(),
            filled_quantity: snapshot.filled_quantity,
            average_price: snapshot.average_price,
            limit_price: child.limit_price,
            raw_status: None,
            state_reason: response.reason_code.clone(),
            submitted_at: child.submitted_at.or(Some(Utc::now())),
            last_reconciled_at: Some(snapshot.observed_at),
        },
    )
    .await?;
    if snapshot.state.is_terminal() {
        record_live_slice(
            database,
            task,
            instrument,
            side,
            unit,
            strategy_kind,
            requested_slice,
            remaining,
            market_volume,
            native_quantity,
            reference_price,
            child.id,
            &order_book,
            &snapshot,
        )
        .await?;
    } else {
        pause_live_task(database, task, snapshot.state.as_str()).await?;
    }
    Ok(())
}

async fn mark_submission_unknown(
    database: &DatabasePool,
    task: &StoredExecutionTask,
    child: &ballast_storage::StoredChildOrder,
    reason: &'static str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    compare_and_set_child_order_state(
        database,
        &child.exchange,
        &child.account_id,
        &child.client_order_id,
        &[child.status],
        &ChildOrderStateUpdate {
            status: OrderState::SubmissionUnknown,
            exchange_order_id: child.exchange_order_id.clone(),
            filled_quantity: child.filled_quantity,
            average_price: child.average_price,
            limit_price: child.limit_price,
            raw_status: None,
            state_reason: Some(reason.to_owned()),
            submitted_at: child.submitted_at,
            last_reconciled_at: Some(Utc::now()),
        },
    )
    .await?;
    pause_live_task(database, task, "submission_unknown").await?;
    Ok(())
}

async fn mark_submission_failed(
    database: &DatabasePool,
    task: &StoredExecutionTask,
    child: &ballast_storage::StoredChildOrder,
    reason: &'static str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    compare_and_set_child_order_state(
        database,
        &child.exchange,
        &child.account_id,
        &child.client_order_id,
        &[child.status],
        &ChildOrderStateUpdate {
            status: OrderState::Failed,
            exchange_order_id: child.exchange_order_id.clone(),
            filled_quantity: child.filled_quantity,
            average_price: child.average_price,
            limit_price: child.limit_price,
            raw_status: None,
            state_reason: Some(reason.to_owned()),
            submitted_at: child.submitted_at,
            last_reconciled_at: Some(Utc::now()),
        },
    )
    .await?;
    ballast_storage::mark_task_state(database, task.id, "failed", Some(reason), task.deadline_at)
        .await?;
    Ok(())
}

async fn pause_live_task(
    database: &DatabasePool,
    task: &StoredExecutionTask,
    reason: &str,
) -> Result<(), sqlx::Error> {
    ballast_storage::mark_task_state(
        database,
        task.id,
        "paused",
        Some(reason),
        next_tick(task, Utc::now()),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn record_live_slice(
    database: &DatabasePool,
    task: &StoredExecutionTask,
    instrument: &Instrument,
    side: Side,
    unit: QuantityUnit,
    strategy_kind: StrategyKind,
    requested_slice: Decimal,
    remaining: Decimal,
    market_volume: Decimal,
    native_quantity: Decimal,
    reference_price: Decimal,
    child_order_id: Uuid,
    order_book: &ballast_gateway_client::OrderBook,
    snapshot: &ballast_execution::OrderSnapshot,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filled_native_quantity = snapshot.filled_quantity;
    let average_price = if filled_native_quantity.is_zero() {
        None
    } else {
        Some(
            snapshot
                .average_price
                .ok_or("live_fill_average_price_missing")?,
        )
    };
    let filled_base_quantity = average_price
        .map(|price| instrument.native_to_base_quantity(filled_native_quantity, price))
        .transpose()?
        .unwrap_or(Decimal::ZERO);
    let filled_quote_quantity = average_price
        .map(|price| instrument.native_to_quote_quantity(filled_native_quantity, price))
        .transpose()?
        .unwrap_or(Decimal::ZERO);
    let executed_delta = match unit {
        QuantityUnit::Contracts => filled_native_quantity,
        QuantityUnit::BaseQuantity => filled_base_quantity,
        QuantityUnit::QuoteNotional => filled_quote_quantity,
    };
    let residual = (remaining - executed_delta).max(Decimal::ZERO);
    let residual_native = instrument.target_to_native_quantity(
        residual,
        unit,
        protected_conversion_price(side, unit, reference_price, task.max_slippage_bps),
    )?;
    let rejected = matches!(snapshot.state, OrderState::Rejected | OrderState::Failed);
    let task_status = if rejected {
        "failed"
    } else if residual.is_zero()
        || !is_native_quantity_executable(instrument, residual_native, reference_price)?
    {
        "completed"
    } else {
        "running"
    };
    let slice_status = if rejected {
        "rejected"
    } else if filled_native_quantity.is_zero() {
        "unfilled"
    } else if residual.is_zero() {
        "filled"
    } else {
        "partial"
    };
    let sequence = ballast_storage::next_slice_sequence(database, task.id).await?;
    ballast_storage::record_slice(
        database,
        SliceRecord {
            task_id: task.id,
            child_order_id: Some(child_order_id),
            sequence,
            requested_amount: requested_slice,
            native_quantity,
            filled_native_quantity,
            filled_base_quantity,
            filled_quote_quantity,
            average_price,
            worst_price: average_price,
            slippage_bps: None,
            fee_amount: None,
            fee_asset: None,
            fee_status: "unavailable",
            status: slice_status,
            market_snapshot: market_snapshot(order_book),
            decision_input: json!({
                "execution_mode": "live",
                "strategy": strategy_kind,
                "remaining_amount": remaining.to_string(),
                "market_volume": market_volume.to_string(),
                "reference_price": reference_price.to_string(),
                "max_slippage_bps": task.max_slippage_bps,
            }),
            executed_amount_delta: executed_delta,
            residual_amount: residual,
            next_tick_at: if task_status == "completed" {
                task.deadline_at
            } else {
                next_tick(task, Utc::now())
            },
            task_status,
        },
    )
    .await?;
    ballast_storage::refresh_slice_fees(database, child_order_id).await?;
    Ok(())
}

fn validate_live_response(
    task: &StoredExecutionTask,
    instrument: &Instrument,
    side: Side,
    quantity: Decimal,
    client_order_id: &str,
    response: &proto::OrderSnapshot,
) -> Result<(), &'static str> {
    let account = response
        .account
        .as_ref()
        .ok_or("live_order_account_missing")?;
    if account.account_id != task.account_id
        || account.exchange != exchange_proto(instrument.id.exchange)
    {
        return Err("live_order_account_mismatch");
    }
    let response_instrument = response
        .instrument
        .as_ref()
        .ok_or("live_order_instrument_missing")?;
    if response_instrument.exchange != exchange_proto(instrument.id.exchange)
        || response_instrument.market_kind != market_kind_proto(instrument.id.market_kind)
        || response_instrument.symbol != instrument.id.symbol
    {
        return Err("live_order_instrument_mismatch");
    }
    if response.client_order_id != client_order_id {
        return Err("live_order_client_id_missing");
    }
    if response.side != side_proto(side) {
        return Err("live_order_side_mismatch");
    }
    let observed_quantity =
        Decimal::from_str(&response.quantity).map_err(|_| "live_order_quantity_invalid")?;
    if observed_quantity != quantity {
        return Err("live_order_quantity_mismatch");
    }
    Ok(())
}

fn protected_limit_price(side: Side, reference_price: Decimal, max_slippage_bps: i32) -> Decimal {
    let ratio = Decimal::from(max_slippage_bps) / Decimal::from(10_000);
    match side {
        Side::Buy => reference_price * (Decimal::ONE + ratio),
        Side::Sell => reference_price * (Decimal::ONE - ratio),
    }
}

fn is_not_found(error: &GatewayClientError) -> bool {
    matches!(error, GatewayClientError::Rpc(status) if status.code() == Code::NotFound)
}

fn is_unknown_submission(error: &GatewayClientError) -> bool {
    match error {
        GatewayClientError::Transport(_) => true,
        GatewayClientError::Rpc(status) => matches!(
            status.code(),
            Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted
        ),
        _ => false,
    }
}

fn gateway_error_code(error: &GatewayClientError) -> &'static str {
    match error {
        GatewayClientError::Transport(_) => "gateway_transport_failed",
        GatewayClientError::Rpc(status) => match status.code() {
            Code::InvalidArgument => "gateway_order_invalid",
            Code::FailedPrecondition => "gateway_order_rejected",
            Code::PermissionDenied | Code::Unauthenticated => "gateway_order_access_denied",
            _ => "gateway_order_submit_failed",
        },
        _ => "gateway_order_protocol_invalid",
    }
}

fn exchange_proto(value: ballast_core::Exchange) -> i32 {
    match value {
        ballast_core::Exchange::Binance => proto::Exchange::Binance as i32,
        ballast_core::Exchange::Okx => proto::Exchange::Okx as i32,
        ballast_core::Exchange::Bybit => proto::Exchange::Bybit as i32,
        ballast_core::Exchange::GateIo => proto::Exchange::GateIo as i32,
        ballast_core::Exchange::Bitget => proto::Exchange::Bitget as i32,
    }
}

fn market_kind_proto(value: ballast_core::MarketKind) -> i32 {
    match value {
        ballast_core::MarketKind::Spot => proto::MarketKind::Spot as i32,
        ballast_core::MarketKind::Perpetual => proto::MarketKind::Perpetual as i32,
    }
}

fn account_environment_proto(value: &str) -> Result<i32, &'static str> {
    match value {
        "demo" => Ok(proto::AccountEnvironment::Demo as i32),
        "production" => Ok(proto::AccountEnvironment::Production as i32),
        _ => Err("live_account_environment_invalid"),
    }
}

fn side_proto(value: Side) -> i32 {
    match value {
        Side::Buy => proto::Side::Buy as i32,
        Side::Sell => proto::Side::Sell as i32,
    }
}

fn side_text(value: Side) -> &'static str {
    match value {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

fn strategy_decimal(
    params: &Value,
    field: &'static str,
) -> Result<Option<Decimal>, Box<dyn std::error::Error + Send + Sync>> {
    let Some(value) = params.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().ok_or("strategy decimal must be a string")?;
    Ok(Some(Decimal::from_str(text)?))
}

fn is_native_quantity_executable(
    instrument: &Instrument,
    native_quantity: Decimal,
    reference_price: Decimal,
) -> Result<bool, ballast_core::DomainError> {
    if native_quantity <= Decimal::ZERO {
        return Ok(false);
    }
    if instrument
        .minimum_quantity
        .is_some_and(|minimum| native_quantity < minimum)
    {
        return Ok(false);
    }
    if let Some(minimum) = instrument.minimum_notional {
        let notional = instrument.native_to_quote_quantity(native_quantity, reference_price)?;
        if notional < minimum {
            return Ok(false);
        }
    }
    Ok(true)
}

fn task_state_for_non_executable_slice(
    instrument: &Instrument,
    residual_native: Decimal,
    reference_price: Decimal,
) -> Result<&'static str, ballast_core::DomainError> {
    Ok(
        if is_native_quantity_executable(instrument, residual_native, reference_price)? {
            "running"
        } else {
            "completed"
        },
    )
}

async fn pause_task(
    database: &DatabasePool,
    task: &StoredExecutionTask,
    reason: &'static str,
    metrics: &AppMetrics,
) -> Result<(), sqlx::Error> {
    if task.status != "paused" || task.paused_reason.as_deref() != Some(reason) {
        metrics.paused_tasks.with_label_values(&[reason]).inc();
    }
    ballast_storage::mark_task_state(
        database,
        task.id,
        "paused",
        Some(reason),
        next_tick(task, Utc::now()),
    )
    .await
}

fn next_tick(task: &StoredExecutionTask, now: DateTime<Utc>) -> DateTime<Utc> {
    (now + chrono::Duration::milliseconds(task.slice_interval_ms)).min(task.deadline_at)
}

fn market_snapshot(order_book: &ballast_gateway_client::OrderBook) -> Value {
    json!({
        "exchange_time_ms": order_book.exchange_time_ms,
        "gateway_received_at_ms": order_book.gateway_received_at_ms,
        "sequence": order_book.sequence,
        "bids": order_book.bids.iter().take(10).map(|level| json!({
            "price": level.price.to_string(), "quantity": level.quantity.to_string()
        })).collect::<Vec<_>>(),
        "asks": order_book.asks.iter().take(10).map(|level| json!({
            "price": level.price.to_string(), "quantity": level.quantity.to_string()
        })).collect::<Vec<_>>(),
    })
}

fn parse_side(value: &str) -> Result<Side, &'static str> {
    match value {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        _ => Err("invalid task side"),
    }
}

fn parse_strategy(value: &str) -> Result<StrategyKind, &'static str> {
    match value {
        "twap" => Ok(StrategyKind::Twap),
        "pov" => Ok(StrategyKind::Pov),
        _ => Err("invalid task strategy"),
    }
}

fn parse_quantity_unit(value: &str) -> Result<QuantityUnit, &'static str> {
    match value {
        "base_quantity" => Ok(QuantityUnit::BaseQuantity),
        "quote_notional" => Ok(QuantityUnit::QuoteNotional),
        "contracts" => Ok(QuantityUnit::Contracts),
        _ => Err("invalid task quantity unit"),
    }
}

#[cfg(test)]
mod tests {
    use ballast_core::{Exchange, InstrumentId, MarketKind};

    use super::*;

    #[tokio::test]
    async fn instrument_gate_counts_contended_waiters() {
        let gate = InstrumentTickGate::default();
        let metrics = AppMetrics::new().expect("metrics");
        let instrument_id = Uuid::nil();
        let first = gate.lock(instrument_id, &metrics).await;
        let waiter = tokio::spawn({
            let gate = gate.clone();
            let metrics = metrics.clone();
            async move {
                let _second = gate.lock(instrument_id, &metrics).await;
            }
        });
        tokio::task::yield_now().await;
        drop(first);
        waiter.await.expect("waiter finished");
        assert!(metrics.instrument_tick_serialized.get() >= 1);
    }

    #[test]
    fn waits_when_twap_slice_is_too_small_but_total_residual_is_executable() {
        let instrument = Instrument {
            id: InstrumentId::new(Exchange::Okx, MarketKind::Spot, "BTC/USDT").unwrap(),
            exchange_symbol: "BTC-USDT".to_owned(),
            base_asset: "BTC".to_owned(),
            quote_asset: "USDT".to_owned(),
            settle_asset: None,
            contract_kind: None,
            contract_size: None,
            price_tick: Decimal::new(1, 1),
            quantity_step: Decimal::new(1, 8),
            minimum_quantity: Some(Decimal::new(1, 5)),
            minimum_notional: None,
            maker_fee_rate: None,
            taker_fee_rate: None,
            active: true,
        };
        let reference_price = Decimal::new(64_500, 0);

        assert!(
            !is_native_quantity_executable(&instrument, Decimal::new(8, 6), reference_price,)
                .unwrap()
        );
        assert_eq!(
            task_state_for_non_executable_slice(&instrument, Decimal::new(4, 5), reference_price,)
                .unwrap(),
            "running"
        );
    }
}
