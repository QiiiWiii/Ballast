use async_trait::async_trait;
use ballast_core::{InstrumentId, Side};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::num::NonZeroU32;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

use crate::PortError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    SubmissionPending,
    SubmissionUnknown,
    Open,
    PartiallyFilled,
    Filled,
    CancelPending,
    Cancelled,
    Rejected,
    Expired,
    Failed,
}

impl OrderState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubmissionPending => "submission_pending",
            Self::SubmissionUnknown => "submission_unknown",
            Self::Open => "open",
            Self::PartiallyFilled => "partially_filled",
            Self::Filled => "filled",
            Self::CancelPending => "cancel_pending",
            Self::Cancelled => "cancelled",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Filled | Self::Cancelled | Self::Rejected | Self::Expired | Self::Failed
        )
    }

    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::SubmissionPending, Self::SubmissionUnknown)
                | (Self::SubmissionPending, Self::Open)
                | (Self::SubmissionPending, Self::PartiallyFilled)
                | (Self::SubmissionPending, Self::Filled)
                | (Self::SubmissionPending, Self::Cancelled)
                | (Self::SubmissionPending, Self::Rejected)
                | (Self::SubmissionPending, Self::Expired)
                | (Self::SubmissionPending, Self::Failed)
                | (Self::SubmissionUnknown, Self::Open)
                | (Self::SubmissionUnknown, Self::PartiallyFilled)
                | (Self::SubmissionUnknown, Self::Filled)
                | (Self::SubmissionUnknown, Self::Cancelled)
                | (Self::SubmissionUnknown, Self::Rejected)
                | (Self::SubmissionUnknown, Self::Expired)
                | (Self::SubmissionUnknown, Self::Failed)
                | (Self::Open, Self::PartiallyFilled)
                | (Self::Open, Self::Filled)
                | (Self::Open, Self::CancelPending)
                | (Self::Open, Self::Cancelled)
                | (Self::Open, Self::Expired)
                | (Self::Open, Self::Failed)
                | (Self::PartiallyFilled, Self::Filled)
                | (Self::PartiallyFilled, Self::CancelPending)
                | (Self::PartiallyFilled, Self::Cancelled)
                | (Self::PartiallyFilled, Self::Expired)
                | (Self::PartiallyFilled, Self::Failed)
                | (Self::CancelPending, Self::Open)
                | (Self::CancelPending, Self::PartiallyFilled)
                | (Self::CancelPending, Self::Filled)
                | (Self::CancelPending, Self::Cancelled)
                | (Self::CancelPending, Self::Failed)
        )
    }
}

impl FromStr for OrderState {
    type Err = OrderStateParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "submission_pending" => Ok(Self::SubmissionPending),
            "submission_unknown" => Ok(Self::SubmissionUnknown),
            "open" => Ok(Self::Open),
            "partially_filled" => Ok(Self::PartiallyFilled),
            "filled" => Ok(Self::Filled),
            "cancel_pending" => Ok(Self::CancelPending),
            "cancelled" => Ok(Self::Cancelled),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            "failed" => Ok(Self::Failed),
            _ => Err(OrderStateParseError(value.to_owned())),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("unknown order state: {0}")]
pub struct OrderStateParseError(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitOrder {
    pub task_id: Uuid,
    pub account_id: String,
    pub instrument: InstrumentId,
    pub client_order_id: String,
    pub side: Side,
    pub quantity: Decimal,
    pub limit_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderSnapshot {
    pub client_order_id: String,
    pub exchange_order_id: Option<String>,
    pub state: OrderState,
    pub filled_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderLifecycle {
    pub client_order_id: String,
    pub state: OrderState,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub exchange_order_id: Option<String>,
}

impl OrderLifecycle {
    pub fn observe(&mut self, snapshot: &OrderSnapshot) -> Result<bool, OrderTransitionError> {
        if snapshot.client_order_id != self.client_order_id {
            return Err(OrderTransitionError::ClientOrderIdMismatch);
        }
        if snapshot.filled_quantity < self.filled_quantity
            || snapshot.filled_quantity < Decimal::ZERO
            || snapshot.filled_quantity > self.quantity
        {
            return Err(OrderTransitionError::FilledQuantityInvalid);
        }
        if snapshot.state != self.state
            && (self.state.is_terminal() || !self.state.can_transition_to(snapshot.state))
        {
            return Err(OrderTransitionError::InvalidState {
                from: self.state,
                to: snapshot.state,
            });
        }
        if matches!(snapshot.state, OrderState::Open | OrderState::Rejected)
            && snapshot.filled_quantity != Decimal::ZERO
            || snapshot.state == OrderState::PartiallyFilled
                && (snapshot.filled_quantity == Decimal::ZERO
                    || snapshot.filled_quantity == self.quantity)
            || snapshot.state == OrderState::Filled && snapshot.filled_quantity != self.quantity
        {
            return Err(OrderTransitionError::StateQuantityMismatch);
        }
        if matches!(
            snapshot.state,
            OrderState::Open
                | OrderState::PartiallyFilled
                | OrderState::Filled
                | OrderState::CancelPending
                | OrderState::Cancelled
                | OrderState::Expired
        ) && snapshot.exchange_order_id.is_none()
        {
            return Err(OrderTransitionError::MissingExchangeOrderId);
        }
        if let (Some(current), Some(observed)) = (
            self.exchange_order_id.as_deref(),
            snapshot.exchange_order_id.as_deref(),
        ) && current != observed
        {
            return Err(OrderTransitionError::ExchangeOrderIdMismatch);
        }
        if snapshot.state == self.state {
            let changed = snapshot.filled_quantity != self.filled_quantity
                || (self.exchange_order_id.is_none() && snapshot.exchange_order_id.is_some());
            self.filled_quantity = snapshot.filled_quantity;
            if self.exchange_order_id.is_none() {
                self.exchange_order_id
                    .clone_from(&snapshot.exchange_order_id);
            }
            return Ok(changed);
        }
        self.state = snapshot.state;
        self.filled_quantity = snapshot.filled_quantity;
        if self.exchange_order_id.is_none() {
            self.exchange_order_id
                .clone_from(&snapshot.exchange_order_id);
        }
        Ok(true)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OrderTransitionError {
    #[error("client order id does not match the persisted order")]
    ClientOrderIdMismatch,
    #[error("exchange order id changed for the same client order id")]
    ExchangeOrderIdMismatch,
    #[error("observed filled quantity is invalid")]
    FilledQuantityInvalid,
    #[error("observed order state does not match its filled quantity")]
    StateQuantityMismatch,
    #[error("exchange-confirmed order state is missing an exchange order id")]
    MissingExchangeOrderId,
    #[error("invalid order state transition from {from:?} to {to:?}")]
    InvalidState { from: OrderState, to: OrderState },
}

#[must_use]
pub fn stable_client_order_id(task_id: Uuid, slice_sequence: NonZeroU32) -> String {
    let mut digest = Sha256::new();
    digest.update(b"ballast-managed-ioc-v1");
    digest.update(task_id.as_bytes());
    digest.update(slice_sequence.get().to_be_bytes());
    let encoded = hex::encode(digest.finalize());
    format!("b1{}", &encoded[..30])
}

#[async_trait]
pub trait ExecutionPort: Send + Sync {
    async fn submit_ioc(&self, command: SubmitOrder) -> Result<OrderSnapshot, PortError>;
    async fn cancel(
        &self,
        account_id: &str,
        client_order_id: &str,
    ) -> Result<OrderSnapshot, PortError>;
    async fn reconcile(
        &self,
        account_id: &str,
        client_order_id: &str,
    ) -> Result<Option<OrderSnapshot>, PortError>;
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn snapshot(state: OrderState, filled_quantity: Decimal) -> OrderSnapshot {
        OrderSnapshot {
            client_order_id: "b1stable".to_owned(),
            exchange_order_id: Some("exchange-1".to_owned()),
            state,
            filled_quantity,
            average_price: Some(Decimal::from(100)),
            observed_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
        }
    }

    fn lifecycle() -> OrderLifecycle {
        OrderLifecycle {
            client_order_id: "b1stable".to_owned(),
            state: OrderState::SubmissionPending,
            quantity: Decimal::ONE,
            filled_quantity: Decimal::ZERO,
            exchange_order_id: None,
        }
    }

    #[test]
    fn client_order_id_is_stable_and_okx_compatible() {
        let task_id = Uuid::parse_str("018f5f7a-3b75-7cc1-8a8e-d0c538f976ab").unwrap();
        let first = stable_client_order_id(task_id, NonZeroU32::new(7).unwrap());
        assert_eq!(
            first,
            stable_client_order_id(task_id, NonZeroU32::new(7).unwrap())
        );
        assert_ne!(
            first,
            stable_client_order_id(task_id, NonZeroU32::new(8).unwrap())
        );
        assert_eq!(first.len(), 32);
        assert!(
            first
                .chars()
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        );
    }

    #[test]
    fn order_state_database_text_round_trips() {
        for state in [
            OrderState::SubmissionPending,
            OrderState::SubmissionUnknown,
            OrderState::Open,
            OrderState::PartiallyFilled,
            OrderState::Filled,
            OrderState::CancelPending,
            OrderState::Cancelled,
            OrderState::Rejected,
            OrderState::Expired,
            OrderState::Failed,
        ] {
            assert_eq!(OrderState::from_str(state.as_str()).unwrap(), state);
        }
        assert!(OrderState::from_str("unknown").is_err());
    }

    #[test]
    fn submission_unknown_reconciles_without_resubmission_state() {
        let mut order = lifecycle();
        assert!(
            order
                .observe(&snapshot(OrderState::SubmissionUnknown, Decimal::ZERO))
                .unwrap()
        );
        assert!(
            order
                .observe(&snapshot(OrderState::Open, Decimal::ZERO))
                .unwrap()
        );
        assert!(
            order
                .observe(&snapshot(OrderState::Filled, Decimal::ONE))
                .unwrap()
        );
        assert!(order.state.is_terminal());
    }

    #[test]
    fn ioc_can_reconcile_directly_to_cancelled_or_expired() {
        let mut cancelled = lifecycle();
        assert!(
            cancelled
                .observe(&snapshot(OrderState::Cancelled, Decimal::ZERO))
                .unwrap()
        );

        let mut expired = lifecycle();
        assert!(
            expired
                .observe(&snapshot(OrderState::Expired, Decimal::new(4, 1)))
                .unwrap()
        );
    }

    #[test]
    fn observations_cannot_reduce_fills_or_reopen_terminal_orders() {
        let mut order = lifecycle();
        order
            .observe(&snapshot(OrderState::PartiallyFilled, Decimal::new(5, 1)))
            .unwrap();
        assert_eq!(
            order.observe(&snapshot(OrderState::Open, Decimal::new(4, 1))),
            Err(OrderTransitionError::FilledQuantityInvalid)
        );
        order
            .observe(&snapshot(OrderState::Filled, Decimal::ONE))
            .unwrap();
        assert!(matches!(
            order.observe(&snapshot(OrderState::Open, Decimal::ONE)),
            Err(OrderTransitionError::InvalidState { .. })
        ));
    }

    #[test]
    fn order_state_must_match_observed_filled_quantity() {
        let mut order = lifecycle();
        assert_eq!(
            order.observe(&snapshot(OrderState::Filled, Decimal::new(9, 1))),
            Err(OrderTransitionError::StateQuantityMismatch)
        );
        assert_eq!(
            order.observe(&snapshot(OrderState::PartiallyFilled, Decimal::ZERO)),
            Err(OrderTransitionError::StateQuantityMismatch)
        );
    }

    #[test]
    fn exchange_confirmed_state_requires_exchange_order_id() {
        let mut order = lifecycle();
        let mut observed = snapshot(OrderState::Open, Decimal::ZERO);
        observed.exchange_order_id = None;
        assert_eq!(
            order.observe(&observed),
            Err(OrderTransitionError::MissingExchangeOrderId)
        );
    }

    #[test]
    fn duplicate_state_observation_is_an_event_noop() {
        let mut order = lifecycle();
        assert!(
            order
                .observe(&snapshot(OrderState::PartiallyFilled, Decimal::new(1, 1)))
                .unwrap()
        );
        assert!(
            !order
                .observe(&snapshot(OrderState::PartiallyFilled, Decimal::new(1, 1)))
                .unwrap()
        );
        assert!(
            order
                .observe(&snapshot(OrderState::PartiallyFilled, Decimal::new(2, 1)))
                .unwrap()
        );
    }
}
