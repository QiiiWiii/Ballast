#![forbid(unsafe_code)]

use ballast_core::{ContractKind, DomainError, Instrument, Side};
use ballast_core::{QuantityUnit, StrategyKind};
use ballast_strategies::{Pov, Strategy, StrategyError, StrategyTickInput, Twap};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use thiserror::Error;

const BPS_DENOMINATOR: Decimal = Decimal::from_parts(10_000, 0, 0, false, 0);

pub const TRADE_VWAP_PROXY_VERSION: &str = "trade_vwap_proxy/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayTrade {
    pub exchange_trade_id: String,
    pub trade_time: DateTime<Utc>,
    pub price: Decimal,
    pub native_quantity: Decimal,
}

#[derive(Debug, Clone)]
pub struct ReplayConfig {
    pub instrument: Instrument,
    pub strategy_kind: StrategyKind,
    pub side: Side,
    pub requested_amount: Decimal,
    pub quantity_unit: QuantityUnit,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub slice_interval: Duration,
    pub participation_rate: Option<Decimal>,
    pub max_slice_amount: Option<Decimal>,
    pub fee_rate: Decimal,
    pub extra_slippage_bps: Decimal,
    pub gap_threshold: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaySliceResult {
    pub sequence: i32,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub status: String,
    pub trade_count: i64,
    pub market_volume: Decimal,
    pub requested_amount: Decimal,
    pub filled_amount: Decimal,
    pub filled_native_quantity: Decimal,
    pub market_vwap: Option<Decimal>,
    pub simulated_price: Option<Decimal>,
    pub fee_amount: Decimal,
    pub decision_input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayMetrics {
    pub arrival_price: Option<Decimal>,
    pub market_vwap: Option<Decimal>,
    pub simulated_execution_vwap: Option<Decimal>,
    pub implementation_shortfall_bps: Option<Decimal>,
    pub implementation_shortfall_amount: Option<Decimal>,
    pub requested_amount: Decimal,
    pub filled_amount: Decimal,
    pub fill_rate: Decimal,
    pub residual_amount: Decimal,
    pub actual_participation_rate: Option<Decimal>,
    pub target_participation_rate: Option<Decimal>,
    pub participation_rate_deviation: Option<Decimal>,
    pub slice_count: i32,
    pub empty_window_count: i32,
    pub fee_amount: Decimal,
    pub explicit_slippage_amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayGap {
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayResult {
    pub status: String,
    pub failure_code: Option<String>,
    pub data_first_at: Option<DateTime<Utc>>,
    pub data_last_at: Option<DateTime<Utc>>,
    pub trade_count: i64,
    pub gap_count: i32,
    pub gaps: Vec<ReplayGap>,
    pub confidence: String,
    pub limitations: Value,
    pub slices: Vec<ReplaySliceResult>,
    pub metrics: ReplayMetrics,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("invalid replay time range or interval")]
    InvalidTimeRange,
    #[error("requested amount must be positive")]
    InvalidRequestedAmount,
    #[error("cost assumptions must be non-negative")]
    InvalidCostAssumption,
    #[error("historical trades are not strictly ordered")]
    TradesNotOrdered,
    #[error("strategy configuration is invalid: {0}")]
    Strategy(#[from] StrategyError),
    #[error("instrument quantity conversion failed: {0}")]
    Instrument(#[from] DomainError),
}

pub fn replay_trade_vwap_proxy(
    config: &ReplayConfig,
    trades: &[ReplayTrade],
) -> Result<ReplayResult, ReplayError> {
    validate_replay_config(config)?;
    validate_trade_order(trades)?;
    let relevant: Vec<_> = trades
        .iter()
        .filter(|trade| trade.trade_time >= config.start_at && trade.trade_time < config.end_at)
        .collect();
    let first_at = relevant.first().map(|trade| trade.trade_time);
    let last_at = relevant.last().map(|trade| trade.trade_time);
    if relevant.is_empty() {
        return Ok(empty_replay_result(config));
    }

    let duration_ms = (config.end_at - config.start_at).num_milliseconds();
    let interval_ms = config.slice_interval.num_milliseconds();
    let total_ticks = u32::try_from((duration_ms + interval_ms - 1) / interval_ms)
        .map_err(|_| ReplayError::InvalidTimeRange)?;
    let arrival_price = relevant[0].price;
    let mut slices = Vec::with_capacity(total_ticks as usize);
    let mut executed_amount = Decimal::ZERO;
    let mut executed_native = Decimal::ZERO;
    let mut execution_quote = Decimal::ZERO;
    let mut total_fees = Decimal::ZERO;
    let mut explicit_slippage_amount = Decimal::ZERO;
    let mut empty_windows = 0_i32;
    let mut trade_index = 0_usize;

    for tick in 0..total_ticks {
        let window_start = config.start_at + config.slice_interval * i32::try_from(tick).unwrap();
        let window_end = (window_start + config.slice_interval).min(config.end_at);
        let begin = trade_index;
        while trade_index < relevant.len() && relevant[trade_index].trade_time < window_end {
            trade_index += 1;
        }
        let window = &relevant[begin..trade_index];
        let market_native = window
            .iter()
            .fold(Decimal::ZERO, |total, trade| total + trade.native_quantity);
        let market_vwap = weighted_vwap(window);
        let market_volume = window.iter().try_fold(Decimal::ZERO, |total, trade| {
            Ok::<_, DomainError>(
                total
                    + native_to_unit(
                        &config.instrument,
                        config.quantity_unit,
                        trade.native_quantity,
                        trade.price,
                    )?,
            )
        })?;
        let remaining = (config.requested_amount - executed_amount).max(Decimal::ZERO);
        let mut desired = strategy_quantity(
            config,
            StrategyTickInput {
                target_amount: config.requested_amount,
                executed_amount,
                market_volume,
                elapsed_ticks: tick,
                total_ticks,
            },
        )?;
        if let Some(maximum) = config.max_slice_amount {
            desired = desired.min(maximum);
        }
        desired = desired.min(remaining).max(Decimal::ZERO);

        let (status, filled_amount, filled_native, simulated_price, fee_amount) =
            if window.is_empty() {
                ("empty", Decimal::ZERO, Decimal::ZERO, None, Decimal::ZERO)
            } else if desired.is_zero() {
                ("skipped", Decimal::ZERO, Decimal::ZERO, None, Decimal::ZERO)
            } else if let Some(market_price) = market_vwap {
                let simulated_price =
                    apply_slippage(market_price, config.side, config.extra_slippage_bps);
                let desired_native = config.instrument.target_to_native_quantity(
                    desired,
                    config.quantity_unit,
                    simulated_price,
                )?;
                let filled_native = desired_native.min(market_native);
                let filled_amount = native_to_unit(
                    &config.instrument,
                    config.quantity_unit,
                    filled_native,
                    simulated_price,
                )?
                .min(remaining);
                let filled_quote = config
                    .instrument
                    .native_to_quote_quantity(filled_native, simulated_price)?;
                let fee = filled_quote * config.fee_rate;
                let status = if filled_amount.is_zero() {
                    "empty"
                } else if filled_amount < desired {
                    "partial"
                } else {
                    "filled"
                };
                explicit_slippage_amount += config
                    .instrument
                    .native_to_base_quantity(filled_native, simulated_price)?
                    * (simulated_price - market_price).abs();
                (
                    status,
                    filled_amount,
                    filled_native,
                    Some(simulated_price),
                    fee,
                )
            } else {
                ("empty", Decimal::ZERO, Decimal::ZERO, None, Decimal::ZERO)
            };
        if window.is_empty() {
            empty_windows += 1;
        }
        executed_amount += filled_amount;
        executed_native += filled_native;
        if let Some(price) = simulated_price {
            execution_quote += filled_native * price;
        }
        total_fees += fee_amount;
        slices.push(ReplaySliceResult {
            sequence: i32::try_from(tick + 1).unwrap(),
            window_start,
            window_end,
            status: status.to_owned(),
            trade_count: i64::try_from(window.len()).unwrap_or(i64::MAX),
            market_volume,
            requested_amount: desired,
            filled_amount,
            filled_native_quantity: filled_native,
            market_vwap,
            simulated_price,
            fee_amount,
            decision_input: json!({
                "elapsed_ticks": tick,
                "total_ticks": total_ticks,
                "remaining_amount": remaining.to_string(),
                "market_native_quantity": market_native.to_string(),
                "model": TRADE_VWAP_PROXY_VERSION,
            }),
        });
    }

    let market_vwap = weighted_vwap_refs(&relevant);
    let execution_vwap = (!executed_native.is_zero()).then_some(execution_quote / executed_native);
    let filled_base = match execution_vwap {
        Some(price) => config
            .instrument
            .native_to_base_quantity(executed_native, price)?,
        None => Decimal::ZERO,
    };
    let price_shortfall = execution_vwap.map(|price| match config.side {
        Side::Buy => (price - arrival_price) * filled_base,
        Side::Sell => (arrival_price - price) * filled_base,
    });
    let shortfall_amount = price_shortfall.map(|value| value + total_fees);
    let arrival_notional = arrival_price * filled_base;
    let shortfall_bps = shortfall_amount
        .filter(|_| !arrival_notional.is_zero())
        .map(|value| value / arrival_notional * BPS_DENOMINATOR);
    let total_market_amount = relevant.iter().try_fold(Decimal::ZERO, |total, trade| {
        Ok::<_, DomainError>(
            total
                + native_to_unit(
                    &config.instrument,
                    config.quantity_unit,
                    trade.native_quantity,
                    trade.price,
                )?,
        )
    })?;
    let actual_participation =
        (!total_market_amount.is_zero()).then_some(executed_amount / total_market_amount);
    let target_participation = (config.strategy_kind == StrategyKind::Pov)
        .then_some(config.participation_rate)
        .flatten();
    let gaps = collect_gaps(config, &relevant);
    let gap_count = i32::try_from(gaps.len()).unwrap_or(i32::MAX);
    let residual = (config.requested_amount - executed_amount).max(Decimal::ZERO);
    let warnings = gap_count > 0 || empty_windows > 0 || !residual.is_zero();

    Ok(ReplayResult {
        status: if warnings {
            "completed_with_warnings"
        } else {
            "completed"
        }
        .to_owned(),
        failure_code: None,
        data_first_at: first_at,
        data_last_at: last_at,
        trade_count: i64::try_from(relevant.len()).unwrap_or(i64::MAX),
        gap_count,
        gaps,
        confidence: if warnings { "limited" } else { "high" }.to_owned(),
        limitations: json!([
            "trade_vwap_proxy uses historical prints and is not an L2 order-book matching simulation",
            "queue position and market impact are not modelled",
            "historical trade quantity is interpreted as gateway-native quantity"
        ]),
        metrics: ReplayMetrics {
            arrival_price: Some(arrival_price),
            market_vwap,
            simulated_execution_vwap: execution_vwap,
            implementation_shortfall_bps: shortfall_bps,
            implementation_shortfall_amount: shortfall_amount,
            requested_amount: config.requested_amount,
            filled_amount: executed_amount,
            fill_rate: executed_amount / config.requested_amount,
            residual_amount: residual,
            actual_participation_rate: actual_participation,
            target_participation_rate: target_participation,
            participation_rate_deviation: actual_participation
                .zip(target_participation)
                .map(|(actual, target)| actual - target),
            slice_count: i32::try_from(slices.len()).unwrap_or(i32::MAX),
            empty_window_count: empty_windows,
            fee_amount: total_fees,
            explicit_slippage_amount,
        },
        slices,
    })
}

fn validate_replay_config(config: &ReplayConfig) -> Result<(), ReplayError> {
    if config.start_at >= config.end_at
        || config.slice_interval <= Duration::zero()
        || config.gap_threshold <= Duration::zero()
    {
        return Err(ReplayError::InvalidTimeRange);
    }
    if config.requested_amount <= Decimal::ZERO {
        return Err(ReplayError::InvalidRequestedAmount);
    }
    if config.fee_rate < Decimal::ZERO
        || config.extra_slippage_bps < Decimal::ZERO
        || config.extra_slippage_bps >= BPS_DENOMINATOR
    {
        return Err(ReplayError::InvalidCostAssumption);
    }
    if config.strategy_kind == StrategyKind::Pov {
        Pov::new(
            config
                .participation_rate
                .ok_or(StrategyError::InvalidParticipationRate)?,
        )?;
    }
    Ok(())
}

fn validate_trade_order(trades: &[ReplayTrade]) -> Result<(), ReplayError> {
    if trades.windows(2).any(|pair| {
        (pair[0].trade_time, pair[0].exchange_trade_id.as_str())
            >= (pair[1].trade_time, pair[1].exchange_trade_id.as_str())
    }) {
        return Err(ReplayError::TradesNotOrdered);
    }
    Ok(())
}

fn strategy_quantity(
    config: &ReplayConfig,
    input: StrategyTickInput,
) -> Result<Decimal, ReplayError> {
    match config.strategy_kind {
        StrategyKind::Twap => Ok(Twap.next_slice_quantity(input)?),
        StrategyKind::Pov => Ok(Pov::new(
            config
                .participation_rate
                .ok_or(StrategyError::InvalidParticipationRate)?,
        )?
        .next_slice_quantity(input)?),
    }
}

fn apply_slippage(price: Decimal, side: Side, bps: Decimal) -> Decimal {
    let ratio = bps / BPS_DENOMINATOR;
    match side {
        Side::Buy => price * (Decimal::ONE + ratio),
        Side::Sell => price * (Decimal::ONE - ratio),
    }
}

fn native_to_unit(
    instrument: &Instrument,
    unit: QuantityUnit,
    native_quantity: Decimal,
    price: Decimal,
) -> Result<Decimal, DomainError> {
    match unit {
        QuantityUnit::Contracts => Ok(native_quantity),
        QuantityUnit::BaseQuantity => instrument.native_to_base_quantity(native_quantity, price),
        QuantityUnit::QuoteNotional => instrument.native_to_quote_quantity(native_quantity, price),
    }
}

fn weighted_vwap(trades: &[&ReplayTrade]) -> Option<Decimal> {
    let total = trades
        .iter()
        .fold(Decimal::ZERO, |sum, trade| sum + trade.native_quantity);
    (!total.is_zero()).then(|| {
        trades.iter().fold(Decimal::ZERO, |sum, trade| {
            sum + trade.price * trade.native_quantity
        }) / total
    })
}

fn weighted_vwap_refs(trades: &[&ReplayTrade]) -> Option<Decimal> {
    weighted_vwap(trades)
}

fn collect_gaps(config: &ReplayConfig, trades: &[&ReplayTrade]) -> Vec<ReplayGap> {
    let mut gaps = Vec::new();
    if trades[0].trade_time - config.start_at > config.gap_threshold {
        gaps.push(ReplayGap {
            start_at: config.start_at,
            end_at: trades[0].trade_time,
        });
    }
    gaps.extend(trades.windows(2).filter_map(|pair| {
        (pair[1].trade_time - pair[0].trade_time > config.gap_threshold).then_some(ReplayGap {
            start_at: pair[0].trade_time,
            end_at: pair[1].trade_time,
        })
    }));
    if config.end_at - trades[trades.len() - 1].trade_time > config.gap_threshold {
        gaps.push(ReplayGap {
            start_at: trades[trades.len() - 1].trade_time,
            end_at: config.end_at,
        });
    }
    gaps
}

fn empty_replay_result(config: &ReplayConfig) -> ReplayResult {
    ReplayResult {
        status: "failed".to_owned(),
        failure_code: Some("historical_trades_not_found".to_owned()),
        data_first_at: None,
        data_last_at: None,
        trade_count: 0,
        gap_count: 1,
        gaps: vec![ReplayGap {
            start_at: config.start_at,
            end_at: config.end_at,
        }],
        confidence: "unusable".to_owned(),
        limitations: json!(["No historical trades exist in the requested interval"]),
        slices: Vec::new(),
        metrics: ReplayMetrics {
            arrival_price: None,
            market_vwap: None,
            simulated_execution_vwap: None,
            implementation_shortfall_bps: None,
            implementation_shortfall_amount: None,
            requested_amount: config.requested_amount,
            filled_amount: Decimal::ZERO,
            fill_rate: Decimal::ZERO,
            residual_amount: config.requested_amount,
            actual_participation_rate: None,
            target_participation_rate: config.participation_rate,
            participation_rate_deviation: None,
            slice_count: 0,
            empty_window_count: 0,
            fee_amount: Decimal::ZERO,
            explicit_slippage_amount: Decimal::ZERO,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillEstimate {
    pub filled_native_quantity: Decimal,
    pub unfilled_native_quantity: Decimal,
    pub filled_base_quantity: Decimal,
    pub filled_quote_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub worst_price: Option<Decimal>,
    pub slippage_bps: Option<Decimal>,
    pub fee_amount: Option<Decimal>,
    pub fee_asset: Option<String>,
}

pub fn estimate_protected_ioc_fill(
    instrument: &Instrument,
    side: Side,
    native_quantity: Decimal,
    max_slippage_bps: u32,
    bids: &[BookLevel],
    asks: &[BookLevel],
) -> Result<FillEstimate, DomainError> {
    let levels = match side {
        Side::Buy => asks,
        Side::Sell => bids,
    };
    let reference_price = levels
        .first()
        .map(|level| level.price)
        .filter(|price| *price > Decimal::ZERO)
        .ok_or(DomainError::NonPositivePrice)?;
    let slippage_ratio = Decimal::from(max_slippage_bps) / BPS_DENOMINATOR;
    let protected_price = match side {
        Side::Buy => reference_price * (Decimal::ONE + slippage_ratio),
        Side::Sell => reference_price * (Decimal::ONE - slippage_ratio),
    };
    let mut remaining = native_quantity.max(Decimal::ZERO);
    let mut native_quote_cost = Decimal::ZERO;
    let mut worst_price = None;

    for level in levels {
        if remaining.is_zero() || !within_price_protection(side, level.price, protected_price) {
            break;
        }
        let fill_quantity = remaining.min(level.quantity.max(Decimal::ZERO));
        if fill_quantity.is_zero() {
            continue;
        }
        native_quote_cost += fill_quantity * level.price;
        remaining -= fill_quantity;
        worst_price = Some(level.price);
    }

    let filled_native_quantity = native_quantity.max(Decimal::ZERO) - remaining;
    let average_price = if filled_native_quantity.is_zero() {
        None
    } else {
        Some(native_quote_cost / filled_native_quantity)
    };
    let (filled_base_quantity, filled_quote_quantity) = match average_price {
        Some(price) => (
            instrument.native_to_base_quantity(filled_native_quantity, price)?,
            instrument.native_to_quote_quantity(filled_native_quantity, price)?,
        ),
        None => (Decimal::ZERO, Decimal::ZERO),
    };
    let slippage_bps = average_price.map(|price| match side {
        Side::Buy => (price / reference_price - Decimal::ONE) * BPS_DENOMINATOR,
        Side::Sell => (Decimal::ONE - price / reference_price) * BPS_DENOMINATOR,
    });
    let (fee_amount, fee_asset) =
        estimate_fee(instrument, filled_base_quantity, filled_quote_quantity);

    Ok(FillEstimate {
        filled_native_quantity,
        unfilled_native_quantity: remaining,
        filled_base_quantity,
        filled_quote_quantity,
        average_price,
        worst_price,
        slippage_bps,
        fee_amount,
        fee_asset,
    })
}

fn within_price_protection(side: Side, price: Decimal, protected_price: Decimal) -> bool {
    match side {
        Side::Buy => price <= protected_price,
        Side::Sell => price >= protected_price,
    }
}

fn estimate_fee(
    instrument: &Instrument,
    base_quantity: Decimal,
    quote_quantity: Decimal,
) -> (Option<Decimal>, Option<String>) {
    let Some(rate) = instrument.taker_fee_rate else {
        return (None, None);
    };
    if instrument.contract_kind == Some(ContractKind::Inverse) {
        (
            Some(base_quantity * rate),
            instrument
                .settle_asset
                .clone()
                .or_else(|| Some(instrument.base_asset.clone())),
        )
    } else {
        (
            Some(quote_quantity * rate),
            Some(instrument.quote_asset.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use ballast_core::{Exchange, InstrumentId, MarketKind};

    use super::*;

    fn spot_instrument() -> Instrument {
        Instrument {
            id: InstrumentId::new(Exchange::Binance, MarketKind::Spot, "BTC/USDT").unwrap(),
            exchange_symbol: "BTCUSDT".to_owned(),
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
        }
    }

    #[test]
    fn buy_stops_at_price_protection() {
        let asks = [
            BookLevel {
                price: Decimal::new(100, 0),
                quantity: Decimal::ONE,
            },
            BookLevel {
                price: Decimal::new(101, 0),
                quantity: Decimal::ONE,
            },
            BookLevel {
                price: Decimal::new(103, 0),
                quantity: Decimal::ONE,
            },
        ];
        let fill = estimate_protected_ioc_fill(
            &spot_instrument(),
            Side::Buy,
            Decimal::new(3, 0),
            200,
            &[],
            &asks,
        )
        .unwrap();

        assert_eq!(fill.filled_native_quantity, Decimal::new(2, 0));
        assert_eq!(fill.unfilled_native_quantity, Decimal::ONE);
        assert_eq!(fill.average_price, Some(Decimal::new(1005, 1)));
        assert_eq!(fill.fee_asset.as_deref(), Some("USDT"));
    }

    #[test]
    fn twap_replay_is_deterministic_and_applies_costs() {
        let start = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let trades = vec![
            ReplayTrade {
                exchange_trade_id: "1".to_owned(),
                trade_time: start + Duration::seconds(5),
                price: Decimal::new(100, 0),
                native_quantity: Decimal::new(5, 0),
            },
            ReplayTrade {
                exchange_trade_id: "2".to_owned(),
                trade_time: start + Duration::seconds(35),
                price: Decimal::new(102, 0),
                native_quantity: Decimal::new(5, 0),
            },
        ];
        let config = ReplayConfig {
            instrument: spot_instrument(),
            strategy_kind: StrategyKind::Twap,
            side: Side::Buy,
            requested_amount: Decimal::new(2, 0),
            quantity_unit: QuantityUnit::BaseQuantity,
            start_at: start,
            end_at: start + Duration::minutes(1),
            slice_interval: Duration::seconds(30),
            participation_rate: None,
            max_slice_amount: None,
            fee_rate: Decimal::new(1, 3),
            extra_slippage_bps: Decimal::new(10, 0),
            gap_threshold: Duration::seconds(40),
        };

        let first = replay_trade_vwap_proxy(&config, &trades).unwrap();
        let second = replay_trade_vwap_proxy(&config, &trades).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.metrics.filled_amount, Decimal::new(2, 0));
        assert_eq!(first.metrics.slice_count, 2);
        assert!(first.metrics.fee_amount > Decimal::ZERO);
        assert!(first.metrics.implementation_shortfall_bps.unwrap() > Decimal::ZERO);
    }

    #[test]
    fn pov_replay_caps_quantity_by_participation() {
        let start = Utc::now();
        let trades = vec![ReplayTrade {
            exchange_trade_id: "trade".to_owned(),
            trade_time: start + Duration::seconds(1),
            price: Decimal::new(100, 0),
            native_quantity: Decimal::new(10, 0),
        }];
        let result = replay_trade_vwap_proxy(
            &ReplayConfig {
                instrument: spot_instrument(),
                strategy_kind: StrategyKind::Pov,
                side: Side::Sell,
                requested_amount: Decimal::new(10, 0),
                quantity_unit: QuantityUnit::BaseQuantity,
                start_at: start,
                end_at: start + Duration::seconds(10),
                slice_interval: Duration::seconds(10),
                participation_rate: Some(Decimal::new(2, 1)),
                max_slice_amount: None,
                fee_rate: Decimal::ZERO,
                extra_slippage_bps: Decimal::ZERO,
                gap_threshold: Duration::seconds(20),
            },
            &trades,
        )
        .unwrap();

        assert_eq!(result.metrics.filled_amount, Decimal::new(2, 0));
        assert_eq!(
            result.metrics.actual_participation_rate,
            Some(Decimal::new(2, 1))
        );
        assert_eq!(result.metrics.residual_amount, Decimal::new(8, 0));
    }

    #[test]
    fn replay_records_no_data_as_auditable_failure() {
        let start = Utc::now();
        let result = replay_trade_vwap_proxy(
            &ReplayConfig {
                instrument: spot_instrument(),
                strategy_kind: StrategyKind::Twap,
                side: Side::Buy,
                requested_amount: Decimal::ONE,
                quantity_unit: QuantityUnit::BaseQuantity,
                start_at: start,
                end_at: start + Duration::minutes(1),
                slice_interval: Duration::seconds(10),
                participation_rate: None,
                max_slice_amount: None,
                fee_rate: Decimal::ZERO,
                extra_slippage_bps: Decimal::ZERO,
                gap_threshold: Duration::seconds(20),
            },
            &[],
        )
        .unwrap();

        assert_eq!(result.status, "failed");
        assert_eq!(
            result.failure_code.as_deref(),
            Some("historical_trades_not_found")
        );
        assert_eq!(result.confidence, "unusable");
    }

    #[test]
    fn replay_records_empty_windows_and_exact_gap_ranges() {
        let start = Utc::now();
        let trades = vec![ReplayTrade {
            exchange_trade_id: "only".to_owned(),
            trade_time: start + Duration::seconds(1),
            price: Decimal::new(100, 0),
            native_quantity: Decimal::new(10, 0),
        }];
        let result = replay_trade_vwap_proxy(
            &ReplayConfig {
                instrument: spot_instrument(),
                strategy_kind: StrategyKind::Twap,
                side: Side::Buy,
                requested_amount: Decimal::new(2, 0),
                quantity_unit: QuantityUnit::BaseQuantity,
                start_at: start,
                end_at: start + Duration::minutes(1),
                slice_interval: Duration::seconds(30),
                participation_rate: None,
                max_slice_amount: None,
                fee_rate: Decimal::ZERO,
                extra_slippage_bps: Decimal::ZERO,
                gap_threshold: Duration::seconds(20),
            },
            &trades,
        )
        .unwrap();

        assert_eq!(result.metrics.empty_window_count, 1);
        assert_eq!(result.gap_count, 1);
        assert_eq!(result.gaps[0].start_at, trades[0].trade_time);
        assert_eq!(result.gaps[0].end_at, start + Duration::minutes(1));
        assert_eq!(result.status, "completed_with_warnings");
    }
}
