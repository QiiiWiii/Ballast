#![forbid(unsafe_code)]

use ballast_core::ExecutionIntent;
use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    Created,
    Running,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
    SubmissionUnknown,
}

#[derive(Debug)]
pub struct ExecutionTask {
    pub intent: ExecutionIntent,
    pub state: ExecutionState,
    pub executed_quantity: Decimal,
}

impl ExecutionTask {
    #[must_use]
    pub fn new(intent: ExecutionIntent) -> Self {
        Self {
            intent,
            state: ExecutionState::Created,
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
        (ExecutionState::Created, ExecutionState::Running)
            | (ExecutionState::Created, ExecutionState::Cancelled)
            | (ExecutionState::Running, ExecutionState::Cancelling)
            | (ExecutionState::Running, ExecutionState::Completed)
            | (ExecutionState::Running, ExecutionState::Failed)
            | (ExecutionState::Running, ExecutionState::SubmissionUnknown)
            | (ExecutionState::SubmissionUnknown, ExecutionState::Running)
            | (ExecutionState::SubmissionUnknown, ExecutionState::Failed)
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
