use std::str::FromStr;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use ballast_core::{QuantityUnit, Side};
use ballast_simulator::{
    ReplayConfig, ReplayTrade, TRADE_VWAP_PROXY_VERSION, replay_trade_vwap_proxy,
};
use ballast_storage::{
    NewReplayMetrics, NewReplayRun, NewReplaySlice, StoredReplayMetrics, StoredReplayRun,
    StoredReplaySlice,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    ApiError, ApiResult, parse_exchange, parse_quantity_unit, parse_strategy_kind,
    quantity_unit_text, side_text, strategy_text,
};
use crate::AppState;

const TRADE_PAGE_SIZE: i64 = 5_000;
const MAX_REPLAY_TRADES: usize = 1_000_000;
const MAX_REPLAY_SLICES: i64 = 10_000;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/replays", post(create_replay))
        .route("/api/v1/replays/{run_id}", get(get_replay))
        .route("/api/v1/replays/{run_id}/slices", get(get_replay_slices))
        .route("/api/v1/replays/{run_id}/metrics", get(get_replay_metrics))
}

#[derive(Debug, Deserialize)]
struct CreateReplayRequest {
    exchange: String,
    market_kind: String,
    symbol: String,
    template_version_id: Uuid,
    side: String,
    requested_amount: String,
    quantity_unit: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    idempotency_key: String,
    #[serde(default = "default_execution_model")]
    execution_model: String,
    fee_rate: Option<String>,
    extra_slippage_bps: Option<String>,
    gap_threshold_seconds: Option<i64>,
}

fn default_execution_model() -> String {
    "trade_vwap_proxy".to_owned()
}

#[derive(Debug, Serialize)]
struct ReplayRunView {
    id: Uuid,
    idempotency_key: String,
    instrument_id: Uuid,
    template_version_id: Uuid,
    strategy_kind: String,
    side: String,
    requested_amount: String,
    quantity_unit: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    execution_model: String,
    model_version: String,
    fee_rate: String,
    extra_slippage_bps: String,
    gap_threshold_seconds: i64,
    strategy_snapshot: Value,
    status: String,
    failure_code: Option<String>,
    data_first_at: Option<DateTime<Utc>>,
    data_last_at: Option<DateTime<Utc>>,
    trade_count: i64,
    gap_count: i32,
    data_gaps: Value,
    confidence: String,
    limitations: Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ReplaySliceView {
    sequence: i32,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    status: String,
    trade_count: i64,
    market_volume: String,
    requested_amount: String,
    filled_amount: String,
    filled_native_quantity: String,
    market_vwap: Option<String>,
    simulated_price: Option<String>,
    fee_amount: String,
    decision_input: Value,
}

#[derive(Debug, Serialize)]
struct ReplayMetricsView {
    arrival_price: Option<String>,
    market_vwap: Option<String>,
    simulated_execution_vwap: Option<String>,
    implementation_shortfall_bps: Option<String>,
    implementation_shortfall_amount: Option<String>,
    requested_amount: String,
    filled_amount: String,
    fill_rate: String,
    residual_amount: String,
    actual_participation_rate: Option<String>,
    target_participation_rate: Option<String>,
    participation_rate_deviation: Option<String>,
    slice_count: i32,
    empty_window_count: i32,
    fee_amount: String,
    explicit_slippage_amount: String,
}

async fn create_replay(
    State(state): State<AppState>,
    Json(request): Json<CreateReplayRequest>,
) -> ApiResult<Json<ReplayRunView>> {
    validate_request_shape(&request)?;
    let request_fingerprint = request_fingerprint(&request);
    if let Some(existing) =
        ballast_storage::get_replay_run_by_key(&state.database, request.idempotency_key.trim())
            .await
            .map_err(ApiError::database)?
    {
        ensure_idempotency_match(&existing.request_fingerprint, &request_fingerprint)?;
        return Ok(Json(existing.into()));
    }

    let exchange = parse_exchange(request.exchange.trim())?;
    let market_kind = super::history::parse_market_kind(request.market_kind.trim())?;
    let instrument = ballast_storage::get_instrument_by_key(
        &state.database,
        exchange,
        market_kind,
        request.symbol.trim(),
    )
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("instrument_not_found"))?;
    let template = ballast_storage::get_strategy_template_version(
        &state.database,
        request.template_version_id,
    )
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("strategy_template_version_not_found"))?;
    let strategy_kind = parse_strategy_kind(&template.strategy_kind)?;
    let quantity_unit = parse_quantity_unit(request.quantity_unit.trim())?;
    if template.quantity_unit != quantity_unit_text(quantity_unit) {
        return Err(ApiError::validation(
            "replay_quantity_unit_mismatch",
            json!({
                "template_quantity_unit": template.quantity_unit,
            }),
        ));
    }
    if request.end_at - request.start_at != Duration::seconds(template.duration_seconds) {
        return Err(ApiError::validation(
            "replay_duration_mismatch",
            json!({
                "template_duration_seconds": template.duration_seconds,
            }),
        ));
    }
    if template.slice_interval_ms <= 0 {
        return Err(ApiError::validation(
            "replay_slice_interval_invalid",
            json!({}),
        ));
    }
    let duration_ms = (request.end_at - request.start_at).num_milliseconds();
    let slice_count = (duration_ms + template.slice_interval_ms - 1) / template.slice_interval_ms;
    if slice_count <= 0 || slice_count > MAX_REPLAY_SLICES {
        return Err(ApiError::validation(
            "replay_slice_count_invalid",
            json!({
                "maximum": MAX_REPLAY_SLICES,
            }),
        ));
    }
    let side = parse_side(request.side.trim())?;
    let requested_amount = positive_decimal(&request.requested_amount, "requested_amount")?;
    validate_quantity_unit(&instrument.instrument, quantity_unit)?;
    let fee_rate = request
        .fee_rate
        .as_deref()
        .map(|value| non_negative_decimal(value, "fee_rate"))
        .transpose()?
        .or(instrument.instrument.taker_fee_rate)
        .unwrap_or(Decimal::ZERO);
    let extra_slippage_bps = request
        .extra_slippage_bps
        .as_deref()
        .map(|value| non_negative_decimal(value, "extra_slippage_bps"))
        .transpose()?
        .unwrap_or(Decimal::ZERO);
    let gap_threshold_seconds = request
        .gap_threshold_seconds
        .unwrap_or_else(|| (template.slice_interval_ms / 500).max(60));
    if gap_threshold_seconds <= 0 {
        return Err(ApiError::validation("gap_threshold_invalid", json!({})));
    }

    let trades = load_trades(&state, instrument.id, request.start_at, request.end_at).await?;
    let strategy_snapshot = json!({
        "template_version_id": template.id,
        "template_id": template.template_id,
        "version": template.version,
        "strategy_kind": template.strategy_kind,
        "quantity_unit": template.quantity_unit,
        "duration_seconds": template.duration_seconds,
        "slice_interval_ms": template.slice_interval_ms,
        "participation_rate": template.participation_rate.map(|value| value.to_string()),
        "max_slice_amount": template.max_slice_amount.map(|value| value.to_string()),
        "execution_backend": template.execution_backend,
    });
    let result = replay_trade_vwap_proxy(
        &ReplayConfig {
            instrument: instrument.instrument,
            strategy_kind,
            side,
            requested_amount,
            quantity_unit,
            start_at: request.start_at,
            end_at: request.end_at,
            slice_interval: Duration::milliseconds(template.slice_interval_ms),
            participation_rate: template.participation_rate,
            max_slice_amount: template.max_slice_amount,
            fee_rate,
            extra_slippage_bps,
            gap_threshold: Duration::seconds(gap_threshold_seconds),
        },
        &trades,
    )
    .map_err(|error| {
        tracing::warn!(%error, "replay calculation rejected");
        ApiError::validation("replay_configuration_invalid", json!({}))
    })?;

    let stored = ballast_storage::create_replay_result(
        &state.database,
        &NewReplayRun {
            idempotency_key: request.idempotency_key.trim().to_owned(),
            instrument_id: instrument.id,
            template_version_id: template.id,
            strategy_kind: strategy_text(strategy_kind).to_owned(),
            side: side_text(side).to_owned(),
            requested_amount,
            quantity_unit: quantity_unit_text(quantity_unit).to_owned(),
            start_at: request.start_at,
            end_at: request.end_at,
            execution_model: request.execution_model,
            model_version: TRADE_VWAP_PROXY_VERSION.to_owned(),
            fee_rate,
            extra_slippage_bps,
            gap_threshold_seconds,
            strategy_snapshot,
            request_fingerprint: request_fingerprint.clone(),
            status: result.status,
            failure_code: result.failure_code,
            data_first_at: result.data_first_at,
            data_last_at: result.data_last_at,
            trade_count: result.trade_count,
            gap_count: result.gap_count,
            data_gaps: json!(
                result
                    .gaps
                    .into_iter()
                    .map(|gap| json!({ "start_at": gap.start_at, "end_at": gap.end_at }))
                    .collect::<Vec<_>>()
            ),
            confidence: result.confidence,
            limitations: result.limitations,
        },
        &result
            .slices
            .into_iter()
            .map(|slice| NewReplaySlice {
                sequence: slice.sequence,
                window_start: slice.window_start,
                window_end: slice.window_end,
                status: slice.status,
                trade_count: slice.trade_count,
                market_volume: slice.market_volume,
                requested_amount: slice.requested_amount,
                filled_amount: slice.filled_amount,
                filled_native_quantity: slice.filled_native_quantity,
                market_vwap: slice.market_vwap,
                simulated_price: slice.simulated_price,
                fee_amount: slice.fee_amount,
                decision_input: slice.decision_input,
            })
            .collect::<Vec<_>>(),
        &NewReplayMetrics {
            arrival_price: result.metrics.arrival_price,
            market_vwap: result.metrics.market_vwap,
            simulated_execution_vwap: result.metrics.simulated_execution_vwap,
            implementation_shortfall_bps: result.metrics.implementation_shortfall_bps,
            implementation_shortfall_amount: result.metrics.implementation_shortfall_amount,
            requested_amount: result.metrics.requested_amount,
            filled_amount: result.metrics.filled_amount,
            fill_rate: result.metrics.fill_rate,
            residual_amount: result.metrics.residual_amount,
            actual_participation_rate: result.metrics.actual_participation_rate,
            target_participation_rate: result.metrics.target_participation_rate,
            participation_rate_deviation: result.metrics.participation_rate_deviation,
            slice_count: result.metrics.slice_count,
            empty_window_count: result.metrics.empty_window_count,
            fee_amount: result.metrics.fee_amount,
            explicit_slippage_amount: result.metrics.explicit_slippage_amount,
        },
    )
    .await
    .map_err(ApiError::database)?;
    ensure_idempotency_match(&stored.request_fingerprint, &request_fingerprint)?;
    Ok(Json(stored.into()))
}

async fn get_replay(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<ReplayRunView>> {
    let run = ballast_storage::get_replay_run(&state.database, run_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("replay_not_found"))?;
    Ok(Json(run.into()))
}

async fn get_replay_slices(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<Vec<ReplaySliceView>>> {
    require_run(&state, run_id).await?;
    Ok(Json(
        ballast_storage::list_replay_slices(&state.database, run_id)
            .await
            .map_err(ApiError::database)?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

async fn get_replay_metrics(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> ApiResult<Json<ReplayMetricsView>> {
    require_run(&state, run_id).await?;
    let metrics = ballast_storage::get_replay_metrics(&state.database, run_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::internal("replay_metrics_missing"))?;
    Ok(Json(metrics.into()))
}

async fn require_run(state: &AppState, run_id: Uuid) -> ApiResult<()> {
    ballast_storage::get_replay_run(&state.database, run_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("replay_not_found"))?;
    Ok(())
}

async fn load_trades(
    state: &AppState,
    instrument_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> ApiResult<Vec<ReplayTrade>> {
    let mut result = Vec::new();
    let mut cursor = None;
    loop {
        let page = ballast_storage::list_historical_trade_page(
            &state.database,
            instrument_id,
            start_at,
            end_at,
            cursor.clone(),
            TRADE_PAGE_SIZE,
        )
        .await
        .map_err(ApiError::database)?;
        if page.is_empty() {
            break;
        }
        if result.len() + page.len() > MAX_REPLAY_TRADES {
            return Err(ApiError::validation(
                "replay_trade_limit_exceeded",
                json!({
                    "maximum": MAX_REPLAY_TRADES,
                }),
            ));
        }
        cursor = page
            .last()
            .map(|trade| (trade.trade_time, trade.exchange_trade_id.clone()));
        result.extend(page.into_iter().map(|trade| ReplayTrade {
            exchange_trade_id: trade.exchange_trade_id,
            trade_time: trade.trade_time,
            price: trade.price,
            native_quantity: trade.quantity,
        }));
        if result.len() % TRADE_PAGE_SIZE as usize != 0 {
            break;
        }
    }
    Ok(result)
}

fn validate_request_shape(request: &CreateReplayRequest) -> ApiResult<()> {
    if request.idempotency_key.trim().is_empty() {
        return Err(ApiError::validation("idempotency_key_required", json!({})));
    }
    if request.execution_model != "trade_vwap_proxy" {
        return Err(ApiError::validation("execution_model_invalid", json!({})));
    }
    if request.start_at >= request.end_at {
        return Err(ApiError::validation("time_range_invalid", json!({})));
    }
    Ok(())
}

fn request_fingerprint(request: &CreateReplayRequest) -> String {
    json!({
        "exchange": request.exchange.trim(),
        "market_kind": request.market_kind.trim(),
        "symbol": request.symbol.trim(),
        "template_version_id": request.template_version_id,
        "side": request.side.trim(),
        "requested_amount": request.requested_amount.trim(),
        "quantity_unit": request.quantity_unit.trim(),
        "start_at": request.start_at,
        "end_at": request.end_at,
        "execution_model": request.execution_model,
        "fee_rate": request.fee_rate.as_deref().map(str::trim),
        "extra_slippage_bps": request.extra_slippage_bps.as_deref().map(str::trim),
        "gap_threshold_seconds": request.gap_threshold_seconds,
    })
    .to_string()
}

fn ensure_idempotency_match(existing: &str, requested: &str) -> ApiResult<()> {
    if existing != requested {
        return Err(ApiError::conflict("replay_idempotency_key_conflict"));
    }
    Ok(())
}

fn parse_side(value: &str) -> ApiResult<Side> {
    match value {
        "buy" => Ok(Side::Buy),
        "sell" => Ok(Side::Sell),
        _ => Err(ApiError::validation("side_invalid", json!({}))),
    }
}

fn positive_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let value = Decimal::from_str(value)
        .map_err(|_| ApiError::validation("invalid_decimal", json!({ "field": field })))?;
    if value <= Decimal::ZERO {
        return Err(ApiError::validation(
            "decimal_must_be_positive",
            json!({ "field": field }),
        ));
    }
    Ok(value)
}

fn non_negative_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let value = Decimal::from_str(value)
        .map_err(|_| ApiError::validation("invalid_decimal", json!({ "field": field })))?;
    if value < Decimal::ZERO {
        return Err(ApiError::validation(
            "decimal_must_be_non_negative",
            json!({ "field": field }),
        ));
    }
    Ok(value)
}

fn validate_quantity_unit(
    instrument: &ballast_core::Instrument,
    quantity_unit: QuantityUnit,
) -> ApiResult<()> {
    if quantity_unit == QuantityUnit::Contracts
        && instrument.id.market_kind == ballast_core::MarketKind::Spot
    {
        return Err(ApiError::validation(
            "quantity_unit_not_supported",
            json!({}),
        ));
    }
    Ok(())
}

impl From<StoredReplayRun> for ReplayRunView {
    fn from(value: StoredReplayRun) -> Self {
        Self {
            id: value.id,
            idempotency_key: value.idempotency_key,
            instrument_id: value.instrument_id,
            template_version_id: value.template_version_id,
            strategy_kind: value.strategy_kind,
            side: value.side,
            requested_amount: value.requested_amount.to_string(),
            quantity_unit: value.quantity_unit,
            start_at: value.start_at,
            end_at: value.end_at,
            execution_model: value.execution_model,
            model_version: value.model_version,
            fee_rate: value.fee_rate.to_string(),
            extra_slippage_bps: value.extra_slippage_bps.to_string(),
            gap_threshold_seconds: value.gap_threshold_seconds,
            strategy_snapshot: value.strategy_snapshot,
            status: value.status,
            failure_code: value.failure_code,
            data_first_at: value.data_first_at,
            data_last_at: value.data_last_at,
            trade_count: value.trade_count,
            gap_count: value.gap_count,
            data_gaps: value.data_gaps,
            confidence: value.confidence,
            limitations: value.limitations,
            created_at: value.created_at,
        }
    }
}

impl From<StoredReplaySlice> for ReplaySliceView {
    fn from(value: StoredReplaySlice) -> Self {
        Self {
            sequence: value.sequence,
            window_start: value.window_start,
            window_end: value.window_end,
            status: value.status,
            trade_count: value.trade_count,
            market_volume: value.market_volume.to_string(),
            requested_amount: value.requested_amount.to_string(),
            filled_amount: value.filled_amount.to_string(),
            filled_native_quantity: value.filled_native_quantity.to_string(),
            market_vwap: value.market_vwap.map(|value| value.to_string()),
            simulated_price: value.simulated_price.map(|value| value.to_string()),
            fee_amount: value.fee_amount.to_string(),
            decision_input: value.decision_input,
        }
    }
}

impl From<StoredReplayMetrics> for ReplayMetricsView {
    fn from(value: StoredReplayMetrics) -> Self {
        Self {
            arrival_price: value.arrival_price.map(|value| value.to_string()),
            market_vwap: value.market_vwap.map(|value| value.to_string()),
            simulated_execution_vwap: value
                .simulated_execution_vwap
                .map(|value| value.to_string()),
            implementation_shortfall_bps: value
                .implementation_shortfall_bps
                .map(|value| value.to_string()),
            implementation_shortfall_amount: value
                .implementation_shortfall_amount
                .map(|value| value.to_string()),
            requested_amount: value.requested_amount.to_string(),
            filled_amount: value.filled_amount.to_string(),
            fill_rate: value.fill_rate.to_string(),
            residual_amount: value.residual_amount.to_string(),
            actual_participation_rate: value
                .actual_participation_rate
                .map(|value| value.to_string()),
            target_participation_rate: value
                .target_participation_rate
                .map(|value| value.to_string()),
            participation_rate_deviation: value
                .participation_rate_deviation
                .map(|value| value.to_string()),
            slice_count: value.slice_count,
            empty_window_count: value.empty_window_count,
            fee_amount: value.fee_amount.to_string(),
            explicit_slippage_amount: value.explicit_slippage_amount.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use super::ensure_idempotency_match;

    #[test]
    fn idempotency_key_rejects_a_different_replay_request() {
        let error = ensure_idempotency_match("first", "second").unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.code, "replay_idempotency_key_conflict");
        assert!(ensure_idempotency_match("same", "same").is_ok());
    }
}
