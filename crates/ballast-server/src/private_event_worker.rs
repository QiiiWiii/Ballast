use std::{str::FromStr, time::Duration};

use ballast_gateway_client::{
    GatewayClient, GatewayClientError, execution_order_snapshot_from_proto, proto,
};
use ballast_storage::{
    ChildOrderStateUpdate, DatabasePool, NewFill, compare_and_set_child_order_state,
    get_child_order_by_client_order_id, get_instrument, get_task, list_accounts, record_fill,
    refresh_slice_fees,
};
use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use tonic::Code;
use tracing::{error, warn};
use uuid::Uuid;

const RETRY_INTERVAL: Duration = Duration::from_secs(5);

pub fn spawn_worker(database: DatabasePool, gateway: GatewayClient) {
    tokio::spawn(async move {
        let accounts = match list_accounts(&database).await {
            Ok(accounts) => accounts,
            Err(error) => {
                error!(%error, "failed to list accounts for private event streams");
                return;
            }
        };
        for account in accounts
            .into_iter()
            .filter(|account| account.enabled && account.exchange == "okx")
        {
            let database = database.clone();
            let gateway = gateway.clone();
            tokio::spawn(async move {
                run_account_stream(database, gateway, account.id, account.environment).await;
            });
        }
    });
}

async fn run_account_stream(
    database: DatabasePool,
    gateway: GatewayClient,
    account_id: String,
    environment: String,
) {
    let environment = match account_environment(&environment) {
        Ok(value) => value,
        Err(error) => {
            warn!(account_id = %account_id, %error, "private event stream account environment is invalid");
            return;
        }
    };
    let mut backoff = RETRY_INTERVAL;
    loop {
        let request = proto::WatchOrderEventsRequest {
            account: Some(proto::AccountRef {
                account_id: account_id.clone(),
                exchange: proto::Exchange::Okx as i32,
                environment,
            }),
            request_id: Uuid::now_v7().to_string(),
        };
        match gateway.watch_order_events(request).await {
            Ok(mut stream) => {
                backoff = RETRY_INTERVAL;
                loop {
                    match stream.message().await {
                        Ok(Some(event)) => {
                            if let Err(error) =
                                handle_event(&database, &account_id, environment, event).await
                            {
                                warn!(account_id = %account_id, %error, "private order event rejected");
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            warn!(account_id = %account_id, %error, "private order event stream failed");
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                if !is_retryable(&error) {
                    warn!(account_id = %account_id, %error, "private order event stream unavailable");
                }
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

async fn handle_event(
    database: &DatabasePool,
    account_id: &str,
    environment: i32,
    event: proto::OrderEvent,
) -> Result<(), String> {
    match event.payload {
        Some(proto::order_event::Payload::Order(order)) => {
            handle_order(database, account_id, environment, order).await
        }
        Some(proto::order_event::Payload::Fill(fill)) => {
            handle_fill(database, account_id, fill).await
        }
        Some(proto::order_event::Payload::Status(_)) | None => Ok(()),
    }
}

async fn handle_order(
    database: &DatabasePool,
    account_id: &str,
    environment: i32,
    order: proto::OrderSnapshot,
) -> Result<(), String> {
    if order.client_order_id.trim().is_empty() {
        return Err("private_order_event_client_id_invalid".to_owned());
    }
    let child =
        get_child_order_by_client_order_id(database, "okx", account_id, &order.client_order_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "private_order_event_not_locally_known".to_owned())?;
    let task = get_task(database, child.task_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "private_order_event_task_not_found".to_owned())?;
    let instrument = get_instrument(database, task.instrument_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "private_order_event_instrument_not_found".to_owned())?;
    let response_account = order
        .account
        .as_ref()
        .ok_or_else(|| "private_order_event_account_missing".to_owned())?;
    if response_account.account_id != account_id
        || response_account.exchange != proto::Exchange::Okx as i32
        || response_account.environment != environment
    {
        return Err("private_order_event_account_mismatch".to_owned());
    }
    let response_instrument = order
        .instrument
        .as_ref()
        .ok_or_else(|| "private_order_event_instrument_missing".to_owned())?;
    let expected_market_kind = match instrument.instrument.id.market_kind {
        ballast_core::MarketKind::Spot => proto::MarketKind::Spot as i32,
        ballast_core::MarketKind::Perpetual => proto::MarketKind::Perpetual as i32,
    };
    if response_instrument.exchange != proto::Exchange::Okx as i32
        || response_instrument.market_kind != expected_market_kind
        || response_instrument.symbol != instrument.instrument.id.symbol
    {
        return Err("private_order_event_instrument_mismatch".to_owned());
    }
    let expected_side = match task.side.as_str() {
        "buy" => proto::Side::Buy as i32,
        "sell" => proto::Side::Sell as i32,
        _ => return Err("private_order_event_side_invalid".to_owned()),
    };
    if order.side != expected_side {
        return Err("private_order_event_side_mismatch".to_owned());
    }
    let response_quantity = Decimal::from_str(&order.quantity)
        .map_err(|_| "private_order_event_quantity_invalid".to_owned())?;
    if response_quantity != child.quantity {
        return Err("private_order_event_quantity_mismatch".to_owned());
    }
    let snapshot =
        execution_order_snapshot_from_proto(order.clone()).map_err(|error| error.to_string())?;
    let snapshot_state = snapshot.state;
    let snapshot_exchange_order_id = snapshot.exchange_order_id.clone();
    let snapshot_observed_at = snapshot.observed_at;
    let updated = compare_and_set_child_order_state(
        database,
        &child.exchange,
        &child.account_id,
        &child.client_order_id,
        &[child.status],
        &ChildOrderStateUpdate {
            status: snapshot_state,
            exchange_order_id: snapshot_exchange_order_id,
            filled_quantity: snapshot.filled_quantity,
            average_price: snapshot.average_price,
            limit_price: child.limit_price,
            raw_status: None,
            state_reason: order.reason_code,
            submitted_at: child.submitted_at,
            last_reconciled_at: Some(snapshot_observed_at),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    if updated.status.is_terminal() {
        ballast_storage::mark_task_state(database, updated.task_id, "running", None, Utc::now())
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn handle_fill(
    database: &DatabasePool,
    account_id: &str,
    fill: proto::Fill,
) -> Result<(), String> {
    let client_order_id = fill.client_order_id.trim();
    if client_order_id.is_empty() {
        return Err("private_fill_client_id_invalid".to_owned());
    }
    let child = get_child_order_by_client_order_id(database, "okx", account_id, client_order_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "private_fill_not_locally_known".to_owned())?;
    let price =
        Decimal::from_str(&fill.price).map_err(|_| "private_fill_price_invalid".to_owned())?;
    let quantity = Decimal::from_str(&fill.quantity)
        .map_err(|_| "private_fill_quantity_invalid".to_owned())?;
    let fee = fill
        .fee
        .as_deref()
        .unwrap_or("0")
        .parse::<Decimal>()
        .map_err(|_| "private_fill_fee_invalid".to_owned())?;
    let fee_status = if fill.fee.is_some() {
        "calculated"
    } else {
        "unavailable"
    };
    let exchange_time = Utc
        .timestamp_millis_opt(fill.exchange_time_ms)
        .single()
        .ok_or_else(|| "private_fill_time_invalid".to_owned())?;
    record_fill(
        database,
        &NewFill {
            child_order_id: child.id,
            exchange: child.exchange,
            account_id: child.account_id,
            exchange_trade_id: fill.exchange_trade_id,
            price,
            quantity,
            fee,
            fee_status,
            fee_asset: fill.fee_asset,
            exchange_time,
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    refresh_slice_fees(database, child.id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn account_environment(value: &str) -> Result<i32, &'static str> {
    match value {
        "demo" => Ok(proto::AccountEnvironment::Demo as i32),
        "production" => Ok(proto::AccountEnvironment::Production as i32),
        _ => Err("private_event_account_environment_invalid"),
    }
}

fn is_retryable(error: &GatewayClientError) -> bool {
    match error {
        GatewayClientError::Transport(_) => true,
        GatewayClientError::Rpc(status) => matches!(
            status.code(),
            Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted
        ),
        _ => false,
    }
}
