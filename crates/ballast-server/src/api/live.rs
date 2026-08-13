use axum::{Json, extract::State};
use ballast_storage::{StoredAccount, StoredReconciliationRun};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::Serialize;
use serde_json::{Value, json};

use crate::AppState;

use super::{ApiError, ApiResult, util};

#[derive(Debug, Serialize)]
pub(super) struct NativeAlgorithmCapabilityView {
    exchange: &'static str,
    algorithm: String,
    market_kind: &'static str,
    submit: bool,
    query: bool,
    cancel: bool,
    list_sub_orders: bool,
    fill_reconciliation: bool,
    protected_price: bool,
    validation_status: String,
    reason_code: Option<String>,
}

pub(super) async fn native_algorithm_capabilities(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<NativeAlgorithmCapabilityView>>> {
    let responses = join_all(
        util::exchanges()
            .iter()
            .copied()
            .map(|exchange| state.gateway.algorithmic_capabilities(exchange)),
    )
    .await;
    let mut result = Vec::new();
    for (exchange, response) in util::exchanges().iter().copied().zip(responses) {
        let response = response.map_err(ApiError::gateway)?;
        for capability in response.algorithms {
            let market_kind =
                match ballast_gateway_client::proto::MarketKind::try_from(capability.market_kind)
                    .ok()
                {
                    Some(ballast_gateway_client::proto::MarketKind::Spot) => "spot",
                    Some(ballast_gateway_client::proto::MarketKind::Perpetual) => "perpetual",
                    _ => "unspecified",
                };
            result.push(NativeAlgorithmCapabilityView {
                exchange: util::exchange_text(exchange),
                algorithm: capability.algorithm,
                market_kind,
                submit: capability.submit,
                query: capability.query,
                cancel: capability.cancel,
                list_sub_orders: capability.list_sub_orders,
                fill_reconciliation: capability.fill_reconciliation,
                protected_price: capability.protected_price,
                validation_status: capability.validation_status,
                reason_code: capability.reason_code,
            });
        }
    }
    Ok(Json(result))
}

pub(super) async fn live_readiness(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "enabled": false,
        "private_services": "disabled",
        "oidc": state.auth.readiness_status(),
        "limits": "zero_default",
        "required": [
            "oidc_validation",
            "read_only_account_reconciliation",
            "private_stream_recovery",
            "risk_limits",
            "production_switch"
        ]
    }))
}

#[derive(Debug, Serialize)]
pub(super) struct AccountView {
    id: String,
    exchange: String,
    label: String,
    environment: String,
    enabled: bool,
    withdrawals_disabled: bool,
    ip_restricted: bool,
    latest_reconciliation: Option<ReconciliationStatusView>,
}

#[derive(Debug, Serialize)]
struct ReconciliationStatusView {
    id: uuid::Uuid,
    status: String,
    difference_count: i32,
    failure_code: Option<String>,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

pub(super) async fn list_accounts(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<AccountView>>> {
    let accounts = ballast_storage::list_accounts(&state.database)
        .await
        .map_err(ApiError::database)?;
    let latest_runs = join_all(accounts.iter().map(|account| {
        ballast_storage::list_recent_reconciliation_runs(&state.database, &account.id, 1)
    }))
    .await;
    let mut result = Vec::with_capacity(accounts.len());
    for (account, runs) in accounts.into_iter().zip(latest_runs) {
        let latest_reconciliation = runs.map_err(ApiError::database)?.into_iter().next();
        result.push(account_view(account, latest_reconciliation));
    }
    Ok(Json(result))
}

fn account_view(
    account: StoredAccount,
    latest_reconciliation: Option<StoredReconciliationRun>,
) -> AccountView {
    AccountView {
        id: account.id,
        exchange: account.exchange,
        label: account.label,
        environment: account.environment,
        enabled: account.enabled,
        withdrawals_disabled: account.withdrawals_disabled,
        ip_restricted: account.ip_restricted,
        latest_reconciliation: latest_reconciliation.map(|run| ReconciliationStatusView {
            id: run.id,
            status: run.status,
            difference_count: run.difference_count,
            failure_code: run.failure_code,
            started_at: run.started_at,
            completed_at: run.completed_at,
        }),
    }
}

pub(super) async fn private_plane_locked() -> ApiResult<Json<Value>> {
    Err(ApiError::locked("live_execution_disabled"))
}
