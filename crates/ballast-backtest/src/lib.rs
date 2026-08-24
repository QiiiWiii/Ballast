#![forbid(unsafe_code)]

use ballast_core::{Instrument, QuantityUnit, Side, StrategyKind};
use ballast_simulator::{BookLevel, estimate_protected_ioc_fill};
use ballast_strategies::{Pov, Strategy, StrategyTickInput, Twap};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct ReplayInput {
    pub instrument: Instrument,
    pub side: ReplaySide,
    pub strategy: StrategyKind,
    pub quantity_unit: QuantityUnit,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_amount: Decimal,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub tick_interval_seconds: u64,
    pub max_slippage_bps: u32,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub participation_rate: Option<Decimal>,
    pub samples: Vec<ReplaySample>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplaySide {
    Buy,
    Sell,
}

impl ReplaySide {
    fn side(self) -> Side {
        match self {
            Self::Buy => Side::Buy,
            Self::Sell => Side::Sell,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplaySample {
    pub at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ReplayReport {
    pub model: &'static str,
    pub strategy: StrategyKind,
    pub quantity_unit: QuantityUnit,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub executed_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub residual_amount: Decimal,
    pub status: &'static str,
    pub total_ticks: u32,
    pub filled_ticks: u32,
    pub sample_count: usize,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub average_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub worst_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub slippage_bps: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub fee_amount: Option<Decimal>,
    pub fee_asset: Option<String>,
    #[serde(with = "rust_decimal::serde::str")]
    pub filled_quote_amount: Decimal,
    pub ticks: Vec<ReplayTickReport>,
}

#[derive(Debug, Serialize)]
pub struct ReplayTickReport {
    pub sequence: u32,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub market_volume: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub requested_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub filled_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub residual_amount: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub average_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub fee_amount: Option<Decimal>,
    pub status: &'static str,
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("replay target amount must be positive")]
    NonPositiveTarget,
    #[error("replay time range is invalid")]
    InvalidTimeRange,
    #[error("replay tick interval must be positive")]
    InvalidTickInterval,
    #[error("replay duration is too long")]
    DurationOverflow,
    #[error("replay participation rate is required for POV")]
    PovRateRequired,
    #[error("replay has no valid historical samples")]
    NoSamples,
    #[error("replay sample at {at} has a non-positive price or negative quantity")]
    InvalidSample { at: DateTime<Utc> },
    #[error("strategy error: {0}")]
    Strategy(#[from] ballast_strategies::StrategyError),
    #[error("instrument conversion error: {0}")]
    Instrument(#[from] ballast_core::DomainError),
}

pub fn replay(mut input: ReplayInput) -> Result<ReplayReport, ReplayError> {
    if input.target_amount <= Decimal::ZERO {
        return Err(ReplayError::NonPositiveTarget);
    }
    if input.start_at >= input.end_at {
        return Err(ReplayError::InvalidTimeRange);
    }
    if input.tick_interval_seconds == 0 {
        return Err(ReplayError::InvalidTickInterval);
    }

    input.samples.sort_by(|left, right| left.at.cmp(&right.at));
    for sample in &input.samples {
        if sample.price <= Decimal::ZERO || sample.quantity < Decimal::ZERO {
            return Err(ReplayError::InvalidSample { at: sample.at });
        }
    }
    let range_start = input.start_at;
    let range_end = input.end_at;
    input
        .samples
        .retain(|sample| sample.at >= range_start && sample.at < range_end);
    if input.samples.is_empty() {
        return Err(ReplayError::NoSamples);
    }

    let tick_millis = input
        .tick_interval_seconds
        .checked_mul(1_000)
        .ok_or(ReplayError::DurationOverflow)?;
    let tick_millis_i64 = i64::try_from(tick_millis).map_err(|_| ReplayError::DurationOverflow)?;
    let span_millis: u64 = (input.end_at - input.start_at)
        .num_milliseconds()
        .try_into()
        .map_err(|_| ReplayError::DurationOverflow)?;
    let total_ticks = span_millis
        .checked_add(tick_millis - 1)
        .ok_or(ReplayError::DurationOverflow)?
        .checked_div(tick_millis)
        .ok_or(ReplayError::DurationOverflow)?;
    let total_ticks = u32::try_from(total_ticks).map_err(|_| ReplayError::DurationOverflow)?;

    let strategy: Box<dyn Strategy> = match input.strategy {
        StrategyKind::Twap => Box::new(Twap),
        StrategyKind::Pov => Box::new(Pov::new(
            input
                .participation_rate
                .ok_or(ReplayError::PovRateRequired)?,
        )?),
    };
    let side = input.side.side();
    let mut executed_amount = Decimal::ZERO;
    let mut filled_ticks = 0;
    let mut weighted_price = Decimal::ZERO;
    let mut filled_native = Decimal::ZERO;
    let mut worst_price = None;
    let mut fee_amount = None;
    let mut fee_asset = None;
    let mut filled_quote_amount = Decimal::ZERO;
    let mut ticks = Vec::with_capacity(total_ticks as usize);

    for sequence in 0..total_ticks {
        let offset_millis = i64::from(sequence)
            .checked_mul(tick_millis_i64)
            .ok_or(ReplayError::DurationOverflow)?;
        let start_at = input
            .start_at
            .checked_add_signed(Duration::milliseconds(offset_millis))
            .ok_or(ReplayError::DurationOverflow)?;
        let end_at = start_at
            .checked_add_signed(Duration::milliseconds(tick_millis_i64))
            .unwrap_or(input.end_at)
            .min(input.end_at);
        let samples: Vec<&ReplaySample> = input
            .samples
            .iter()
            .filter(|sample| sample.at >= start_at && sample.at < end_at)
            .collect();
        let market_volume = samples.iter().try_fold(Decimal::ZERO, |total, sample| {
            Ok::<_, ReplayError>(
                total
                    + native_to_target(
                        &input.instrument,
                        sample.quantity,
                        sample.price,
                        input.quantity_unit,
                    )?,
            )
        })?;
        let remaining_before = (input.target_amount - executed_amount).max(Decimal::ZERO);
        let requested_amount = if remaining_before.is_zero() || samples.is_empty() {
            Decimal::ZERO
        } else {
            strategy.next_slice_quantity(StrategyTickInput {
                target_amount: input.target_amount,
                executed_amount,
                market_volume,
                elapsed_ticks: sequence,
                total_ticks,
            })?
        };
        let (
            filled_amount,
            filled_native_quantity,
            average_price,
            fill_worst_price,
            tick_fee,
            tick_fee_asset,
            tick_status,
        ) = if requested_amount.is_zero() {
            (
                Decimal::ZERO,
                Decimal::ZERO,
                None,
                None,
                None,
                None,
                if remaining_before.is_zero() {
                    "complete"
                } else {
                    "no_data"
                },
            )
        } else {
            let reference_price = samples[0].price;
            let native_quantity = input.instrument.target_to_native_quantity(
                requested_amount,
                input.quantity_unit,
                reference_price,
            )?;
            let levels: Vec<BookLevel> = samples
                .iter()
                .map(|sample| BookLevel {
                    price: sample.price,
                    quantity: sample.quantity,
                })
                .collect();
            let fill = estimate_protected_ioc_fill(
                &input.instrument,
                side,
                native_quantity,
                input.max_slippage_bps,
                &levels,
                &levels,
            )
            .map_err(ReplayError::Instrument)?;
            let filled_amount = match fill.average_price {
                Some(price) => native_to_target(
                    &input.instrument,
                    fill.filled_native_quantity,
                    price,
                    input.quantity_unit,
                )?
                .min(requested_amount),
                None => Decimal::ZERO,
            };
            let status = if filled_amount.is_zero() {
                "unfilled"
            } else if filled_amount < requested_amount {
                "partial"
            } else {
                "filled"
            };
            (
                filled_amount,
                fill.filled_native_quantity,
                fill.average_price,
                fill.worst_price,
                fill.fee_amount,
                fill.fee_asset,
                status,
            )
        };
        executed_amount += filled_amount;
        if let Some(price) = average_price {
            filled_native += filled_native_quantity;
            weighted_price += filled_native_quantity * price;
            if let Some(level_price) = fill_worst_price {
                worst_price = match (worst_price, side) {
                    (None, _) => Some(level_price),
                    (Some(current), Side::Buy) => Some(current.max(level_price)),
                    (Some(current), Side::Sell) => Some(current.min(level_price)),
                };
            }
            filled_quote_amount += input
                .instrument
                .native_to_quote_quantity(filled_native_quantity, price)?;
            if let Some(value) = tick_fee {
                fee_amount = Some(fee_amount.unwrap_or(Decimal::ZERO) + value);
            }
            if fee_asset.is_none() {
                fee_asset = tick_fee_asset;
            }
            filled_ticks += 1;
        }
        ticks.push(ReplayTickReport {
            sequence: sequence + 1,
            start_at,
            end_at,
            market_volume,
            requested_amount,
            filled_amount,
            residual_amount: (input.target_amount - executed_amount).max(Decimal::ZERO),
            average_price,
            fee_amount: tick_fee,
            status: tick_status,
        });
    }

    let average_price = if filled_native.is_zero() {
        None
    } else {
        Some(weighted_price / filled_native)
    };
    let first_price = input
        .samples
        .iter()
        .find(|sample| sample.at >= input.start_at && sample.at < input.end_at)
        .map(|sample| sample.price);
    let slippage_bps = average_price
        .zip(first_price)
        .map(|(average, reference)| match side {
            Side::Buy => (average / reference - Decimal::ONE) * Decimal::from(10_000),
            Side::Sell => (Decimal::ONE - average / reference) * Decimal::from(10_000),
        });
    let residual_amount = (input.target_amount - executed_amount).max(Decimal::ZERO);
    Ok(ReplayReport {
        model: "historical_volume_as_liquidity",
        strategy: input.strategy,
        quantity_unit: input.quantity_unit,
        target_amount: input.target_amount,
        executed_amount,
        residual_amount,
        status: if residual_amount.is_zero() {
            "completed"
        } else {
            "partial"
        },
        total_ticks,
        filled_ticks,
        sample_count: input.samples.len(),
        average_price,
        worst_price,
        slippage_bps,
        fee_amount,
        fee_asset,
        filled_quote_amount,
        ticks,
    })
}

fn native_to_target(
    instrument: &Instrument,
    native_quantity: Decimal,
    price: Decimal,
    unit: QuantityUnit,
) -> Result<Decimal, ReplayError> {
    match unit {
        QuantityUnit::BaseQuantity => instrument
            .native_to_base_quantity(native_quantity, price)
            .map_err(ReplayError::Instrument),
        QuantityUnit::QuoteNotional => instrument
            .native_to_quote_quantity(native_quantity, price)
            .map_err(ReplayError::Instrument),
        QuantityUnit::Contracts => Ok(native_quantity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ballast_core::{ContractKind, Exchange, InstrumentId, MarketKind};

    fn instrument() -> Instrument {
        Instrument {
            id: InstrumentId::new(Exchange::Okx, MarketKind::Spot, "BTC/USDT").unwrap(),
            exchange_symbol: "BTC-USDT".to_owned(),
            base_asset: "BTC".to_owned(),
            quote_asset: "USDT".to_owned(),
            settle_asset: None,
            contract_kind: None,
            contract_size: None,
            price_tick: Decimal::new(1, 2),
            quantity_step: Decimal::new(1, 3),
            minimum_quantity: None,
            minimum_notional: None,
            maker_fee_rate: None,
            taker_fee_rate: Some(Decimal::new(1, 3)),
            active: true,
        }
    }

    fn input(strategy: StrategyKind) -> ReplayInput {
        let start_at = DateTime::parse_from_rfc3339("2026-08-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        ReplayInput {
            instrument: instrument(),
            side: ReplaySide::Buy,
            strategy,
            quantity_unit: QuantityUnit::BaseQuantity,
            target_amount: Decimal::new(3, 0),
            start_at,
            end_at: start_at + Duration::seconds(3),
            tick_interval_seconds: 1,
            max_slippage_bps: 100,
            participation_rate: (strategy == StrategyKind::Pov).then_some(Decimal::new(5, 1)),
            samples: (0..3)
                .map(|index| ReplaySample {
                    at: start_at + Duration::seconds(index),
                    price: Decimal::new(100 + index, 0),
                    quantity: Decimal::ONE,
                })
                .collect(),
        }
    }

    #[test]
    fn twap_replays_historical_liquidity() {
        let report = replay(input(StrategyKind::Twap)).unwrap();
        assert_eq!(report.status, "completed");
        assert_eq!(report.executed_amount, Decimal::new(3, 0));
        assert_eq!(report.filled_ticks, 3);
        assert_eq!(report.worst_price, Some(Decimal::new(102, 0)));
        assert_eq!(report.fee_amount, Some(Decimal::new(303, 3)));
        assert_eq!(report.fee_asset.as_deref(), Some("USDT"));
    }

    #[test]
    fn pov_replays_participation_rate() {
        let report = replay(input(StrategyKind::Pov)).unwrap();
        assert_eq!(report.status, "partial");
        assert_eq!(report.executed_amount, Decimal::new(15, 1));
    }

    #[test]
    fn rejects_inverse_without_contract_metadata() {
        let mut replay_input = input(StrategyKind::Twap);
        replay_input.instrument.id.market_kind = MarketKind::Perpetual;
        replay_input.instrument.contract_kind = Some(ContractKind::Inverse);
        replay_input.instrument.contract_size = None;
        assert!(matches!(
            replay(replay_input),
            Err(ReplayError::Instrument(_))
        ));
    }

    #[test]
    fn rejects_samples_outside_requested_range() {
        let mut replay_input = input(StrategyKind::Twap);
        replay_input.samples = vec![ReplaySample {
            at: replay_input.end_at,
            price: Decimal::new(100, 0),
            quantity: Decimal::ONE,
        }];
        assert!(matches!(replay(replay_input), Err(ReplayError::NoSamples)));
    }

    #[test]
    fn accepts_zero_volume_candles_as_unfilled_ticks() {
        let mut replay_input = input(StrategyKind::Twap);
        replay_input.samples.iter_mut().for_each(|sample| {
            sample.quantity = Decimal::ZERO;
        });
        let report = replay(replay_input).unwrap();
        assert_eq!(report.status, "partial");
        assert_eq!(report.executed_amount, Decimal::ZERO);
        assert_eq!(report.filled_ticks, 0);
    }
}
