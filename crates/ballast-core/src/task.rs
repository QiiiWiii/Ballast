pub use barter_instrument::Side;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, InstrumentId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutionTaskId(Uuid);

impl ExecutionTaskId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for ExecutionTaskId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    Twap,
    Pov,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantityUnit {
    BaseQuantity,
    QuoteNotional,
    Contracts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetAmount {
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    pub unit: QuantityUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionIntent {
    pub task_id: ExecutionTaskId,
    pub account_id: String,
    pub instrument: InstrumentId,
    pub side: Side,
    pub strategy: StrategyKind,
    pub target: TargetAmount,
}

impl ExecutionIntent {
    pub fn new(
        account_id: impl Into<String>,
        instrument: InstrumentId,
        side: Side,
        strategy: StrategyKind,
        target: TargetAmount,
    ) -> Result<Self, DomainError> {
        if target.amount <= Decimal::ZERO {
            return Err(DomainError::NonPositiveQuantity);
        }

        Ok(Self {
            task_id: ExecutionTaskId::new(),
            account_id: account_id.into(),
            instrument,
            side,
            strategy,
            target,
        })
    }
}
