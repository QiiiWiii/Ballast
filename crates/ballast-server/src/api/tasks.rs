use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use ballast_core::{MarketKind, QuantityUnit, Side};
use ballast_storage::{NewExecutionTask, StoredExecutionSlice, StoredExecutionTask};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, ApiResult, util};

#[derive(Debug, Deserialize)]
pub(super) struct CreateTaskRequest {
    instrument_id: Uuid,
    side: Side,
    target_amount: String,
    template_version_id: Uuid,
}

pub(super) async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateTaskRequest>,
) -> ApiResult<(StatusCode, Json<TaskView>)> {
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 200)
        .ok_or_else(|| ApiError::validation("idempotency_key_required", json!({})))?;
    let target_amount = util::positive_decimal(&request.target_amount, "target_amount")?;
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
    let start_at = Utc::now();
    let duration = chrono::Duration::seconds(template_version.duration_seconds);
    let deadline_at = start_at + duration;
    let strategy_params = json!({
        "participation_rate": template_version.participation_rate.map(|value| value.to_string()),
        "max_slice_amount": template_version.max_slice_amount.map(|value| value.to_string()),
    });
    let expected_strategy_params = strategy_params.clone();
    let task = ballast_storage::create_task(
        &state.database,
        NewExecutionTask {
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
