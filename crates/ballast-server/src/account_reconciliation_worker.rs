use std::time::Duration;

use ballast_gateway_client::{GatewayClient, GatewayClientError, proto};
use ballast_storage::{
    DatabasePool, LocalOpenOrderSummary, NewAccountSnapshot, StoredAccount,
    list_account_open_orders, list_accounts, record_account_reconciliation,
    record_failed_account_reconciliation,
};
use chrono::{TimeZone, Utc};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use tonic::Code;
use tracing::{error, warn};
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const MAX_CONCURRENT_ACCOUNTS: usize = 4;

pub fn spawn_worker(
    database: DatabasePool,
    gateway: GatewayClient,
    reconciliation_alerts_enabled: bool,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        loop {
            interval.tick().await;
            match list_accounts(&database).await {
                Ok(accounts) => {
                    stream::iter(
                        accounts
                            .into_iter()
                            .filter(|account| account.enabled && account.exchange == "okx"),
                    )
                        .for_each_concurrent(MAX_CONCURRENT_ACCOUNTS, |account| {
                            let database = database.clone();
                            let gateway = gateway.clone();
                            async move {
                                if let Err(error) = reconcile_account(
                                    &database,
                                    &gateway,
                                    &account,
                                    reconciliation_alerts_enabled,
                                )
                                .await
                                {
                                    warn!(account_id = %account.id, %error, "account reconciliation failed");
                                }
                            }
                        })
                        .await;
                }
                Err(storage_error) => {
                    error!(%storage_error, "failed to list accounts for reconciliation");
                }
            }
        }
    });
}

async fn reconcile_account(
    database: &DatabasePool,
    gateway: &GatewayClient,
    account: &StoredAccount,
    reconciliation_alerts_enabled: bool,
) -> Result<(), String> {
    let mut lock_transaction = database.begin().await.map_err(|error| error.to_string())?;
    let claimed: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1, 1869376613))")
            .bind(&account.id)
            .fetch_one(&mut *lock_transaction)
            .await
            .map_err(|error| error.to_string())?;
    if !claimed {
        return Ok(());
    }
    let result =
        reconcile_claimed_account(database, gateway, account, reconciliation_alerts_enabled).await;
    if let Err(code) = result {
        if let Err(error) = record_failed_account_reconciliation(database, &account.id, code).await
        {
            error!(%error, account_id = %account.id, failure_code = code, "failed to persist account reconciliation failure");
        }
    }
    lock_transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    result.map_err(str::to_owned)
}

async fn reconcile_claimed_account(
    database: &DatabasePool,
    gateway: &GatewayClient,
    account: &StoredAccount,
    reconciliation_alerts_enabled: bool,
) -> Result<(), &'static str> {
    let account_ref = account_ref(account)?;
    let local_orders_before = local_order_summaries(database, account).await?;
    let response = gateway
        .account_snapshot(proto::GetAccountSnapshotRequest {
            account: Some(account_ref.clone()),
            request_id: Uuid::now_v7().to_string(),
        })
        .await
        .map_err(gateway_failure_code)?;
    validate_account_ref(response.account.as_ref(), &account_ref)?;
    let observed_at = Utc
        .timestamp_millis_opt(response.gateway_received_at_ms)
        .single()
        .ok_or("gateway_account_snapshot_time_invalid")?;
    let local_orders_after = local_order_summaries(database, account).await?;
    if local_orders_before != local_orders_after {
        return Err("account_local_orders_changed_during_snapshot");
    }
    record_account_reconciliation(
        database,
        NewAccountSnapshot {
            account_id: account.id.clone(),
            exchange: account.exchange.clone(),
            balances: balances_json(response.balances),
            positions: positions_json(response.positions)?,
            open_orders: open_orders_json(response.open_orders, &account_ref)?,
            observed_at,
        },
        &local_orders_before,
        reconciliation_alerts_enabled,
    )
    .await
    .map_err(|_| "account_reconciliation_persist_failed")?;
    Ok(())
}

async fn local_order_summaries(
    database: &DatabasePool,
    account: &StoredAccount,
) -> Result<Vec<LocalOpenOrderSummary>, &'static str> {
    list_account_open_orders(database, &account.id, &account.exchange)
        .await
        .map_err(|_| "account_local_orders_query_failed")?
        .into_iter()
        .map(|order| {
            if order.exchange != account.exchange {
                return Err("account_local_order_exchange_mismatch");
            }
            Ok(LocalOpenOrderSummary {
                client_order_id: order.client_order_id,
                state: order.status.as_str().to_owned(),
            })
        })
        .collect()
}

fn account_ref(account: &StoredAccount) -> Result<proto::AccountRef, &'static str> {
    let exchange = match account.exchange.as_str() {
        "okx" => proto::Exchange::Okx as i32,
        _ => return Err("account_exchange_not_supported"),
    };
    let environment = match account.environment.as_str() {
        "demo" => proto::AccountEnvironment::Demo as i32,
        "production" => proto::AccountEnvironment::Production as i32,
        _ => return Err("account_environment_invalid"),
    };
    Ok(proto::AccountRef {
        account_id: account.id.clone(),
        exchange,
        environment,
    })
}

fn validate_account_ref(
    observed: Option<&proto::AccountRef>,
    expected: &proto::AccountRef,
) -> Result<(), &'static str> {
    match observed {
        Some(observed) if observed == expected => Ok(()),
        _ => Err("gateway_account_snapshot_scope_mismatch"),
    }
}

fn balances_json(balances: Vec<proto::Balance>) -> Value {
    Value::Array(
        balances
            .into_iter()
            .map(|balance| {
                json!({
                    "asset": balance.asset,
                    "total": balance.total,
                    "available": balance.available,
                })
            })
            .collect(),
    )
}

fn positions_json(positions: Vec<proto::Position>) -> Result<Value, &'static str> {
    positions
        .into_iter()
        .map(|position| {
            let instrument = position
                .instrument
                .ok_or("gateway_account_position_instrument_missing")?;
            if instrument.exchange != proto::Exchange::Okx as i32 {
                return Err("gateway_account_position_exchange_mismatch");
            }
            Ok(json!({
                "instrument": instrument_json(instrument),
                "quantity": position.quantity,
                "entry_price": position.entry_price,
                "mark_price": position.mark_price,
                "unrealized_pnl": position.unrealized_pnl,
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn open_orders_json(
    orders: Vec<proto::OrderSnapshot>,
    expected_account: &proto::AccountRef,
) -> Result<Value, &'static str> {
    orders
        .into_iter()
        .map(|order| {
            let account = order.account.ok_or("gateway_open_order_account_missing")?;
            validate_account_ref(Some(&account), expected_account)?;
            let instrument = order
                .instrument
                .ok_or("gateway_open_order_instrument_missing")?;
            if instrument.exchange != expected_account.exchange {
                return Err("gateway_open_order_exchange_mismatch");
            }
            let state = order_state_text(order.state)?;
            Ok(json!({
                "account": account_ref_json(account),
                "instrument": instrument_json(instrument),
                "client_order_id": order.client_order_id,
                "exchange_order_id": order.exchange_order_id,
                "state": state,
                "side": order.side,
                "quantity": order.quantity,
                "filled_quantity": order.filled_quantity,
                "average_price": order.average_price,
                "reason_code": order.reason_code,
                "exchange_time_ms": order.exchange_time_ms.to_string(),
                "gateway_received_at_ms": order.gateway_received_at_ms.to_string(),
            }))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn account_ref_json(account: proto::AccountRef) -> Value {
    json!({
        "account_id": account.account_id,
        "exchange": account.exchange,
        "environment": account.environment,
    })
}

fn instrument_json(instrument: proto::InstrumentKey) -> Value {
    json!({
        "exchange": instrument.exchange,
        "market_kind": instrument.market_kind,
        "symbol": instrument.symbol,
    })
}

fn order_state_text(value: i32) -> Result<&'static str, &'static str> {
    match proto::OrderState::try_from(value).ok() {
        Some(proto::OrderState::SubmissionPending) => Ok("submission_pending"),
        Some(proto::OrderState::SubmissionUnknown) => Ok("submission_unknown"),
        Some(proto::OrderState::Open) => Ok("open"),
        Some(proto::OrderState::PartiallyFilled) => Ok("partially_filled"),
        Some(proto::OrderState::CancelPending) => Ok("cancel_pending"),
        _ => Err("gateway_open_order_state_invalid"),
    }
}

fn gateway_failure_code(error: GatewayClientError) -> &'static str {
    match error {
        GatewayClientError::Transport(_) => "account_gateway_transport_failed",
        GatewayClientError::Rpc(status) => match status.code() {
            Code::Unauthenticated | Code::PermissionDenied => {
                "account_gateway_authentication_failed"
            }
            Code::ResourceExhausted => "account_gateway_rate_limited",
            Code::FailedPrecondition => "account_gateway_not_configured",
            _ => "account_gateway_snapshot_failed",
        },
        _ => "account_gateway_protocol_invalid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_supported_account_scope_is_accepted() {
        let mut account = sample_account();
        assert_eq!(
            account_ref(&account).unwrap().exchange,
            proto::Exchange::Okx as i32
        );
        account.exchange = "binance".to_owned();
        assert_eq!(account_ref(&account), Err("account_exchange_not_supported"));
    }

    #[test]
    fn open_order_states_must_be_non_terminal() {
        assert_eq!(order_state_text(proto::OrderState::Open as i32), Ok("open"));
        assert_eq!(
            order_state_text(proto::OrderState::Filled as i32),
            Err("gateway_open_order_state_invalid")
        );
    }

    fn sample_account() -> StoredAccount {
        let now = Utc::now();
        StoredAccount {
            id: "okx-demo".to_owned(),
            exchange: "okx".to_owned(),
            label: "OKX demo".to_owned(),
            environment: "demo".to_owned(),
            secret_name: "okx-demo-secret".to_owned(),
            enabled: true,
            withdrawals_disabled: true,
            ip_restricted: true,
            created_by: "test".to_owned(),
            created_at: now,
            updated_at: now,
        }
    }
}
