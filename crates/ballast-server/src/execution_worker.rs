use std::{
    collections::{HashMap, HashSet, VecDeque},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use ballast_core::{Instrument, QuantityUnit, Side, StrategyKind};
use ballast_execution::{ManagedSliceInput, calculate_managed_slice, protected_conversion_price};
use ballast_gateway_client::{GatewayClient, proto};
use ballast_simulator::{BookLevel, estimate_protected_ioc_fill};
use ballast_storage::{DatabasePool, SliceRecord, StoredExecutionTask};
use chrono::{DateTime, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::metrics::AppMetrics;

#[derive(Clone, Default)]
pub struct TradeVolumeTracker {
    entries: Arc<Mutex<HashMap<Uuid, Arc<TradeEntry>>>>,
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

pub fn spawn_worker(
    database: DatabasePool,
    gateway: GatewayClient,
    trade_volume: TradeVolumeTracker,
    metrics: AppMetrics,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let lease_until = Utc::now() + chrono::Duration::seconds(15);
            match ballast_storage::claim_runnable_tasks(&database, 20, lease_until).await {
                Ok(tasks) => {
                    metrics.worker_batch_size.set(tasks.len() as i64);
                    metrics.claimed_tasks.inc_by(tasks.len() as u64);
                    for task in tasks {
                        if let Err(error) =
                            process_task(&database, &gateway, &trade_volume, task, &metrics).await
                        {
                            error!(%error, "paper execution tick failed");
                        }
                    }
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
    let side = parse_side(&task.side)?;
    let unit = parse_quantity_unit(&task.quantity_unit)?;
    let strategy_kind = parse_strategy(&task.strategy_kind)?;
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

    let order_book = match gateway.get_order_book(&instrument.id, 50).await {
        Ok(value) => value,
        Err(error) => {
            warn!(%error, task_id = %task.id, "pausing task because order book is unavailable");
            pause_task(database, &task, "order_book_unavailable", metrics).await?;
            return Ok(());
        }
    };
    let age_ms = Utc::now().timestamp_millis() - order_book.gateway_received_at_ms;
    if age_ms > 5_000 {
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
