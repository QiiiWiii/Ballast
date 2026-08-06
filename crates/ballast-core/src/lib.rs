#![forbid(unsafe_code)]

mod error;
mod instrument;
mod task;

pub use error::DomainError;
pub use instrument::{ContractKind, Exchange, Instrument, InstrumentId, MarketKind};
pub use task::{ExecutionIntent, ExecutionTaskId, QuantityUnit, Side, StrategyKind, TargetAmount};
