use axum::{
    Json,
    extract::{Path, Query, State},
};
use ballast_storage::{StoredAccount, StoredReconciliationRun};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{RequestAuth, Role};
use crate::risk::RiskStage;

use super::{ApiError, ApiResult, tasks, util};

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

pub(super) async fn live_readiness(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let accounts = ballast_storage::list_accounts(&state.database)
        .await
        .map_err(ApiError::database)?;
    let limits = ballast_storage::list_risk_limits(&state.database)
        .await
        .map_err(ApiError::database)?;
    let switches = ballast_storage::list_kill_switches(&state.database)
        .await
        .map_err(ApiError::database)?;
    let private_stream_recovery = state
        .gateway
        .trading_capabilities(ballast_core::Exchange::Okx)
        .await
        .map(|capabilities| capabilities.private_order_stream && capabilities.private_fill_stream)
        .unwrap_or(false);
    let prerequisites = vec![
        (
            "oidc_validation",
            state.auth.readiness_status() == "configured",
        ),
        (
            "read_only_account_reconciliation",
            accounts
                .iter()
                .any(|account| account.enabled && account.exchange == "okx"),
        ),
        ("private_stream_recovery", private_stream_recovery),
        ("risk_limits", limits.len() >= 3),
        ("production_switch", state.live_enabled),
    ];
    Ok(Json(json!({
        "enabled": state.live_enabled,
        "private_services": if state.live_enabled { "enabled" } else { "locked" },
        "oidc": state.auth.readiness_status(),
        "limits": if limits.is_empty() { "zero_default" } else { "configured" },
        "kill_switches": {
            "configured": switches.len(),
            "enabled": switches.iter().filter(|switch| switch.enabled).count(),
        },
        "prerequisites": prerequisites.into_iter().map(|(name, ready)| json!({"name": name, "ready": ready})).collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize)]
pub(super) struct ApprovalQuery {
    limit: Option<i64>,
}

pub(super) async fn list_approvals(
    State(state): State<AppState>,
    auth: RequestAuth,
    Query(query): Query<ApprovalQuery>,
) -> ApiResult<Json<Value>> {
    auth.require_role(Role::Viewer)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let pending_tasks = ballast_storage::list_pending_approval_tasks(&state.database, limit)
        .await
        .map_err(ApiError::database)?;
    Ok(Json(json!({
        "items": pending_tasks.into_iter().map(tasks::TaskView::from).collect::<Vec<_>>(),
        "limit": limit,
    })))
}

pub(super) async fn approve_task(
    State(state): State<AppState>,
    auth: RequestAuth,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Json<tasks::TaskView>> {
    let principal = auth
        .require_role(Role::Admin)?
        .ok_or_else(|| ApiError::unauthorized("approval_requires_oidc"))?;
    let task = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    if task.execution_mode != "live" || task.status != "pending_approval" {
        return Err(ApiError::conflict("task_not_pending_approval"));
    }
    check_task_risk(&state, &task, RiskStage::Approve).await?;
    match ballast_storage::approve_task(&state.database, task_id, &principal.subject).await {
        Ok(true) => {}
        Ok(false) => return Err(ApiError::conflict("task_not_pending_approval")),
        Err(error) if matches!(&error, sqlx::Error::Protocol(message) if message == "self_approval_forbidden") =>
        {
            return Err(ApiError::forbidden("self_approval_forbidden"));
        }
        Err(error) => return Err(ApiError::database(error)),
    }
    let task = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    Ok(Json(tasks::TaskView::from(task)))
}

#[derive(Debug, Deserialize)]
pub(super) struct RejectTaskRequest {
    reason: String,
}

pub(super) async fn reject_task(
    State(state): State<AppState>,
    auth: RequestAuth,
    Path(task_id): Path<Uuid>,
    Json(request): Json<RejectTaskRequest>,
) -> ApiResult<Json<tasks::TaskView>> {
    let principal = auth
        .require_role(Role::Admin)?
        .ok_or_else(|| ApiError::unauthorized("approval_requires_oidc"))?;
    if request.reason.trim().is_empty() || request.reason.len() > 500 {
        return Err(ApiError::validation("rejection_reason_invalid", json!({})));
    }
    match ballast_storage::reject_task(
        &state.database,
        task_id,
        &principal.subject,
        request.reason.trim(),
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return Err(ApiError::conflict("task_not_pending_approval")),
        Err(error) if matches!(&error, sqlx::Error::Protocol(message) if message == "self_approval_forbidden") =>
        {
            return Err(ApiError::forbidden("self_approval_forbidden"));
        }
        Err(error) => return Err(ApiError::database(error)),
    }
    let task = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    Ok(Json(tasks::TaskView::from(task)))
}

async fn check_task_risk(
    state: &AppState,
    task: &ballast_storage::StoredExecutionTask,
    stage: RiskStage,
) -> ApiResult<()> {
    let instrument = ballast_storage::get_instrument(&state.database, task.instrument_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("instrument_not_found"))?;
    let account = ballast_storage::get_account(&state.database, &task.account_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("account_not_found"))?;
    let template =
        ballast_storage::get_strategy_template_version(&state.database, task.template_version_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("strategy_template_version_not_found"))?;
    let side = match task.side.as_str() {
        "buy" => ballast_core::Side::Buy,
        "sell" => ballast_core::Side::Sell,
        _ => return Err(ApiError::conflict("task_side_invalid")),
    };
    let quantity_unit = util::parse_quantity_unit(&task.quantity_unit)?;
    tasks::check_live_risk(
        state,
        stage,
        task.id,
        &account.id,
        &account.exchange,
        task.instrument_id,
        &instrument.instrument,
        side,
        task.requested_amount,
        quantity_unit,
        task.max_slippage_bps,
        template.duration_seconds,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub(super) struct KillSwitchRequest {
    scope_type: String,
    scope_id: String,
    enabled: bool,
    reason: String,
}

pub(super) async fn risk_state(
    State(state): State<AppState>,
    auth: RequestAuth,
) -> ApiResult<Json<Value>> {
    auth.require_role(Role::Viewer)?;
    let limits = ballast_storage::list_risk_limits(&state.database)
        .await
        .map_err(ApiError::database)?;
    let switches = ballast_storage::list_kill_switches(&state.database)
        .await
        .map_err(ApiError::database)?;
    let decisions = ballast_storage::list_risk_decisions(&state.database, 100)
        .await
        .map_err(ApiError::database)?;
    Ok(Json(
        json!({ "limits": limits, "kill_switches": switches, "decisions": decisions }),
    ))
}

pub(super) async fn set_kill_switch(
    State(state): State<AppState>,
    auth: RequestAuth,
    Json(request): Json<KillSwitchRequest>,
) -> ApiResult<Json<Value>> {
    let principal = auth
        .require_role(Role::Admin)?
        .ok_or_else(|| ApiError::unauthorized("risk_requires_oidc"))?;
    let switch = ballast_storage::upsert_kill_switch(
        &state.database,
        request.scope_type.trim(),
        request.scope_id.trim(),
        request.enabled,
        request.reason.trim(),
        &principal.subject,
    )
    .await
    .map_err(map_risk_storage_error)?;
    if switch.enabled {
        ballast_storage::enqueue_system_alert(
            &state.database,
            "kill_switch_changed",
            &switch.scope_id,
            "kill_switch_enabled",
        )
        .await
        .map_err(ApiError::database)?;
    }
    Ok(Json(
        serde_json::to_value(switch).expect("kill switch serializes"),
    ))
}

#[derive(Debug, Deserialize)]
pub(super) struct RiskLimitRequest {
    scope_type: String,
    scope_id: String,
    allowed_exchanges: Value,
    allowed_accounts: Value,
    allowed_instruments: Value,
    allowed_backends: Value,
    max_order_notional: String,
    max_task_notional: String,
    max_daily_notional: String,
    max_active_tasks: i32,
    max_slippage_bps: i32,
    max_market_age_ms: i64,
    max_account_age_ms: i64,
    max_net_exposure: String,
    max_native_algo_orders: i32,
    max_native_duration_seconds: i64,
}

pub(super) async fn set_risk_limit(
    State(state): State<AppState>,
    auth: RequestAuth,
    Json(request): Json<RiskLimitRequest>,
) -> ApiResult<Json<Value>> {
    let principal = auth
        .require_role(Role::Admin)?
        .ok_or_else(|| ApiError::unauthorized("risk_requires_oidc"))?;
    let max_order_notional =
        util::non_negative_decimal(&request.max_order_notional, "max_order_notional")?;
    let max_task_notional =
        util::non_negative_decimal(&request.max_task_notional, "max_task_notional")?;
    let max_daily_notional =
        util::non_negative_decimal(&request.max_daily_notional, "max_daily_notional")?;
    let max_net_exposure =
        util::non_negative_decimal(&request.max_net_exposure, "max_net_exposure")?;
    let limit = ballast_storage::upsert_risk_limit(
        &state.database,
        request.scope_type.trim(),
        request.scope_id.trim(),
        request.allowed_exchanges,
        request.allowed_accounts,
        request.allowed_instruments,
        request.allowed_backends,
        max_order_notional,
        max_task_notional,
        max_daily_notional,
        request.max_active_tasks,
        request.max_slippage_bps,
        request.max_market_age_ms,
        request.max_account_age_ms,
        max_net_exposure,
        request.max_native_algo_orders,
        request.max_native_duration_seconds,
        &principal.subject,
    )
    .await
    .map_err(map_risk_storage_error)?;
    Ok(Json(
        serde_json::to_value(limit).expect("risk limit serializes"),
    ))
}

fn map_risk_storage_error(error: sqlx::Error) -> ApiError {
    if matches!(&error, sqlx::Error::Protocol(message) if message == "risk_scope_invalid" || message == "risk_global_scope_invalid" || message == "risk_limit_invalid" || message == "kill_switch_metadata_invalid")
    {
        ApiError::validation("risk_configuration_invalid", json!({}))
    } else {
        ApiError::database(error)
    }
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
