use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use ballast_core::{Instrument, MarketKind, QuantityUnit, Side};
use ballast_storage::{NewExecutionTask, StoredExecutionSlice, StoredExecutionTask};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{RequestAuth, Role};
use crate::risk::{self, RiskContext, RiskError, RiskStage};
use ballast_gateway_client::execution_order_snapshot_from_proto;

use super::{ApiError, ApiResult, util};

#[derive(Debug, Deserialize)]
pub(super) struct CreateTaskRequest {
    instrument_id: Uuid,
    side: Side,
    target_amount: String,
    template_version_id: Uuid,
    execution_mode: Option<String>,
    account_id: Option<String>,
}

pub(super) async fn create_task(
    State(state): State<AppState>,
    auth: RequestAuth,
    headers: HeaderMap,
    Json(request): Json<CreateTaskRequest>,
) -> ApiResult<(StatusCode, Json<TaskView>)> {
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 200)
        .ok_or_else(|| ApiError::validation("idempotency_key_required", json!({})))?;
    let target_amount = util::positive_decimal(&request.target_amount, "target_amount")?;
    let execution_mode = request.execution_mode.as_deref().unwrap_or("paper");
    if !matches!(execution_mode, "paper" | "live") {
        return Err(ApiError::validation(
            "execution_mode_invalid",
            json!({ "execution_mode": execution_mode }),
        ));
    }
    let requested_by = if execution_mode == "live" {
        if !state.live_enabled {
            return Err(ApiError::locked("live_execution_disabled"));
        }
        let principal = auth
            .require_role(Role::Operator)?
            .ok_or_else(|| ApiError::unauthorized("live_requires_oidc"))?;
        Some(principal.subject.clone())
    } else {
        None
    };
    let template_version = ballast_storage::get_strategy_template_version(
        &state.database,
        request.template_version_id,
    )
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("strategy_template_version_not_found"))?;
    let template =
        ballast_storage::get_strategy_template(&state.database, template_version.template_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
    if template.status != "active" {
        return Err(ApiError::conflict("strategy_template_archived"));
    }
    if template_version.execution_backend != "managed_ioc" {
        return Err(ApiError::conflict("live_execution_disabled"));
    }
    let strategy = util::parse_strategy_kind(&template_version.strategy_kind)?;
    let quantity_unit = util::parse_quantity_unit(&template_version.quantity_unit)?;
    let instrument = ballast_storage::get_instrument(&state.database, request.instrument_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("instrument_not_found"))?;
    if quantity_unit == QuantityUnit::Contracts
        && instrument.instrument.id.market_kind == MarketKind::Spot
    {
        return Err(ApiError::validation(
            "quantity_unit_not_supported",
            json!({ "quantity_unit": "contracts" }),
        ));
    }
    let account = if execution_mode == "live" {
        let account_id = request
            .account_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ApiError::validation("account_id_required", json!({})))?;
        let account = ballast_storage::get_account(&state.database, account_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("account_not_found"))?;
        if !account.enabled {
            return Err(ApiError::conflict("account_disabled"));
        }
        if !account.withdrawals_disabled || !account.ip_restricted {
            return Err(ApiError::conflict("account_safety_metadata_invalid"));
        }
        if account.exchange != util::exchange_text(instrument.instrument.id.exchange) {
            return Err(ApiError::validation(
                "account_exchange_mismatch",
                json!({ "account_id": account.id }),
            ));
        }
        if account.exchange != "okx" {
            return Err(ApiError::conflict("live_order_exchange_unsupported"));
        }
        Some((account.id, account.exchange))
    } else {
        None
    };
    let start_at = Utc::now();
    let duration = chrono::Duration::seconds(template_version.duration_seconds);
    let deadline_at = start_at + duration;
    let strategy_params = json!({
        "participation_rate": template_version.participation_rate.map(|value| value.to_string()),
        "max_slice_amount": template_version.max_slice_amount.map(|value| value.to_string()),
    });
    let expected_strategy_params = strategy_params.clone();
    let task_id = Uuid::now_v7();
    if execution_mode == "live" {
        let (account_id, exchange) = account.as_ref().expect("live account is validated");
        check_live_risk(
            &state,
            RiskStage::Create,
            task_id,
            account_id,
            exchange,
            request.instrument_id,
            &instrument.instrument,
            request.side,
            target_amount,
            quantity_unit,
            template_version.max_slippage_bps,
            template_version.duration_seconds,
        )
        .await?;
    }
    let task = ballast_storage::create_task(
        &state.database,
        NewExecutionTask {
            id: task_id,
            account_id: account
                .as_ref()
                .map_or_else(|| "paper".to_owned(), |value| value.0.clone()),
            execution_mode: execution_mode.to_owned(),
            instrument_id: request.instrument_id,
            template_version_id: request.template_version_id,
            idempotency_key: idempotency_key.to_owned(),
            side: request.side,
            strategy_kind: strategy,
            strategy_params,
            requested_amount: target_amount,
            quantity_unit,
            max_slippage_bps: template_version.max_slippage_bps,
            slice_interval_ms: template_version.slice_interval_ms,
            start_at,
            deadline_at,
            requested_by: requested_by.clone(),
        },
    )
    .await
    .map_err(ApiError::database)?;
    if task.instrument_id != request.instrument_id
        || task.template_version_id != request.template_version_id
        || task.side != util::side_text(request.side)
        || task.strategy_kind != util::strategy_text(strategy)
        || task.requested_amount != target_amount
        || task.quantity_unit != util::quantity_unit_text(quantity_unit)
        || task.strategy_params != expected_strategy_params
        || task.max_slippage_bps != template_version.max_slippage_bps
        || task.slice_interval_ms != template_version.slice_interval_ms
        || task.account_id != account.as_ref().map_or("paper", |value| value.0.as_str())
        || task.execution_mode != execution_mode
        || task.requested_by != requested_by
    {
        return Err(ApiError::conflict("idempotency_key_conflict"));
    }
    Ok((StatusCode::CREATED, Json(TaskView::from(task))))
}

#[derive(Debug, Deserialize)]
pub(super) struct ListTaskQuery {
    limit: Option<i64>,
}

pub(super) async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTaskQuery>,
) -> ApiResult<Json<Vec<TaskView>>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    Ok(Json(
        ballast_storage::list_tasks(&state.database, limit)
            .await
            .map_err(ApiError::database)?
            .into_iter()
            .map(TaskView::from)
            .collect(),
    ))
}

pub(super) async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Json<TaskView>> {
    let task = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    Ok(Json(TaskView::from(task)))
}

pub(super) async fn cancel_task(
    State(state): State<AppState>,
    auth: RequestAuth,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Json<TaskView>> {
    let existing = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    if !matches!(
        existing.status.as_str(),
        "completed" | "cancelled" | "expired" | "failed"
    ) {
        if existing.execution_mode == "live" {
            auth.require_role(Role::Operator)?
                .ok_or_else(|| ApiError::unauthorized("live_requires_oidc"))?;
            cancel_live_children(&state, &existing).await?;
        }
        ballast_storage::cancel_task(&state.database, task_id)
            .await
            .map_err(ApiError::database)?;
    }
    let task = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    Ok(Json(TaskView::from(task)))
}

async fn cancel_live_children(state: &AppState, task: &StoredExecutionTask) -> ApiResult<()> {
    let children = ballast_storage::list_task_open_orders(&state.database, task.id)
        .await
        .map_err(ApiError::database)?;
    if children.is_empty() {
        return Ok(());
    }
    let account = ballast_storage::get_account(&state.database, &task.account_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("account_not_found"))?;
    let instrument = ballast_storage::get_instrument(&state.database, task.instrument_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("instrument_not_found"))?;
    let exchange = match instrument.instrument.id.exchange {
        ballast_core::Exchange::Okx => ballast_gateway_client::proto::Exchange::Okx as i32,
        _ => return Err(ApiError::conflict("live_order_exchange_unsupported")),
    };
    let market_kind = match instrument.instrument.id.market_kind {
        MarketKind::Spot => ballast_gateway_client::proto::MarketKind::Spot as i32,
        MarketKind::Perpetual => ballast_gateway_client::proto::MarketKind::Perpetual as i32,
    };
    let environment = match account.environment.as_str() {
        "demo" => ballast_gateway_client::proto::AccountEnvironment::Demo as i32,
        "production" => ballast_gateway_client::proto::AccountEnvironment::Production as i32,
        _ => return Err(ApiError::conflict("account_environment_invalid")),
    };
    for child in children {
        if matches!(
            child.status,
            ballast_execution::OrderState::SubmissionPending
                | ballast_execution::OrderState::SubmissionUnknown
        ) {
            return Err(ApiError::conflict("live_order_reconciliation_required"));
        }
        ballast_storage::compare_and_set_child_order_state(
            &state.database,
            &child.exchange,
            &child.account_id,
            &child.client_order_id,
            &[child.status],
            &ballast_storage::ChildOrderStateUpdate {
                status: ballast_execution::OrderState::CancelPending,
                exchange_order_id: child.exchange_order_id.clone(),
                filled_quantity: child.filled_quantity,
                average_price: child.average_price,
                limit_price: child.limit_price,
                raw_status: None,
                state_reason: Some("cancel_requested".to_owned()),
                submitted_at: child.submitted_at,
                last_reconciled_at: Some(Utc::now()),
            },
        )
        .await
        .map_err(ApiError::database)?;
        let response = state
            .gateway
            .cancel_order(ballast_gateway_client::proto::CancelOrderRequest {
                account: Some(ballast_gateway_client::proto::AccountRef {
                    account_id: account.id.clone(),
                    exchange,
                    environment,
                }),
                instrument: Some(ballast_gateway_client::proto::InstrumentKey {
                    exchange,
                    market_kind,
                    symbol: instrument.instrument.id.symbol.clone(),
                }),
                request_id: Uuid::now_v7().to_string(),
                client_order_id: child.client_order_id.clone(),
            })
            .await
            .map_err(ApiError::gateway)?;
        if response.client_order_id != child.client_order_id {
            return Err(ApiError::conflict("live_cancel_response_mismatch"));
        }
        let snapshot = execution_order_snapshot_from_proto(response.clone())
            .map_err(|_| ApiError::conflict("live_cancel_response_invalid"))?;
        let snapshot_state = snapshot.state;
        let snapshot_observed_at = snapshot.observed_at;
        ballast_storage::compare_and_set_child_order_state(
            &state.database,
            &child.exchange,
            &child.account_id,
            &child.client_order_id,
            &[ballast_execution::OrderState::CancelPending],
            &ballast_storage::ChildOrderStateUpdate {
                status: snapshot_state,
                exchange_order_id: snapshot.exchange_order_id.clone(),
                filled_quantity: snapshot.filled_quantity,
                average_price: snapshot.average_price,
                limit_price: child.limit_price,
                raw_status: None,
                state_reason: response.reason_code,
                submitted_at: child.submitted_at,
                last_reconciled_at: Some(snapshot_observed_at),
            },
        )
        .await
        .map_err(ApiError::database)?;
        if !snapshot_state.is_terminal() {
            return Err(ApiError::conflict("live_order_cancel_pending"));
        }
    }
    Ok(())
}

pub(super) async fn list_slices(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Json<Vec<SliceView>>> {
    if ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .is_none()
    {
        return Err(ApiError::not_found("task_not_found"));
    }
    Ok(Json(
        ballast_storage::list_slices(&state.database, task_id)
            .await
            .map_err(ApiError::database)?
            .into_iter()
            .map(SliceView::from)
            .collect(),
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn check_live_risk(
    state: &AppState,
    stage: RiskStage,
    task_id: Uuid,
    account_id: &str,
    exchange: &str,
    instrument_id: Uuid,
    instrument: &Instrument,
    side: Side,
    target_amount: rust_decimal::Decimal,
    quantity_unit: QuantityUnit,
    max_slippage_bps: i32,
    duration_seconds: i64,
) -> ApiResult<()> {
    let order_book = state
        .gateway
        .get_order_book(&instrument.id, 5)
        .await
        .map_err(ApiError::gateway)?;
    let reference_price = match side {
        Side::Buy => order_book.asks.first().map(|level| level.price),
        Side::Sell => order_book.bids.first().map(|level| level.price),
    }
    .ok_or_else(|| ApiError::conflict("live_market_side_unavailable"))?;
    let market_age_ms = (Utc::now().timestamp_millis() - order_book.gateway_received_at_ms).max(0);
    let native_quantity = instrument
        .target_to_native_quantity(target_amount, quantity_unit, reference_price)
        .map_err(|_| ApiError::conflict("live_quantity_conversion_failed"))?;
    let task_notional = instrument
        .native_to_quote_quantity(native_quantity, reference_price)
        .map_err(|_| ApiError::conflict("live_notional_conversion_failed"))?;
    let snapshot = risk::latest_snapshot(&state.database, account_id)
        .await
        .map_err(ApiError::database)?;
    let current_exposure = risk::snapshot_exposure(snapshot.as_ref(), &instrument.id.symbol)
        .map_err(ApiError::conflict)?;
    let account_age_ms = risk::snapshot_age_ms(snapshot.as_ref()).map_err(ApiError::conflict)?;
    let projected_exposure = instrument
        .native_to_base_quantity(native_quantity, reference_price)
        .map_err(|_| ApiError::conflict("live_exposure_conversion_failed"))?
        .abs();
    risk::check(
        &state.database,
        stage,
        &RiskContext {
            task_id: Some(task_id),
            account_id: account_id.to_owned(),
            exchange: exchange.to_owned(),
            instrument_id,
            backend: "managed_ioc".to_owned(),
            order_notional: task_notional,
            task_notional,
            market_age_ms,
            account_age_ms,
            max_slippage_bps,
            daily_notional: task_notional,
            net_exposure: current_exposure + projected_exposure,
            native_algo_orders: 0,
            native_duration_seconds: duration_seconds,
        },
    )
    .await
    .map_err(map_risk_error)
}

fn map_risk_error(error: RiskError) -> ApiError {
    match error {
        RiskError::Denied(code) => ApiError::conflict(code),
        RiskError::Database(error) => ApiError::database(error),
    }
}

#[derive(Debug, Serialize)]
pub(super) struct TaskView {
    id: Uuid,
    account_id: String,
    execution_mode: String,
    execution_backend: String,
    instrument_id: Uuid,
    template_version_id: Uuid,
    side: String,
    strategy: String,
    strategy_params: Value,
    requested_amount: String,
    quantity_unit: String,
    executed_amount: String,
    residual_amount: String,
    max_slippage_bps: i32,
    slice_interval_ms: i64,
    status: String,
    paused_reason: Option<String>,
    failure_code: Option<String>,
    requested_by: Option<String>,
    approved_by: Option<String>,
    started_at: Option<DateTime<Utc>>,
    deadline_at: DateTime<Utc>,
    next_tick_at: DateTime<Utc>,
    last_tick_at: Option<DateTime<Utc>>,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<StoredExecutionTask> for TaskView {
    fn from(value: StoredExecutionTask) -> Self {
        Self {
            id: value.id,
            account_id: value.account_id,
            execution_mode: value.execution_mode,
            execution_backend: value.execution_backend,
            instrument_id: value.instrument_id,
            template_version_id: value.template_version_id,
            side: value.side,
            strategy: value.strategy_kind,
            strategy_params: value.strategy_params,
            requested_amount: value.requested_amount.to_string(),
            quantity_unit: value.quantity_unit,
            executed_amount: value.executed_amount.to_string(),
            residual_amount: value.residual_amount.to_string(),
            max_slippage_bps: value.max_slippage_bps,
            slice_interval_ms: value.slice_interval_ms,
            status: value.status,
            paused_reason: value.paused_reason,
            failure_code: value.failure_code,
            requested_by: value.requested_by,
            approved_by: value.approved_by,
            started_at: value.started_at,
            deadline_at: value.deadline_at,
            next_tick_at: value.next_tick_at,
            last_tick_at: value.last_tick_at,
            version: value.version,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SliceView {
    id: Uuid,
    task_id: Uuid,
    sequence: i32,
    requested_amount: String,
    native_quantity: String,
    filled_native_quantity: String,
    filled_base_quantity: String,
    filled_quote_quantity: String,
    average_price: Option<String>,
    worst_price: Option<String>,
    slippage_bps: Option<String>,
    fee_amount: Option<String>,
    fee_asset: Option<String>,
    fee_status: String,
    status: String,
    market_snapshot: Value,
    decision_input: Value,
    created_at: DateTime<Utc>,
}

impl From<StoredExecutionSlice> for SliceView {
    fn from(value: StoredExecutionSlice) -> Self {
        Self {
            id: value.id,
            task_id: value.task_id,
            sequence: value.sequence,
            requested_amount: value.requested_amount.to_string(),
            native_quantity: value.native_quantity.to_string(),
            filled_native_quantity: value.filled_native_quantity.to_string(),
            filled_base_quantity: value.filled_base_quantity.to_string(),
            filled_quote_quantity: value.filled_quote_quantity.to_string(),
            average_price: util::decimal_option(value.average_price),
            worst_price: util::decimal_option(value.worst_price),
            slippage_bps: util::decimal_option(value.slippage_bps),
            fee_amount: util::decimal_option(value.fee_amount),
            fee_asset: value.fee_asset,
            fee_status: value.fee_status,
            status: value.status,
            market_snapshot: value.market_snapshot,
            decision_input: value.decision_input,
            created_at: value.created_at,
        }
    }
}
