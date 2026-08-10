use std::str::FromStr;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use ballast_core::MarketKind;
use ballast_gateway_client::proto::HistoricalDataType;
use ballast_storage::{
    HistoricalCandle, HistoricalTrade, NewHistoricalBackfill, StoredHistoricalBackfill,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{ApiError, ApiResult, parse_exchange};
use crate::AppState;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/history/backfills", post(run_backfill))
        .route("/api/v1/history/backfills/{job_id}", get(get_backfill))
        .route("/api/v1/history/ohlcv", get(list_candles))
        .route("/api/v1/history/trades", get(list_trades))
}

#[derive(Debug, Deserialize)]
struct BackfillRequest {
    exchange: String,
    market_kind: String,
    symbol: String,
    data_type: String,
    timeframe: Option<String>,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    idempotency_key: String,
    #[serde(default = "default_page_limit")]
    page_limit: u32,
    #[serde(default = "default_max_pages")]
    max_pages: u32,
}

const fn default_page_limit() -> u32 {
    500
}
const fn default_max_pages() -> u32 {
    25
}

#[derive(Debug, Serialize)]
struct BackfillView {
    id: Uuid,
    idempotency_key: String,
    instrument_id: Uuid,
    data_type: String,
    timeframe: Option<String>,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    cursor_at: DateTime<Utc>,
    covered_from: Option<DateTime<Utc>>,
    covered_to: Option<DateTime<Utc>>,
    status: String,
    rows_written: i64,
    last_error_code: Option<String>,
}

async fn run_backfill(
    State(state): State<AppState>,
    Json(request): Json<BackfillRequest>,
) -> ApiResult<Json<BackfillView>> {
    validate_backfill(&request)?;
    let exchange = parse_exchange(&request.exchange)?;
    let market_kind = parse_market_kind(&request.market_kind)?;
    let instrument = ballast_storage::get_instrument_by_key(
        &state.database,
        exchange,
        market_kind,
        request.symbol.trim(),
    )
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("instrument_not_found"))?;
    let timeframe = if request.data_type == "ohlcv" {
        Some(request.timeframe.clone().expect("validated timeframe"))
    } else {
        None
    };
    let created = ballast_storage::create_or_get_historical_backfill(
        &state.database,
        &NewHistoricalBackfill {
            idempotency_key: request.idempotency_key.trim().to_owned(),
            instrument_id: instrument.id,
            data_type: request.data_type.clone(),
            timeframe: timeframe.clone(),
            start_at: request.start_at,
            end_at: request.end_at,
        },
    )
    .await
    .map_err(ApiError::database)?;
    if created.instrument_id != instrument.id
        || created.data_type != request.data_type
        || created.timeframe != timeframe
        || created.start_at != request.start_at
        || created.end_at != request.end_at
    {
        return Err(ApiError::conflict("idempotency_key_conflict"));
    }
    if created.status == "completed" {
        return Ok(Json(created.into()));
    }
    ballast_storage::mark_historical_backfill_running(&state.database, created.id)
        .await
        .map_err(ApiError::database)?;

    let data_type = if request.data_type == "ohlcv" {
        HistoricalDataType::Ohlcv
    } else {
        HistoricalDataType::Trades
    };
    let mut cursor = created.cursor_at;
    for _ in 0..request.max_pages {
        let response = match state
            .gateway
            .fetch_historical_batch(
                &instrument.instrument.id,
                data_type,
                timeframe.clone(),
                cursor.timestamp_millis(),
                request.end_at.timestamp_millis(),
                request.page_limit,
            )
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let code = gateway_error_code(&error);
                ballast_storage::mark_historical_backfill_failed(
                    &state.database,
                    created.id,
                    &code,
                )
                .await
                .map_err(ApiError::database)?;
                return Err(ApiError::gateway(error));
            }
        };
        let next_cursor = millis(response.next_cursor_ms, "next_cursor_ms")?;
        if let Err(error) = validate_historical_cursor(cursor, next_cursor, response.exhausted) {
            ballast_storage::mark_historical_backfill_failed(
                &state.database,
                created.id,
                "historical_cursor_not_advanced",
            )
            .await
            .map_err(ApiError::database)?;
            return Err(error);
        }
        let observed_at = millis(response.observed_at_ms, "observed_at_ms")?;
        if data_type == HistoricalDataType::Ohlcv {
            let candles = response
                .candles
                .into_iter()
                .map(|row| {
                    Ok(HistoricalCandle {
                        open_time: millis(row.open_time_ms, "open_time_ms")?,
                        open: decimal(&row.open, "open")?,
                        high: decimal(&row.high, "high")?,
                        low: decimal(&row.low, "low")?,
                        close: decimal(&row.close, "close")?,
                        volume: non_negative_decimal(&row.volume, "volume")?,
                        source: request.exchange.clone(),
                        observed_at,
                    })
                })
                .collect::<ApiResult<Vec<_>>>()?;
            ballast_storage::persist_candle_batch(
                &state.database,
                created.id,
                instrument.id,
                timeframe.as_deref().expect("timeframe"),
                &candles,
                next_cursor,
                response.exhausted,
            )
            .await
            .map_err(ApiError::database)?;
        } else {
            let trades = response
                .trades
                .into_iter()
                .map(|row| {
                    Ok(HistoricalTrade {
                        exchange_trade_id: row.exchange_trade_id,
                        trade_time: millis(row.trade_time_ms, "trade_time_ms")?,
                        price: decimal(&row.price, "price")?,
                        quantity: decimal(&row.quantity, "quantity")?,
                        taker_side: normalize_side(&row.taker_side)?,
                        source: request.exchange.clone(),
                        observed_at,
                    })
                })
                .collect::<ApiResult<Vec<_>>>()?;
            ballast_storage::persist_trade_batch(
                &state.database,
                created.id,
                instrument.id,
                &trades,
                next_cursor,
                response.exhausted,
            )
            .await
            .map_err(ApiError::database)?;
        }
        cursor = next_cursor;
        if response.exhausted {
            break;
        }
    }
    let job = ballast_storage::get_historical_backfill(&state.database, created.id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::internal("historical_backfill_missing"))?;
    Ok(Json(job.into()))
}

async fn get_backfill(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> ApiResult<Json<BackfillView>> {
    let job = ballast_storage::get_historical_backfill(&state.database, job_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("historical_backfill_not_found"))?;
    Ok(Json(job.into()))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    exchange: String,
    market_kind: String,
    symbol: String,
    timeframe: Option<String>,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct Page<T> {
    items: Vec<T>,
    total: i64,
    limit: i64,
    offset: i64,
}

#[derive(Debug, Serialize)]
struct CandlePage {
    items: Vec<CandleView>,
    total: i64,
    limit: i64,
    offset: i64,
    coverage: CoverageView,
    gaps: Vec<GapView>,
    gap_scope: &'static str,
}

#[derive(Debug, Serialize)]
struct CoverageView {
    first_at: Option<DateTime<Utc>>,
    last_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct GapView {
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct CandleView {
    open_time: DateTime<Utc>,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    source: String,
}

#[derive(Debug, Serialize)]
struct TradeView {
    exchange_trade_id: String,
    trade_time: DateTime<Utc>,
    price: String,
    quantity: String,
    taker_side: String,
    source: String,
}

async fn list_candles(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<CandlePage>> {
    let (instrument_id, limit, offset) = resolve_query(&state, &query).await?;
    let timeframe = query
        .timeframe
        .as_deref()
        .ok_or_else(|| ApiError::validation("timeframe_required", json!({})))?;
    let step = timeframe_duration(timeframe)?;
    let (items, total) = ballast_storage::list_historical_candles(
        &state.database,
        instrument_id,
        timeframe,
        query.start_at,
        query.end_at,
        limit,
        offset,
    )
    .await
    .map_err(ApiError::database)?;
    let coverage = CoverageView {
        first_at: items.first().map(|v| v.open_time),
        last_at: items.last().map(|v| v.open_time),
    };
    let gaps = candle_gaps(&items, query.start_at, query.end_at, step, offset, total);
    Ok(Json(CandlePage {
        items: items
            .into_iter()
            .map(|v| CandleView {
                open_time: v.open_time,
                open: v.open.to_string(),
                high: v.high.to_string(),
                low: v.low.to_string(),
                close: v.close.to_string(),
                volume: v.volume.to_string(),
                source: v.source,
            })
            .collect(),
        total,
        limit,
        offset,
        coverage,
        gaps,
        gap_scope: "returned_page_with_range_boundaries",
    }))
}

async fn list_trades(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<Page<TradeView>>> {
    let (instrument_id, limit, offset) = resolve_query(&state, &query).await?;
    let (items, total) = ballast_storage::list_historical_trades(
        &state.database,
        instrument_id,
        query.start_at,
        query.end_at,
        limit,
        offset,
    )
    .await
    .map_err(ApiError::database)?;
    Ok(Json(Page {
        items: items
            .into_iter()
            .map(|v| TradeView {
                exchange_trade_id: v.exchange_trade_id,
                trade_time: v.trade_time,
                price: v.price.to_string(),
                quantity: v.quantity.to_string(),
                taker_side: v.taker_side,
                source: v.source,
            })
            .collect(),
        total,
        limit,
        offset,
    }))
}

async fn resolve_query(state: &AppState, query: &HistoryQuery) -> ApiResult<(Uuid, i64, i64)> {
    if query.start_at >= query.end_at {
        return Err(ApiError::validation("invalid_time_range", json!({})));
    }
    let limit = query.limit.unwrap_or(500);
    let offset = query.offset.unwrap_or(0);
    if !(1..=1_000).contains(&limit) || offset < 0 {
        return Err(ApiError::validation("invalid_pagination", json!({})));
    }
    let instrument = ballast_storage::get_instrument_by_key(
        &state.database,
        parse_exchange(&query.exchange)?,
        parse_market_kind(&query.market_kind)?,
        query.symbol.trim(),
    )
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("instrument_not_found"))?;
    Ok((instrument.id, limit, offset))
}

fn validate_backfill(request: &BackfillRequest) -> ApiResult<()> {
    if request.start_at >= request.end_at {
        return Err(ApiError::validation("invalid_time_range", json!({})));
    }
    if request.idempotency_key.trim().is_empty() {
        return Err(ApiError::validation("idempotency_key_required", json!({})));
    }
    if !(1..=1_000).contains(&request.page_limit) || !(1..=100).contains(&request.max_pages) {
        return Err(ApiError::validation(
            "invalid_backfill_pagination",
            json!({}),
        ));
    }
    match request.data_type.as_str() {
        "ohlcv" => {
            timeframe_duration(
                request
                    .timeframe
                    .as_deref()
                    .ok_or_else(|| ApiError::validation("timeframe_required", json!({})))?,
            )?;
        }
        "trades" if request.timeframe.is_none() => {}
        "trades" => return Err(ApiError::validation("timeframe_not_allowed", json!({}))),
        _ => {
            return Err(ApiError::validation(
                "historical_data_type_invalid",
                json!({}),
            ));
        }
    }
    Ok(())
}

fn candle_gaps(
    items: &[HistoricalCandle],
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    step: Duration,
    offset: i64,
    total: i64,
) -> Vec<GapView> {
    if total == 0 {
        return vec![GapView { start_at, end_at }];
    }
    let mut gaps = Vec::new();
    if offset == 0 {
        if let Some(first) = items.first() {
            if first.open_time > start_at {
                gaps.push(GapView {
                    start_at,
                    end_at: first.open_time,
                });
            }
        }
    }
    for pair in items.windows(2) {
        let expected = pair[0].open_time + step;
        if pair[1].open_time > expected {
            gaps.push(GapView {
                start_at: expected,
                end_at: pair[1].open_time,
            });
        }
    }
    if offset + items.len() as i64 >= total {
        if let Some(last) = items.last() {
            let expected = last.open_time + step;
            if expected < end_at {
                gaps.push(GapView {
                    start_at: expected,
                    end_at,
                });
            }
        }
    }
    gaps
}

fn timeframe_duration(value: &str) -> ApiResult<Duration> {
    let (number, unit) = value.split_at(value.len().saturating_sub(1));
    let count = number
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::validation("timeframe_invalid", json!({ "timeframe": value })))?;
    match unit {
        "m" => Ok(Duration::minutes(count)),
        "h" => Ok(Duration::hours(count)),
        "d" => Ok(Duration::days(count)),
        "w" => Ok(Duration::weeks(count)),
        _ => Err(ApiError::validation(
            "timeframe_invalid",
            json!({ "timeframe": value }),
        )),
    }
}

fn validate_historical_cursor(
    cursor: DateTime<Utc>,
    next_cursor: DateTime<Utc>,
    exhausted: bool,
) -> ApiResult<()> {
    if !exhausted && next_cursor <= cursor {
        return Err(ApiError::internal("historical_cursor_not_advanced"));
    }
    Ok(())
}

pub(super) fn parse_market_kind(value: &str) -> ApiResult<MarketKind> {
    match value {
        "spot" => Ok(MarketKind::Spot),
        "perpetual" => Ok(MarketKind::Perpetual),
        _ => Err(ApiError::validation("market_kind_invalid", json!({}))),
    }
}

fn millis(value: i64, field: &'static str) -> ApiResult<DateTime<Utc>> {
    DateTime::from_timestamp_millis(value).ok_or_else(|| {
        ApiError::validation("historical_timestamp_invalid", json!({ "field": field }))
    })
}

fn decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let value = Decimal::from_str(value).map_err(|_| {
        ApiError::validation("historical_decimal_invalid", json!({ "field": field }))
    })?;
    if value <= Decimal::ZERO {
        return Err(ApiError::validation(
            "historical_decimal_invalid",
            json!({ "field": field }),
        ));
    }
    Ok(value)
}

fn non_negative_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let value = Decimal::from_str(value).map_err(|_| {
        ApiError::validation("historical_decimal_invalid", json!({ "field": field }))
    })?;
    if value < Decimal::ZERO {
        return Err(ApiError::validation(
            "historical_decimal_invalid",
            json!({ "field": field }),
        ));
    }
    Ok(value)
}

fn normalize_side(value: &str) -> ApiResult<String> {
    match value {
        "buy" | "sell" | "unknown" => Ok(value.to_owned()),
        _ => Err(ApiError::validation(
            "historical_trade_side_invalid",
            json!({}),
        )),
    }
}

fn gateway_error_code(error: &ballast_gateway_client::GatewayClientError) -> String {
    match error {
        ballast_gateway_client::GatewayClientError::Rpc(status) => {
            format!("gateway_{:?}", status.code()).to_lowercase()
        }
        _ => "gateway_unavailable".to_owned(),
    }
}

impl From<StoredHistoricalBackfill> for BackfillView {
    fn from(value: StoredHistoricalBackfill) -> Self {
        Self {
            id: value.id,
            idempotency_key: value.idempotency_key,
            instrument_id: value.instrument_id,
            data_type: value.data_type,
            timeframe: value.timeframe,
            start_at: value.start_at,
            end_at: value.end_at,
            cursor_at: value.cursor_at,
            covered_from: value.covered_from,
            covered_to: value.covered_to,
            status: value.status,
            rows_written: value.rows_written,
            last_error_code: value.last_error_code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_timeframes_and_boundaries() {
        assert_eq!(timeframe_duration("1m").unwrap(), Duration::minutes(1));
        assert_eq!(timeframe_duration("4h").unwrap(), Duration::hours(4));
        assert!(timeframe_duration("0m").is_err());
        assert!(timeframe_duration("month").is_err());
    }

    #[test]
    fn reports_internal_and_range_boundary_gaps() {
        let start = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let candle = |minutes| HistoricalCandle {
            open_time: start + Duration::minutes(minutes),
            open: Decimal::ONE,
            high: Decimal::ONE,
            low: Decimal::ONE,
            close: Decimal::ONE,
            volume: Decimal::ZERO,
            source: "okx".to_owned(),
            observed_at: start,
        };
        let gaps = candle_gaps(
            &[candle(1), candle(3)],
            start,
            start + Duration::minutes(5),
            Duration::minutes(1),
            0,
            2,
        );
        assert_eq!(gaps.len(), 3);
        assert_eq!(gaps[0].start_at, start);
        assert_eq!(gaps[1].start_at, start + Duration::minutes(2));
        assert_eq!(gaps[2].end_at, start + Duration::minutes(5));
    }

    #[test]
    fn rejects_saturated_trade_page_without_advancing_past_its_millisecond() {
        let cursor = DateTime::from_timestamp_millis(1_001).unwrap();
        assert!(validate_historical_cursor(cursor, cursor, false).is_err());
        assert!(validate_historical_cursor(cursor, cursor, true).is_ok());
    }
}
