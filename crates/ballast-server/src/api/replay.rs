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

#[derive(Debug, Clone, Deserialize)]
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
    data_snapshot: Value,
    coverage_snapshot: Value,
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

    let coverage_snapshot =
        require_trade_coverage(&state, instrument.id, request.start_at, request.end_at).await?;
    let loaded = load_trades(&state, instrument.id, request.start_at, request.end_at).await?;
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
        &loaded.trades,
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
            data_snapshot: loaded.snapshot,
            coverage_snapshot,
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
    .map_err(map_create_replay_error)?;
    Ok(Json(stored.into()))
}

fn map_create_replay_error(error: ballast_storage::CreateReplayError) -> ApiError {
    match error {
        ballast_storage::CreateReplayError::IdempotencyConflict => {
            ApiError::conflict("replay_idempotency_key_conflict")
        }
        ballast_storage::CreateReplayError::Database(error) => ApiError::database(error),
    }
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
) -> ApiResult<LoadedTrades> {
    let mut transaction = state.database.begin().await.map_err(ApiError::database)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::database)?;
    let postgres_snapshot: String = sqlx::query_scalar("SELECT pg_current_snapshot()::text")
        .fetch_one(&mut *transaction)
        .await
        .map_err(ApiError::database)?;
    let snapshot_max_ingestion_id = ballast_storage::historical_trade_snapshot_high_watermark(
        &mut *transaction,
        instrument_id,
        start_at,
        end_at,
    )
    .await
    .map_err(ApiError::database)?;
    let mut result = Vec::new();
    let mut cursor = None;
    loop {
        let page = ballast_storage::list_historical_trade_page(
            &mut *transaction,
            instrument_id,
            start_at,
            end_at,
            snapshot_max_ingestion_id,
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
    transaction.commit().await.map_err(ApiError::database)?;
    let snapshot = json!({
        "version": 1,
        "postgres_snapshot": postgres_snapshot,
        "isolation": "repeatable_read",
        "instrument_id": instrument_id,
        "start_at": start_at,
        "end_at": end_at,
        "maximum_ingestion_id": snapshot_max_ingestion_id,
        "ordering": ["trade_time", "exchange_trade_id"],
        "trade_count": result.len(),
        "first_trade": result.first().map(|trade| json!({
            "trade_time": trade.trade_time,
            "exchange_trade_id": trade.exchange_trade_id,
        })),
        "last_trade": result.last().map(|trade| json!({
            "trade_time": trade.trade_time,
            "exchange_trade_id": trade.exchange_trade_id,
        })),
    });
    Ok(LoadedTrades {
        trades: result,
        snapshot,
    })
}

struct LoadedTrades {
    trades: Vec<ReplayTrade>,
    snapshot: Value,
}

async fn require_trade_coverage(
    state: &AppState,
    instrument_id: Uuid,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> ApiResult<Value> {
    let jobs = ballast_storage::list_historical_trade_backfills(
        &state.database,
        instrument_id,
        start_at,
        end_at,
    )
    .await
    .map_err(ApiError::database)?;
    if jobs.is_empty() {
        return Err(ApiError::conflict("replay_history_backfill_required"));
    }

    let coverage_jobs =
        select_trade_coverage(&jobs, start_at, end_at).map_err(ApiError::conflict)?;

    Ok(json!({
        "version": 1,
        "required_start_at": start_at,
        "required_end_at": end_at,
        "jobs": coverage_jobs.into_iter().map(|job| json!({
            "id": job.id,
            "idempotency_key": job.idempotency_key,
            "start_at": job.start_at,
            "end_at": job.end_at,
            "cursor_at": job.cursor_at,
            "covered_from": job.covered_from,
            "covered_to": job.covered_to,
            "rows_written": job.rows_written,
            "status": job.status,
        })).collect::<Vec<_>>(),
    }))
}

fn select_trade_coverage(
    jobs: &[ballast_storage::StoredHistoricalBackfill],
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
) -> Result<Vec<&ballast_storage::StoredHistoricalBackfill>, &'static str> {
    let mut covered_until = start_at;
    let mut coverage_jobs = Vec::new();
    for job in jobs.iter().filter(|job| job.status == "completed") {
        if job.start_at > covered_until {
            continue;
        }
        let job_covered_until = job.cursor_at.min(job.end_at);
        if job_covered_until <= covered_until {
            continue;
        }
        coverage_jobs.push(job);
        covered_until = job_covered_until;
        if covered_until >= end_at {
            return Ok(coverage_jobs);
        }
    }
    if jobs.iter().any(|job| job.status == "failed") {
        return Err("replay_history_backfill_failed");
    }
    if jobs
        .iter()
        .any(|job| matches!(job.status.as_str(), "pending" | "running"))
    {
        return Err("replay_history_backfill_incomplete");
    }
    Err("replay_history_coverage_insufficient")
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
    if request.start_at.timestamp_subsec_nanos() % 1_000 != 0
        || request.end_at.timestamp_subsec_nanos() % 1_000 != 0
    {
        return Err(ApiError::validation(
            "timestamp_precision_invalid",
            json!({ "maximum_precision": "microseconds" }),
        ));
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
            data_snapshot: value.data_snapshot,
            coverage_snapshot: value.coverage_snapshot,
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
    use ballast_core::{Exchange, Instrument, InstrumentId, MarketKind};
    use ballast_storage::{HistoricalTrade, NewHistoricalBackfill, NewStrategyTemplate};
    use chrono::{Duration, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use axum::http::StatusCode;
    use axum::{Json, extract::State};

    use super::{
        CreateReplayRequest, create_replay, ensure_idempotency_match, map_create_replay_error,
        select_trade_coverage,
    };

    fn backfill(
        status: &str,
        start_at: chrono::DateTime<Utc>,
        end_at: chrono::DateTime<Utc>,
    ) -> ballast_storage::StoredHistoricalBackfill {
        ballast_storage::StoredHistoricalBackfill {
            id: Uuid::now_v7(),
            idempotency_key: Uuid::now_v7().to_string(),
            instrument_id: Uuid::now_v7(),
            data_type: "trades".to_owned(),
            timeframe: None,
            start_at,
            end_at,
            cursor_at: if status == "completed" {
                end_at
            } else {
                start_at
            },
            covered_from: None,
            covered_to: None,
            status: status.to_owned(),
            rows_written: 0,
            last_error_code: (status == "failed").then(|| "gateway_unavailable".to_owned()),
        }
    }

    #[test]
    fn idempotency_key_rejects_a_different_replay_request() {
        let error = ensure_idempotency_match("first", "second").unwrap_err();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.code, "replay_idempotency_key_conflict");
        assert!(ensure_idempotency_match("same", "same").is_ok());
        let storage_conflict =
            map_create_replay_error(ballast_storage::CreateReplayError::IdempotencyConflict);
        assert_eq!(storage_conflict.status, StatusCode::CONFLICT);
        assert_eq!(storage_conflict.code, "replay_idempotency_key_conflict");
    }

    #[test]
    fn coverage_requires_completed_jobs_across_the_whole_range() {
        let start = Utc::now();
        let middle = start + Duration::minutes(1);
        let end = middle + Duration::minutes(1);
        let jobs = [
            backfill("completed", start, middle),
            backfill("completed", middle, end),
        ];
        assert_eq!(select_trade_coverage(&jobs, start, end).unwrap().len(), 2);

        assert_eq!(
            select_trade_coverage(&[backfill("pending", start, end)], start, end).unwrap_err(),
            "replay_history_backfill_incomplete"
        );
        assert_eq!(
            select_trade_coverage(&[backfill("failed", start, end)], start, end).unwrap_err(),
            "replay_history_backfill_failed"
        );
        assert_eq!(
            select_trade_coverage(&[backfill("completed", start, middle)], start, end,)
                .unwrap_err(),
            "replay_history_coverage_insufficient"
        );
    }

    #[tokio::test]
    async fn api_returns_conflict_for_a_concurrent_key_reused_with_different_input() {
        let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
            eprintln!("TEST_DATABASE_URL is not set; skipping replay API integration test");
            return;
        };
        let pool = ballast_storage::connect(&database_url).await.unwrap();
        ballast_storage::migrate(&pool).await.unwrap();
        let suffix = Utc::now().timestamp_nanos_opt().unwrap();
        let symbol = format!("API-REPLAY/USDT-{suffix}");
        let instrument = Instrument {
            id: InstrumentId::new(Exchange::Okx, MarketKind::Spot, &symbol).unwrap(),
            exchange_symbol: format!("APIREPLAYUSDT{suffix}"),
            base_asset: "API-REPLAY".to_owned(),
            quote_asset: "USDT".to_owned(),
            settle_asset: None,
            contract_kind: None,
            contract_size: None,
            price_tick: Decimal::new(1, 2),
            quantity_step: Decimal::new(1, 3),
            minimum_quantity: None,
            minimum_notional: None,
            maker_fee_rate: None,
            taker_fee_rate: Some(Decimal::new(1, 3)),
            active: true,
        };
        let instrument = ballast_storage::upsert_instruments(&pool, &[instrument])
            .await
            .unwrap()
            .remove(0);
        let (_, template) = ballast_storage::create_strategy_template(
            &pool,
            NewStrategyTemplate {
                name: format!("API replay {suffix}"),
                description: "API replay integration".to_owned(),
                strategy_kind: "twap".to_owned(),
                quantity_unit: "base_quantity".to_owned(),
                duration_seconds: 20,
                slice_interval_ms: 10_000,
                max_slippage_bps: 20,
                participation_rate: None,
                max_slice_amount: None,
                change_note: "initial".to_owned(),
                execution_backend: "managed_ioc".to_owned(),
                venue_exchange: None,
                venue_market_kind: None,
                native_algorithm: None,
                native_params: None,
            },
        )
        .await
        .unwrap();
        let start = chrono::DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap()
            - Duration::minutes(5);
        let end = start + Duration::seconds(20);
        let backfill = ballast_storage::create_or_get_historical_backfill(
            &pool,
            &NewHistoricalBackfill {
                idempotency_key: format!("api-replay-history-{suffix}"),
                instrument_id: instrument.id,
                data_type: "trades".to_owned(),
                timeframe: None,
                start_at: start,
                end_at: end,
            },
        )
        .await
        .unwrap();
        ballast_storage::persist_trade_batch(
            &pool,
            backfill.id,
            instrument.id,
            &[
                HistoricalTrade {
                    exchange_trade_id: "first".to_owned(),
                    trade_time: start + Duration::seconds(1),
                    price: Decimal::new(100, 0),
                    quantity: Decimal::new(10, 0),
                    taker_side: "buy".to_owned(),
                    source: "okx".to_owned(),
                    observed_at: Utc::now(),
                },
                HistoricalTrade {
                    exchange_trade_id: "second".to_owned(),
                    trade_time: start + Duration::seconds(11),
                    price: Decimal::new(101, 0),
                    quantity: Decimal::new(10, 0),
                    taker_side: "buy".to_owned(),
                    source: "okx".to_owned(),
                    observed_at: Utc::now(),
                },
            ],
            end,
            true,
        )
        .await
        .unwrap();
        let state = crate::AppState {
            database: pool,
            gateway: ballast_gateway_client::GatewayClient::connect_lazy("http://127.0.0.1:1"),
            metrics: crate::metrics::AppMetrics::new().unwrap(),
        };
        let request = CreateReplayRequest {
            exchange: "okx".to_owned(),
            market_kind: "spot".to_owned(),
            symbol,
            template_version_id: template.id,
            side: "buy".to_owned(),
            requested_amount: "2".to_owned(),
            quantity_unit: "base_quantity".to_owned(),
            start_at: start,
            end_at: end,
            idempotency_key: format!("api-replay-{suffix}"),
            execution_model: "trade_vwap_proxy".to_owned(),
            fee_rate: Some("0.001".to_owned()),
            extra_slippage_bps: Some("0".to_owned()),
            gap_threshold_seconds: Some(20),
        };
        let mut conflicting = request.clone();
        conflicting.fee_rate = Some("0.002".to_owned());
        let (first, second) = tokio::join!(
            create_replay(State(state.clone()), Json(request)),
            create_replay(State(state), Json(conflicting)),
        );
        let outcomes = [first, second];
        let errors = outcomes
            .iter()
            .filter_map(|result| {
                result
                    .as_ref()
                    .err()
                    .map(|error| (error.status, error.code))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes.iter().filter(|result| result.is_ok()).count(),
            1,
            "unexpected API errors: {errors:?}"
        );
        let conflict = outcomes
            .into_iter()
            .find_map(Result::err)
            .expect("one request must lose the idempotency race");
        assert_eq!(conflict.status, StatusCode::CONFLICT);
        assert_eq!(conflict.code, "replay_idempotency_key_conflict");
    }
}
