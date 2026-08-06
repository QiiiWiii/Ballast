#![forbid(unsafe_code)]

mod pov;
mod twap;

use rust_decimal::Decimal;
use thiserror::Error;

pub use pov::Pov;
pub use twap::Twap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrategyTickInput {
    pub target_amount: Decimal,
    pub executed_amount: Decimal,
    pub market_volume: Decimal,
    pub elapsed_ticks: u32,
    pub total_ticks: u32,
}

impl StrategyTickInput {
    #[must_use]
    pub fn remaining_quantity(self) -> Decimal {
        (self.target_amount - self.executed_amount).max(Decimal::ZERO)
    }
}

pub trait Strategy: Send + Sync {
    fn next_slice_quantity(&self, input: StrategyTickInput) -> Result<Decimal, StrategyError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrategyError {
    #[error("total ticks must be greater than zero")]
    ZeroTotalTicks,
    #[error("participation rate must be greater than zero and at most one")]
    InvalidParticipationRate,
}
