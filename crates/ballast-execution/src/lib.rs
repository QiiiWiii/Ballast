#![forbid(unsafe_code)]

use async_trait::async_trait;
use ballast_core::{ExecutionIntent, InstrumentId, QuantityUnit, Side, StrategyKind};
use ballast_strategies::{Pov, Strategy, StrategyError, StrategyTickInput, Twap};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;
mod order;

pub use order::{
    ExecutionPort, OrderLifecycle, OrderSnapshot, OrderState, OrderStateParseError,
    OrderTransitionError, SubmitOrder, stable_client_order_id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionBackend {
    ManagedIoc,
    VenueNativeAlgo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    PendingApproval,
    Scheduled,
    Running,
    Paused,
    Cancelling,
    Completed,
    Cancelled,
    Expired,
    Failed,
    Rejected,
}

#[derive(Debug)]
pub struct ExecutionTask {
    pub intent: ExecutionIntent,
    pub backend: ExecutionBackend,
    pub state: ExecutionState,
    pub executed_quantity: Decimal,
}

impl ExecutionTask {
    #[must_use]
    pub fn new(intent: ExecutionIntent, backend: ExecutionBackend, live: bool) -> Self {
        Self {
            intent,
            backend,
            state: if live {
                ExecutionState::PendingApproval
            } else {
                ExecutionState::Scheduled
            },
            executed_quantity: Decimal::ZERO,
        }
    }

    pub fn transition(&mut self, next: ExecutionState) -> Result<(), TransitionError> {
        if !is_allowed(self.state, next) {
            return Err(TransitionError {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

const fn is_allowed(from: ExecutionState, to: ExecutionState) -> bool {
    matches!(
        (from, to),
        (ExecutionState::PendingApproval, ExecutionState::Scheduled)
            | (ExecutionState::PendingApproval, ExecutionState::Rejected)
            | (ExecutionState::PendingApproval, ExecutionState::Cancelled)
            | (ExecutionState::Scheduled, ExecutionState::Running)
            | (ExecutionState::Scheduled, ExecutionState::Cancelled)
            | (ExecutionState::Scheduled, ExecutionState::Expired)
            | (ExecutionState::Running, ExecutionState::Paused)
            | (ExecutionState::Running, ExecutionState::Cancelling)
            | (ExecutionState::Running, ExecutionState::Completed)
            | (ExecutionState::Running, ExecutionState::Expired)
            | (ExecutionState::Running, ExecutionState::Failed)
            | (ExecutionState::Paused, ExecutionState::Running)
            | (ExecutionState::Paused, ExecutionState::Cancelling)
            | (ExecutionState::Paused, ExecutionState::Expired)
            | (ExecutionState::Paused, ExecutionState::Failed)
            | (ExecutionState::Cancelling, ExecutionState::Cancelled)
            | (ExecutionState::Cancelling, ExecutionState::Failed)
    )
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid execution state transition from {from:?} to {to:?}")]
pub struct TransitionError {
    pub from: ExecutionState,
    pub to: ExecutionState,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    #[must_use]
    pub const fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketSnapshot {
    pub instrument: InstrumentId,
    pub bids: Vec<MarketLevel>,
    pub asks: Vec<MarketLevel>,
    pub exchange_time_ms: i64,
    pub received_at_ms: i64,
}

#[async_trait]
pub trait MarketDataPort: Send + Sync {
    async fn order_book(&self, instrument: &InstrumentId) -> Result<MarketSnapshot, PortError>;
    async fn traded_volume_since(
        &self,
        instrument: &InstrumentId,
        since: DateTime<Utc>,
    ) -> Result<Decimal, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub observed_at: DateTime<Utc>,
    pub balances: Vec<(String, Decimal)>,
    pub positions: Vec<(InstrumentId, Decimal)>,
}

#[async_trait]
pub trait AccountPort: Send + Sync {
    async fn snapshot(&self, account_id: &str) -> Result<AccountSnapshot, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionEvent {
    TaskApproved {
        actor_id: String,
    },
    TaskRejected {
        actor_id: String,
        reason: String,
    },
    OrderSubmissionStarted {
        client_order_id: String,
    },
    OrderSubmissionUnknown {
        client_order_id: String,
    },
    OrderObserved {
        client_order_id: String,
        state: OrderState,
    },
    TaskPaused {
        reason: String,
    },
    TaskResumed,
    TaskCompleted,
    TaskFailed {
        code: String,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PortError {
    #[error("capability is not supported: {0}")]
    Unsupported(&'static str),
    #[error("external data is stale")]
    Stale,
    #[error("request result is unknown")]
    SubmissionUnknown,
    #[error("external service rejected the request: {0}")]
    Rejected(String),
    #[error("external service is unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagedSliceInput {
    pub strategy: StrategyKind,
    pub target_amount: Decimal,
    pub executed_amount: Decimal,
    pub market_volume: Decimal,
    pub remaining_time_ms: i64,
    pub slice_interval_ms: i64,
    pub participation_rate: Option<Decimal>,
    pub max_slice_amount: Option<Decimal>,
}

pub fn calculate_managed_slice(input: ManagedSliceInput) -> Result<Decimal, DecisionError> {
    if input.slice_interval_ms <= 0 || input.remaining_time_ms <= 0 {
        return Err(DecisionError::InvalidSchedule);
    }
    let remaining_ticks = ((input.remaining_time_ms + input.slice_interval_ms - 1)
        / input.slice_interval_ms)
        .max(1) as u32;
    let tick = StrategyTickInput {
        target_amount: input.target_amount,
        executed_amount: input.executed_amount,
        market_volume: input.market_volume,
        elapsed_ticks: 0,
        total_ticks: remaining_ticks,
    };
    let amount = match input.strategy {
        StrategyKind::Twap => Twap.next_slice_quantity(tick),
        StrategyKind::Pov => Pov::new(
            input
                .participation_rate
                .ok_or(DecisionError::MissingParticipationRate)?,
        )?
        .next_slice_quantity(tick),
    }?;
    Ok(input
        .max_slice_amount
        .map_or(amount, |maximum| amount.min(maximum)))
}

pub fn protected_conversion_price(
    side: Side,
    unit: QuantityUnit,
    reference_price: Decimal,
    max_slippage_bps: i32,
) -> Decimal {
    const BPS: Decimal = Decimal::from_parts(10_000, 0, 0, false, 0);
    if side == Side::Buy && unit == QuantityUnit::QuoteNotional {
        reference_price * (Decimal::ONE + Decimal::from(max_slippage_bps) / BPS)
    } else {
        reference_price
    }
}

#[derive(Debug, Error)]
pub enum DecisionError {
    #[error("execution schedule is invalid")]
    InvalidSchedule,
    #[error("POV execution is missing a participation rate")]
    MissingParticipationRate,
    #[error(transparent)]
    Strategy(#[from] StrategyError),
}

#[cfg(test)]
mod tests {
    use ballast_core::{
        Exchange, InstrumentId, MarketKind, QuantityUnit, StrategyKind, TargetAmount,
    };

    use super::*;

    fn task(live: bool) -> ExecutionTask {
        ExecutionTask::new(
            ballast_core::ExecutionIntent::new(
                "paper",
                InstrumentId::new(Exchange::Binance, MarketKind::Spot, "BTC/USDT").unwrap(),
                Side::Sell,
                StrategyKind::Twap,
                TargetAmount {
                    amount: Decimal::ONE,
                    unit: QuantityUnit::BaseQuantity,
                },
            )
            .unwrap(),
            ExecutionBackend::ManagedIoc,
            live,
        )
    }

    #[test]
    fn live_task_requires_approval() {
        let mut task = task(true);
        assert_eq!(task.state, ExecutionState::PendingApproval);
        task.transition(ExecutionState::Scheduled).unwrap();
        task.transition(ExecutionState::Running).unwrap();
    }

    #[test]
    fn rejected_task_is_terminal() {
        let mut task = task(true);
        task.transition(ExecutionState::Rejected).unwrap();
        assert!(task.transition(ExecutionState::Scheduled).is_err());
    }

    #[test]
    fn paper_pause_recovery_and_cancellation_are_explicit() {
        let mut task = task(false);
        task.transition(ExecutionState::Running).unwrap();
        task.transition(ExecutionState::Paused).unwrap();
        task.transition(ExecutionState::Running).unwrap();
        task.transition(ExecutionState::Cancelling).unwrap();
        task.transition(ExecutionState::Cancelled).unwrap();
    }

    #[test]
    fn twap_rebalances_remaining_amount_over_remaining_ticks() {
        let amount = calculate_managed_slice(ManagedSliceInput {
            strategy: StrategyKind::Twap,
            target_amount: Decimal::from(10),
            executed_amount: Decimal::from(4),
            market_volume: Decimal::ZERO,
            remaining_time_ms: 3_000,
            slice_interval_ms: 1_000,
            participation_rate: None,
            max_slice_amount: None,
        })
        .unwrap();
        assert_eq!(amount, Decimal::from(2));
    }

    #[test]
    fn pov_requires_explicit_participation_rate() {
        let result = calculate_managed_slice(ManagedSliceInput {
            strategy: StrategyKind::Pov,
            target_amount: Decimal::from(10),
            executed_amount: Decimal::ZERO,
            market_volume: Decimal::from(5),
            remaining_time_ms: 1_000,
            slice_interval_ms: 1_000,
            participation_rate: None,
            max_slice_amount: None,
        });
        assert!(matches!(
            result,
            Err(DecisionError::MissingParticipationRate)
        ));
    }
}
