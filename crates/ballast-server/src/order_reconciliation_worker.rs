use std::time::Duration;

use ballast_core::{Exchange, MarketKind};
use ballast_gateway_client::{
    GatewayClient, GatewayClientError, execution_order_snapshot_from_proto, proto,
};
use ballast_storage::{
    ChildOrderStateUpdate, ClaimedChildOrderReconciliation, DatabasePool,
    claim_child_orders_for_reconciliation, compare_and_set_child_order_state,
    complete_child_order_reconciliation, fail_child_order_reconciliation, get_instrument,
    suspend_child_order_reconciliation,
};
use chrono::Utc;
use futures_util::{StreamExt, stream};
use rust_decimal::Decimal;
use std::str::FromStr;
use thiserror::Error;
use tonic::Code;
use tracing::{error, info, warn};
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const FAILURE_RETRY_INTERVAL: Duration = Duration::from_secs(10);
const CAPABILITY_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const CLAIM_DURATION_SECONDS: i64 = 30;
const SUCCESS_INTERVAL_SECONDS: i64 = 5;
const FAILURE_RETRY_SECONDS: i64 = 10;
const MAX_CONCURRENT_RECONCILIATIONS: usize = 4;
const CLAIM_BATCH_SIZE: i64 = MAX_CONCURRENT_RECONCILIATIONS as i64;
const ORDER_NOT_FOUND_RETRY_LIMIT: i32 = 5;

#[derive(Debug, Error)]
enum ReconciliationError {
    #[error("transient gateway or storage failure: {0}")]
    Transient(String),
    #[error("gateway did not find the order")]
    NotFound,
    #[error("permanent reconciliation failure: {0}")]
    Permanent(&'static str),
    #[error("reconciliation claim was lost")]
    ClaimLost,
}

pub fn spawn_worker(database: DatabasePool, gateway: GatewayClient) {
    tokio::spawn(async move {
        loop {
            match gateway.trading_capabilities(Exchange::Okx).await {
                Ok(capabilities) if capabilities.query_by_client_order_id => break,
                Ok(_) => {
                    info!("OKX order reconciliation is disabled by gateway capabilities");
                    tokio::time::sleep(CAPABILITY_RETRY_INTERVAL).await;
                }
                Err(error) => {
                    warn!(%error, "failed to read OKX order reconciliation capability");
                    tokio::time::sleep(FAILURE_RETRY_INTERVAL).await;
                }
            }
        }
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        loop {
            interval.tick().await;
            let claimed_until = Utc::now() + chrono::Duration::seconds(CLAIM_DURATION_SECONDS);
            match claim_child_orders_for_reconciliation(&database, CLAIM_BATCH_SIZE, claimed_until)
                .await
            {
                Ok(orders) => {
                    stream::iter(orders)
                        .for_each_concurrent(MAX_CONCURRENT_RECONCILIATIONS, |claimed| {
                            let database = database.clone();
                            let gateway = gateway.clone();
                            async move {
                                if let Err(error) =
                                    reconcile_one(&database, &gateway, &claimed).await
                                {
                                    warn!(
                                        %error,
                                        order_id = %claimed.order.id,
                                        client_order_id = %claimed.order.client_order_id,
                                        "child order reconciliation failed"
                                    );
                                    let result = match error {
                                        ReconciliationError::Transient(_) => {
                                            fail_child_order_reconciliation(
                                                &database,
                                                claimed.order.id,
                                                claimed.claim_token,
                                                Utc::now() + retry_delay(claimed.failure_count),
                                                "transient_query_failure",
                                            )
                                            .await
                                        }
                                        ReconciliationError::NotFound
                                            if claimed.failure_count
                                                < ORDER_NOT_FOUND_RETRY_LIMIT =>
                                        {
                                            fail_child_order_reconciliation(
                                                &database,
                                                claimed.order.id,
                                                claimed.claim_token,
                                                Utc::now() + retry_delay(claimed.failure_count),
                                                "order_not_found",
                                            )
                                            .await
                                        }
                                        ReconciliationError::NotFound => {
                                            suspend_child_order_reconciliation(
                                                &database,
                                                claimed.order.id,
                                                claimed.claim_token,
                                                "order_not_found_observation_exhausted",
                                            )
                                            .await
                                        }
                                        ReconciliationError::Permanent(code) => {
                                            suspend_child_order_reconciliation(
                                                &database,
                                                claimed.order.id,
                                                claimed.claim_token,
                                                code,
                                            )
                                            .await
                                        }
                                        ReconciliationError::ClaimLost => return,
                                    };
                                    if let Err(storage_error) = result {
                                        error!(
                                            %storage_error,
                                            order_id = %claimed.order.id,
                                            "failed to release child order reconciliation claim"
                                        );
                                    }
                                }
                            }
                        })
                        .await;
                }
                Err(error) => error!(%error, "failed to claim child orders for reconciliation"),
            }
        }
    });
}

async fn reconcile_one(
    database: &DatabasePool,
    gateway: &GatewayClient,
    claimed: &ClaimedChildOrderReconciliation,
) -> Result<(), ReconciliationError> {
    let instrument = get_instrument(database, claimed.instrument_id)
        .await
        .map_err(storage_error)?
        .ok_or(ReconciliationError::Permanent(
            "child_order_instrument_missing",
        ))?
        .instrument;
    if instrument.id.exchange != Exchange::Okx || claimed.order.exchange != "okx" {
        return Err(ReconciliationError::Permanent(
            "child_order_exchange_mismatch",
        ));
    }
    let response = gateway
        .get_order_by_client_id(proto::GetOrderByClientIdRequest {
            account: Some(proto::AccountRef {
                account_id: claimed.order.account_id.clone(),
                exchange: proto::Exchange::Okx as i32,
                environment: account_environment_to_proto(&claimed.account_environment)
                    .map_err(ReconciliationError::Permanent)?,
            }),
            instrument: Some(proto::InstrumentKey {
                exchange: proto::Exchange::Okx as i32,
                market_kind: market_kind_to_proto(instrument.id.market_kind),
                symbol: instrument.id.symbol.clone(),
            }),
            request_id: Uuid::now_v7().to_string(),
            client_order_id: claimed.order.client_order_id.clone(),
        })
        .await
        .map_err(gateway_error)?;
    validate_response_intent(claimed, &instrument.id, &response)
        .map_err(ReconciliationError::Permanent)?;
    let reason_code = response.reason_code.clone();
    let snapshot = execution_order_snapshot_from_proto(response)
        .map_err(|_| ReconciliationError::Permanent("gateway_order_protocol_invalid"))?;
    let updated = compare_and_set_child_order_state(
        database,
        &claimed.order.exchange,
        &claimed.order.account_id,
        &claimed.order.client_order_id,
        &[claimed.order.status],
        &ChildOrderStateUpdate {
            status: snapshot.state,
            exchange_order_id: snapshot.exchange_order_id,
            filled_quantity: snapshot.filled_quantity,
            average_price: snapshot.average_price,
            limit_price: claimed.order.limit_price,
            raw_status: None,
            state_reason: reason_code,
            submitted_at: claimed.order.submitted_at,
            last_reconciled_at: Some(snapshot.observed_at),
        },
    )
    .await
    .map_err(storage_error)?;
    if updated.status.is_terminal() {
        ballast_storage::mark_task_state(database, updated.task_id, "running", None, Utc::now())
            .await
            .map_err(storage_error)?;
    }
    if !complete_child_order_reconciliation(
        database,
        claimed.order.id,
        claimed.claim_token,
        Utc::now() + chrono::Duration::seconds(SUCCESS_INTERVAL_SECONDS),
    )
    .await
    .map_err(storage_error)?
    {
        return Err(ReconciliationError::ClaimLost);
    }
    Ok(())
}

fn validate_response_intent(
    claimed: &ClaimedChildOrderReconciliation,
    instrument: &ballast_core::InstrumentId,
    response: &proto::OrderSnapshot,
) -> Result<(), &'static str> {
    let account = response
        .account
        .as_ref()
        .ok_or("gateway order account is missing")?;
    if account.account_id != claimed.order.account_id
        || account.exchange != proto::Exchange::Okx as i32
        || account.environment != account_environment_to_proto(&claimed.account_environment)?
    {
        return Err("gateway order account does not match persisted intent");
    }
    if response.client_order_id != claimed.order.client_order_id {
        return Err("gateway order client id does not match persisted intent");
    }
    let response_instrument = response
        .instrument
        .as_ref()
        .ok_or("gateway order instrument is missing")?;
    if response_instrument.exchange != proto::Exchange::Okx as i32
        || response_instrument.market_kind != market_kind_to_proto(instrument.market_kind)
        || response_instrument.symbol != instrument.symbol
    {
        return Err("gateway order instrument does not match persisted intent");
    }
    let side = match claimed.order.side.as_str() {
        "buy" => proto::Side::Buy as i32,
        "sell" => proto::Side::Sell as i32,
        _ => return Err("persisted child order side is invalid"),
    };
    if response.side != side {
        return Err("gateway order side does not match persisted intent");
    }
    let quantity =
        Decimal::from_str(&response.quantity).map_err(|_| "gateway order quantity is invalid")?;
    if quantity != claimed.order.quantity {
        return Err("gateway order quantity does not match persisted intent");
    }
    Ok(())
}

fn gateway_error(error: GatewayClientError) -> ReconciliationError {
    match error {
        GatewayClientError::Transport(error) => ReconciliationError::Transient(error.to_string()),
        GatewayClientError::Rpc(status) => match status.code() {
            Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted => {
                ReconciliationError::Transient(status.to_string())
            }
            Code::NotFound => ReconciliationError::NotFound,
            _ => ReconciliationError::Permanent("gateway_order_query_rejected"),
        },
        _ => ReconciliationError::Permanent("gateway_order_protocol_invalid"),
    }
}

fn storage_error(error: sqlx::Error) -> ReconciliationError {
    match error {
        sqlx::Error::Protocol(_) => {
            ReconciliationError::Permanent("child_order_state_convergence_invalid")
        }
        other => ReconciliationError::Transient(other.to_string()),
    }
}

fn retry_delay(failure_count: i32) -> chrono::Duration {
    let exponent = failure_count.clamp(0, 5) as u32;
    chrono::Duration::seconds(FAILURE_RETRY_SECONDS * 2_i64.pow(exponent))
}

fn account_environment_to_proto(value: &str) -> Result<i32, &'static str> {
    match value {
        "demo" => Ok(proto::AccountEnvironment::Demo as i32),
        "production" => Ok(proto::AccountEnvironment::Production as i32),
        _ => Err("child order account environment is invalid"),
    }
}

const fn market_kind_to_proto(value: MarketKind) -> i32 {
    match value {
        MarketKind::Spot => proto::MarketKind::Spot as i32,
        MarketKind::Perpetual => proto::MarketKind::Perpetual as i32,
    }
}

#[cfg(test)]
mod tests {
    use ballast_storage::StoredChildOrder;
    use chrono::TimeZone;

    use super::*;

    fn claimed_order() -> ClaimedChildOrderReconciliation {
        let now = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        ClaimedChildOrderReconciliation {
            order: StoredChildOrder {
                id: Uuid::nil(),
                task_id: Uuid::nil(),
                exchange: "okx".to_owned(),
                account_id: "okx-demo".to_owned(),
                request_id: Uuid::nil(),
                client_order_id: "b100000000000000000000000000000".to_owned(),
                exchange_order_id: None,
                side: "sell".to_owned(),
                order_type: "limit_ioc".to_owned(),
                order_backend: "managed_ioc".to_owned(),
                quantity: Decimal::ONE,
                filled_quantity: Decimal::ZERO,
                average_price: None,
                limit_price: Some(Decimal::from(100)),
                status: ballast_execution::OrderState::SubmissionUnknown,
                raw_status: None,
                state_reason: Some("submit_timeout".to_owned()),
                submitted_at: None,
                last_reconciled_at: Some(now),
                created_at: now,
                updated_at: now,
            },
            account_environment: "demo".to_owned(),
            instrument_id: Uuid::nil(),
            claim_token: Uuid::nil(),
            failure_count: 0,
        }
    }

    #[test]
    fn account_environment_is_explicit() {
        assert_eq!(
            account_environment_to_proto("demo").unwrap(),
            proto::AccountEnvironment::Demo as i32
        );
        assert!(account_environment_to_proto("paper").is_err());
    }

    #[test]
    fn response_intent_must_match_persisted_order() {
        let claimed = claimed_order();
        let instrument =
            ballast_core::InstrumentId::new(Exchange::Okx, MarketKind::Spot, "BTC/USDT").unwrap();
        let mut response = proto::OrderSnapshot {
            account: Some(proto::AccountRef {
                account_id: "okx-demo".to_owned(),
                exchange: proto::Exchange::Okx as i32,
                environment: proto::AccountEnvironment::Demo as i32,
            }),
            instrument: Some(proto::InstrumentKey {
                exchange: proto::Exchange::Okx as i32,
                market_kind: proto::MarketKind::Spot as i32,
                symbol: "BTC/USDT".to_owned(),
            }),
            client_order_id: claimed.order.client_order_id.clone(),
            exchange_order_id: Some("exchange-1".to_owned()),
            state: proto::OrderState::Open as i32,
            side: proto::Side::Sell as i32,
            quantity: "1".to_owned(),
            filled_quantity: "0".to_owned(),
            average_price: None,
            reason_code: None,
            exchange_time_ms: 1_700_000_000_000,
            gateway_received_at_ms: 1_700_000_000_100,
        };
        assert!(validate_response_intent(&claimed, &instrument, &response).is_ok());
        response.quantity = "2".to_owned();
        assert!(validate_response_intent(&claimed, &instrument, &response).is_err());
        response.quantity = "1".to_owned();
        response.client_order_id = "b199999999999999999999999999999".to_owned();
        assert!(validate_response_intent(&claimed, &instrument, &response).is_err());
    }

    #[test]
    fn transient_retries_back_off_with_a_cap() {
        assert_eq!(retry_delay(0), chrono::Duration::seconds(10));
        assert_eq!(retry_delay(3), chrono::Duration::seconds(80));
        assert_eq!(retry_delay(99), chrono::Duration::seconds(320));
    }
}
