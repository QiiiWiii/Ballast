use std::{collections::BTreeMap, time::Instant};

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, ApiResult, util};

#[derive(Debug, Clone, Serialize)]
pub(super) struct ExchangeView {
    exchange: &'static str,
    status: String,
    last_error_code: Option<String>,
    last_success_at_ms: Option<i64>,
    health_query_latency_ms: Option<i64>,
    instrument_count: i64,
    active_subscriptions: i64,
    stale_subscriptions: i64,
    capabilities: Option<CapabilitiesView>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct CapabilitiesView {
    spot: bool,
    perpetual_linear: bool,
    perpetual_inverse: bool,
    fetch_order_book: bool,
    watch_order_book: bool,
    watch_trades: bool,
}

pub(super) async fn list_exchanges(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ExchangeView>>> {
    let started = Instant::now();
    let capability_results = join_all(
        util::exchanges()
            .iter()
            .copied()
            .map(|exchange| state.gateway.capabilities(exchange)),
    )
    .await;
    let health = state.gateway.health().await.map_err(ApiError::gateway)?;
    let health_query_latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let instrument_counts = exchange_instrument_counts(&state).await?;
    let subscription_counts = exchange_subscription_counts(&state).await?;
    let mut health_by_exchange = BTreeMap::new();
    for adapter in health.adapters {
        health_by_exchange.insert(adapter.exchange, adapter);
    }
    let mut result = Vec::with_capacity(5);
    for (exchange, capability_result) in util::exchanges().iter().copied().zip(capability_results) {
        let capabilities = capability_result.ok().map(|value| CapabilitiesView {
            spot: value.spot,
            perpetual_linear: value.perpetual_linear,
            perpetual_inverse: value.perpetual_inverse,
            fetch_order_book: value.fetch_order_book,
            watch_order_book: value.watch_order_book,
            watch_trades: value.watch_trades,
        });
        let key = util::exchange_proto_number(exchange);
        let adapter = health_by_exchange.remove(&key);
        let exchange_name = util::exchange_text(exchange);
        let instrument_count = instrument_counts.get(exchange_name).copied().unwrap_or(0);
        let subscriptions = subscription_counts
            .get(exchange_name)
            .copied()
            .unwrap_or((0, 0));
        let status = adapter
            .as_ref()
            .map_or_else(|| "unknown".to_owned(), |value| value.status.clone());
        let error_code = adapter
            .as_ref()
            .and_then(|value| value.last_error_code.clone());
        let last_success_at_ms = adapter.as_ref().and_then(|value| value.last_success_at_ms);
        upsert_exchange_health(
            &state,
            exchange_name,
            &status,
            error_code.as_deref(),
            last_success_at_ms,
            health_query_latency_ms,
        )
        .await?;
        record_exchange_health_event(
            &state,
            exchange_name,
            &status,
            error_code.as_deref(),
            health_query_latency_ms,
        )
        .await?;
        result.push(ExchangeView {
            exchange: exchange_name,
            status,
            last_error_code: error_code,
            last_success_at_ms,
            health_query_latency_ms: Some(health_query_latency_ms),
            instrument_count,
            active_subscriptions: subscriptions.0,
            stale_subscriptions: subscriptions.1,
            capabilities,
        });
    }
    Ok(Json(result))
}

pub(super) async fn exchange_snapshots(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ExchangeView>>> {
    Ok(Json(list_exchange_snapshots(&state).await?))
}

async fn exchange_instrument_counts(state: &AppState) -> ApiResult<BTreeMap<String, i64>> {
    sqlx::query(
        "SELECT exchange, COUNT(*)::bigint AS instrument_count FROM instruments GROUP BY exchange",
    )
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| {
        Ok((
            row.try_get::<String, _>("exchange")?,
            row.try_get::<i64, _>("instrument_count")?,
        ))
    })
    .collect::<Result<_, sqlx::Error>>()
    .map_err(ApiError::database)
}

async fn exchange_subscription_counts(state: &AppState) -> ApiResult<BTreeMap<String, (i64, i64)>> {
    sqlx::query(
        r#"
        SELECT exchange,
               COUNT(*) FILTER (WHERE status IN ('connecting', 'connected', 'reconnecting'))::bigint AS active,
               COUNT(*) FILTER (WHERE status = 'stale')::bigint AS stale
        FROM market_subscription_health
        GROUP BY exchange
        "#,
    )
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| {
        Ok((
            row.try_get::<String, _>("exchange")?,
            (
                row.try_get::<i64, _>("active")?,
                row.try_get::<i64, _>("stale")?,
            ),
        ))
    })
    .collect::<Result<_, sqlx::Error>>()
    .map_err(ApiError::database)
}

pub(super) async fn list_exchange_snapshots(state: &AppState) -> ApiResult<Vec<ExchangeView>> {
    let health_rows = sqlx::query(
        r#"
        SELECT health.exchange,
               health.status,
               health.last_error_code,
               health.last_success_at,
               health.request_latency_ms
        FROM exchange_health health
        "#,
    )
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?;
    let instrument_rows = sqlx::query(
        "SELECT exchange, COUNT(*)::bigint AS instrument_count FROM instruments GROUP BY exchange",
    )
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?;
    let subscription_rows = sqlx::query(
        r#"
        SELECT exchange,
               COUNT(*) FILTER (WHERE status IN ('connecting', 'connected', 'reconnecting'))::bigint AS active,
               COUNT(*) FILTER (WHERE status = 'stale')::bigint AS stale
        FROM market_subscription_health
        GROUP BY exchange
        "#,
    )
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?;

    let health = health_rows
        .into_iter()
        .map(|row| {
            let exchange = row.try_get::<String, _>("exchange")?;
            let snapshot = (
                row.try_get::<String, _>("status")?,
                row.try_get::<Option<String>, _>("last_error_code")?,
                row.try_get::<Option<DateTime<Utc>>, _>("last_success_at")?,
                row.try_get::<Option<i64>, _>("request_latency_ms")?,
            );
            Ok((exchange, snapshot))
        })
        .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()
        .map_err(ApiError::database)?;
    let instrument_counts = instrument_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("exchange")?,
                row.try_get::<i64, _>("instrument_count")?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()
        .map_err(ApiError::database)?;
    let subscription_counts = subscription_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("exchange")?,
                (
                    row.try_get::<i64, _>("active")?,
                    row.try_get::<i64, _>("stale")?,
                ),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()
        .map_err(ApiError::database)?;

    Ok(util::exchanges()
        .iter()
        .copied()
        .map(|exchange| {
            let exchange_name = util::exchange_text(exchange);
            let snapshot = health.get(exchange_name);
            let subscriptions = subscription_counts
                .get(exchange_name)
                .copied()
                .unwrap_or((0, 0));
            ExchangeView {
                exchange: exchange_name,
                status: snapshot.map_or_else(|| "unknown".to_owned(), |item| item.0.clone()),
                last_error_code: snapshot.and_then(|item| item.1.clone()),
                last_success_at_ms: snapshot
                    .and_then(|item| item.2)
                    .map(|value| value.timestamp_millis()),
                health_query_latency_ms: snapshot.and_then(|item| item.3),
                instrument_count: instrument_counts.get(exchange_name).copied().unwrap_or(0),
                active_subscriptions: subscriptions.0,
                stale_subscriptions: subscriptions.1,
                capabilities: None,
            }
        })
        .collect())
}

async fn upsert_exchange_health(
    state: &AppState,
    exchange: &str,
    status: &str,
    error_code: Option<&str>,
    last_success_at_ms: Option<i64>,
    health_query_latency_ms: i64,
) -> ApiResult<()> {
    let last_success_at = last_success_at_ms.and_then(DateTime::<Utc>::from_timestamp_millis);
    sqlx::query(
        r#"
        INSERT INTO exchange_health (
            exchange, status, last_error_code, last_success_at, request_latency_ms, observed_at
        )
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (exchange) DO UPDATE SET
            status = EXCLUDED.status,
            last_error_code = EXCLUDED.last_error_code,
            last_success_at = COALESCE(EXCLUDED.last_success_at, exchange_health.last_success_at),
            request_latency_ms = EXCLUDED.request_latency_ms,
            observed_at = EXCLUDED.observed_at
        "#,
    )
    .bind(exchange)
    .bind(status)
    .bind(error_code)
    .bind(last_success_at)
    .bind(health_query_latency_ms)
    .execute(&state.database)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

async fn record_exchange_health_event(
    state: &AppState,
    exchange: &str,
    status: &str,
    error_code: Option<&str>,
    health_query_latency_ms: i64,
) -> ApiResult<()> {
    sqlx::query(
        r#"
        INSERT INTO exchange_health_events (exchange, status, error_code, request_latency_ms)
        SELECT $1, $2, $3, $4
        WHERE NOT EXISTS (
            SELECT 1 FROM (
                SELECT status, error_code
                FROM exchange_health_events
                WHERE exchange = $1
                ORDER BY sequence DESC
                LIMIT 1
            ) latest
            WHERE latest.status = $2
              AND latest.error_code IS NOT DISTINCT FROM $3
        )
        "#,
    )
    .bind(exchange)
    .bind(status)
    .bind(error_code)
    .bind(health_query_latency_ms)
    .execute(&state.database)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

pub(super) async fn get_exchange(
    State(state): State<AppState>,
    Path(exchange): Path<String>,
) -> ApiResult<Json<ExchangeView>> {
    let parsed_exchange = util::parse_exchange(&exchange)?;
    let started = Instant::now();
    let (capabilities, health, instrument_count, subscription_counts) = tokio::join!(
        state.gateway.capabilities(parsed_exchange),
        state.gateway.health(),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*)::bigint FROM instruments WHERE exchange = $1")
            .bind(&exchange)
            .fetch_one(&state.database),
        sqlx::query(
            r#"
            SELECT COUNT(*) FILTER (WHERE status IN ('connecting', 'connected', 'reconnecting'))::bigint AS active,
                   COUNT(*) FILTER (WHERE status = 'stale')::bigint AS stale
            FROM market_subscription_health WHERE exchange = $1
            "#,
        )
        .bind(&exchange)
        .fetch_one(&state.database),
    );
    let health = health.map_err(ApiError::gateway)?;
    let instrument_count = instrument_count.map_err(ApiError::database)?;
    let subscription_counts = subscription_counts.map_err(ApiError::database)?;
    let health_query_latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let adapter = health
        .adapters
        .into_iter()
        .find(|adapter| adapter.exchange == util::exchange_proto_number(parsed_exchange));
    let status = adapter
        .as_ref()
        .map_or_else(|| "unknown".to_owned(), |value| value.status.clone());
    let error_code = adapter
        .as_ref()
        .and_then(|value| value.last_error_code.clone());
    let last_success_at_ms = adapter.and_then(|value| value.last_success_at_ms);
    upsert_exchange_health(
        &state,
        &exchange,
        &status,
        error_code.as_deref(),
        last_success_at_ms,
        health_query_latency_ms,
    )
    .await?;
    record_exchange_health_event(
        &state,
        &exchange,
        &status,
        error_code.as_deref(),
        health_query_latency_ms,
    )
    .await?;
    Ok(Json(ExchangeView {
        exchange: util::exchange_text(parsed_exchange),
        status,
        last_error_code: error_code,
        last_success_at_ms,
        health_query_latency_ms: Some(health_query_latency_ms),
        instrument_count,
        active_subscriptions: subscription_counts
            .try_get("active")
            .map_err(ApiError::database)?,
        stale_subscriptions: subscription_counts
            .try_get("stale")
            .map_err(ApiError::database)?,
        capabilities: capabilities.ok().map(|value| CapabilitiesView {
            spot: value.spot,
            perpetual_linear: value.perpetual_linear,
            perpetual_inverse: value.perpetual_inverse,
            fetch_order_book: value.fetch_order_book,
            watch_order_book: value.watch_order_book,
            watch_trades: value.watch_trades,
        }),
    }))
}

#[derive(Debug, Serialize)]
pub(super) struct ExchangeHealthEventView {
    sequence: i64,
    status: String,
    error_code: Option<String>,
    health_query_latency_ms: Option<i64>,
    observed_at: DateTime<Utc>,
}

pub(super) async fn list_exchange_health_events(
    State(state): State<AppState>,
    Path(exchange): Path<String>,
) -> ApiResult<Json<Vec<ExchangeHealthEventView>>> {
    util::parse_exchange(&exchange)?;
    let rows = sqlx::query(
        "SELECT * FROM exchange_health_events WHERE exchange = $1 ORDER BY observed_at DESC LIMIT 200",
    )
    .bind(exchange)
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?;
    Ok(Json(
        rows.iter()
            .map(|row| ExchangeHealthEventView {
                sequence: row.try_get("sequence").unwrap_or_default(),
                status: row.try_get("status").unwrap_or_default(),
                error_code: row.try_get("error_code").unwrap_or(None),
                health_query_latency_ms: row.try_get("request_latency_ms").unwrap_or(None),
                observed_at: row.try_get("observed_at").unwrap_or_else(|_| Utc::now()),
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize)]
pub(super) struct SubscriptionView {
    instrument_id: Uuid,
    stream_kind: String,
    status: String,
    reconnect_attempt: i32,
    error_code: Option<String>,
    last_event_at: Option<DateTime<Utc>>,
    observed_at: DateTime<Utc>,
}

pub(super) async fn list_exchange_subscriptions(
    State(state): State<AppState>,
    Path(exchange): Path<String>,
) -> ApiResult<Json<Vec<SubscriptionView>>> {
    util::parse_exchange(&exchange)?;
    let rows = sqlx::query(
        "SELECT * FROM market_subscription_health WHERE exchange = $1 ORDER BY observed_at DESC",
    )
    .bind(exchange)
    .fetch_all(&state.database)
    .await
    .map_err(ApiError::database)?;
    Ok(Json(
        rows.iter()
            .map(|row| SubscriptionView {
                instrument_id: row.try_get("instrument_id").unwrap_or_default(),
                stream_kind: row.try_get("stream_kind").unwrap_or_default(),
                status: row.try_get("status").unwrap_or_default(),
                reconnect_attempt: row.try_get("reconnect_attempt").unwrap_or_default(),
                error_code: row.try_get("error_code").unwrap_or(None),
                last_event_at: row.try_get("last_event_at").unwrap_or(None),
                observed_at: row.try_get("observed_at").unwrap_or_else(|_| Utc::now()),
            })
            .collect(),
    ))
}
