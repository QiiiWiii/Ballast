use std::{
    collections::BTreeMap,
    str::FromStr,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{
        Path, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use ballast_core::{Exchange, MarketKind, QuantityUnit, Side, StrategyKind};
use ballast_storage::{
    NewExecutionTask, NewStrategyTemplate, NewStrategyTemplateVersion, StoredExecutionEvent,
    StoredExecutionSlice, StoredExecutionTask, StoredStrategyTemplate,
    StoredStrategyTemplateVersion,
};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/exchanges", get(list_exchanges))
        .route("/api/v1/exchanges/snapshots", get(exchange_snapshots))
        .route("/api/v1/exchanges/{exchange}", get(get_exchange))
        .route(
            "/api/v1/exchanges/{exchange}/health-events",
            get(list_exchange_health_events),
        )
        .route(
            "/api/v1/exchanges/{exchange}/subscriptions",
            get(list_exchange_subscriptions),
        )
        .route("/api/v1/instruments", get(list_instruments))
        .route("/api/v1/instruments/sync", post(sync_instruments))
        .route("/api/v1/tasks", get(list_tasks).post(create_task))
        .route("/api/v1/tasks/{task_id}", get(get_task))
        .route("/api/v1/tasks/{task_id}/cancel", post(cancel_task))
        .route("/api/v1/tasks/{task_id}/slices", get(list_slices))
        .route(
            "/api/v1/strategy-templates",
            get(list_strategy_templates).post(create_strategy_template),
        )
        .route(
            "/api/v1/strategy-templates/{template_id}",
            get(get_strategy_template),
        )
        .route(
            "/api/v1/strategy-templates/{template_id}/versions",
            post(create_strategy_template_version),
        )
        .route(
            "/api/v1/strategy-templates/{template_id}/archive",
            post(archive_strategy_template),
        )
        .route("/api/v1/dashboard/operations", get(operations_dashboard))
        .route("/api/v1/analytics/executions", get(execution_analytics))
        .route(
            "/api/v1/native-algorithms",
            get(native_algorithm_capabilities),
        )
        .route("/api/v1/live/readiness", get(live_readiness))
        .route("/api/v1/accounts", get(private_plane_locked))
        .route("/api/v1/approvals", get(private_plane_locked))
        .route("/api/v1/risk", get(private_plane_locked))
        .route("/api/v1/hedges", get(private_plane_locked))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/ws", get(websocket))
}

#[derive(Debug, Serialize)]
struct NativeAlgorithmCapabilityView {
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

async fn native_algorithm_capabilities(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<NativeAlgorithmCapabilityView>>> {
    let responses = join_all(
        exchanges()
            .iter()
            .copied()
            .map(|exchange| state.gateway.algorithmic_capabilities(exchange)),
    )
    .await;
    let mut result = Vec::new();
    for (exchange, response) in exchanges().iter().copied().zip(responses) {
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
                exchange: exchange_text(exchange),
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

async fn live_readiness() -> Json<Value> {
    Json(json!({
        "enabled": false,
        "private_services": "disabled",
        "oidc": "not_configured",
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

async fn private_plane_locked() -> ApiResult<Json<Value>> {
    Err(ApiError::locked("live_execution_disabled"))
}

#[derive(Debug, Clone, Serialize)]
struct ExchangeView {
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
struct CapabilitiesView {
    spot: bool,
    perpetual_linear: bool,
    perpetual_inverse: bool,
    fetch_order_book: bool,
    watch_order_book: bool,
    watch_trades: bool,
}

async fn list_exchanges(State(state): State<AppState>) -> ApiResult<Json<Vec<ExchangeView>>> {
    let started = Instant::now();
    let capability_results = join_all(
        exchanges()
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
    for (exchange, capability_result) in exchanges().iter().copied().zip(capability_results) {
        let capabilities = capability_result.ok().map(|value| CapabilitiesView {
            spot: value.spot,
            perpetual_linear: value.perpetual_linear,
            perpetual_inverse: value.perpetual_inverse,
            fetch_order_book: value.fetch_order_book,
            watch_order_book: value.watch_order_book,
            watch_trades: value.watch_trades,
        });
        let key = exchange_proto_number(exchange);
        let adapter = health_by_exchange.remove(&key);
        let exchange_name = exchange_text(exchange);
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

async fn exchange_snapshots(State(state): State<AppState>) -> ApiResult<Json<Vec<ExchangeView>>> {
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

async fn list_exchange_snapshots(state: &AppState) -> ApiResult<Vec<ExchangeView>> {
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

    Ok(exchanges()
        .iter()
        .copied()
        .map(|exchange| {
            let exchange_name = exchange_text(exchange);
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

async fn get_exchange(
    State(state): State<AppState>,
    Path(exchange): Path<String>,
) -> ApiResult<Json<ExchangeView>> {
    let parsed_exchange = parse_exchange(&exchange)?;
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
        .find(|adapter| adapter.exchange == exchange_proto_number(parsed_exchange));
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
        exchange: exchange_text(parsed_exchange),
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
struct ExchangeHealthEventView {
    sequence: i64,
    status: String,
    error_code: Option<String>,
    health_query_latency_ms: Option<i64>,
    observed_at: DateTime<Utc>,
}

async fn list_exchange_health_events(
    State(state): State<AppState>,
    Path(exchange): Path<String>,
) -> ApiResult<Json<Vec<ExchangeHealthEventView>>> {
    parse_exchange(&exchange)?;
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
struct SubscriptionView {
    instrument_id: Uuid,
    stream_kind: String,
    status: String,
    reconnect_attempt: i32,
    error_code: Option<String>,
    last_event_at: Option<DateTime<Utc>>,
    observed_at: DateTime<Utc>,
}

async fn list_exchange_subscriptions(
    State(state): State<AppState>,
    Path(exchange): Path<String>,
) -> ApiResult<Json<Vec<SubscriptionView>>> {
    parse_exchange(&exchange)?;
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

#[derive(Debug, Deserialize)]
struct InstrumentQuery {
    exchange: Option<Exchange>,
    market_kind: Option<MarketKind>,
    active_only: Option<bool>,
    search: Option<String>,
    ids: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct InstrumentView {
    id: Uuid,
    exchange: Exchange,
    market_kind: MarketKind,
    symbol: String,
    exchange_symbol: String,
    base_asset: String,
    quote_asset: String,
    settle_asset: Option<String>,
    contract_kind: Option<ballast_core::ContractKind>,
    contract_size: Option<String>,
    price_tick: String,
    quantity_step: String,
    minimum_quantity: Option<String>,
    minimum_notional: Option<String>,
    maker_fee_rate: Option<String>,
    taker_fee_rate: Option<String>,
    active: bool,
    observed_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct InstrumentPageView {
    items: Vec<InstrumentView>,
    total: i64,
    limit: i64,
    offset: i64,
}

impl From<ballast_storage::StoredInstrument> for InstrumentView {
    fn from(stored: ballast_storage::StoredInstrument) -> Self {
        Self {
            id: stored.id,
            exchange: stored.instrument.id.exchange,
            market_kind: stored.instrument.id.market_kind,
            symbol: stored.instrument.id.symbol,
            exchange_symbol: stored.instrument.exchange_symbol,
            base_asset: stored.instrument.base_asset,
            quote_asset: stored.instrument.quote_asset,
            settle_asset: stored.instrument.settle_asset,
            contract_kind: stored.instrument.contract_kind,
            contract_size: decimal_option(stored.instrument.contract_size),
            price_tick: stored.instrument.price_tick.to_string(),
            quantity_step: stored.instrument.quantity_step.to_string(),
            minimum_quantity: decimal_option(stored.instrument.minimum_quantity),
            minimum_notional: decimal_option(stored.instrument.minimum_notional),
            maker_fee_rate: decimal_option(stored.instrument.maker_fee_rate),
            taker_fee_rate: decimal_option(stored.instrument.taker_fee_rate),
            active: stored.instrument.active,
            observed_at: stored.observed_at,
        }
    }
}

async fn list_instruments(
    State(state): State<AppState>,
    Query(query): Query<InstrumentQuery>,
) -> ApiResult<Json<InstrumentPageView>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 250);
    let offset = query.offset.unwrap_or(0).max(0);
    let ids = query.ids.as_deref().map(parse_uuid_list).transpose()?;
    let (instruments, total) = ballast_storage::list_instruments_page(
        &state.database,
        query.exchange,
        query.market_kind,
        query.active_only.unwrap_or(false),
        query.search.as_deref(),
        ids.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(ApiError::database)?;
    Ok(Json(InstrumentPageView {
        items: instruments.into_iter().map(InstrumentView::from).collect(),
        total,
        limit,
        offset,
    }))
}

#[derive(Debug, Deserialize)]
struct SyncInstrumentsRequest {
    exchanges: Option<Vec<Exchange>>,
    reload: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SyncInstrumentsResponse {
    synchronized: BTreeMap<&'static str, usize>,
    failed: BTreeMap<&'static str, &'static str>,
}

async fn sync_instruments(
    State(state): State<AppState>,
    Json(request): Json<SyncInstrumentsRequest>,
) -> ApiResult<Json<SyncInstrumentsResponse>> {
    let requested = request.exchanges.unwrap_or_else(|| exchanges().to_vec());
    if requested.is_empty() {
        return Err(ApiError::validation("exchanges_required", json!({})));
    }
    let results = join_all(requested.iter().copied().map(|exchange| {
        state
            .gateway
            .list_instruments(exchange, None, None, true, request.reload.unwrap_or(false))
    }))
    .await;
    let mut synchronized = BTreeMap::new();
    let mut failed = BTreeMap::new();
    for (exchange, result) in requested.into_iter().zip(results) {
        match result {
            Ok(instruments) => {
                let count = instruments.len();
                ballast_storage::upsert_instruments(&state.database, &instruments)
                    .await
                    .map_err(ApiError::database)?;
                synchronized.insert(exchange_text(exchange), count);
            }
            Err(error) => {
                tracing::warn!(exchange = exchange_text(exchange), %error, "instrument synchronization failed");
                failed.insert(exchange_text(exchange), "gateway_unavailable");
            }
        }
    }
    let Json(_) = list_exchanges(State(state)).await?;
    Ok(Json(SyncInstrumentsResponse {
        synchronized,
        failed,
    }))
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    instrument_id: Uuid,
    side: Side,
    target_amount: String,
    template_version_id: Uuid,
}

async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateTaskRequest>,
) -> ApiResult<(StatusCode, Json<TaskView>)> {
    let idempotency_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty() && value.len() <= 200)
        .ok_or_else(|| ApiError::validation("idempotency_key_required", json!({})))?;
    let target_amount = positive_decimal(&request.target_amount, "target_amount")?;
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
    let strategy = parse_strategy_kind(&template_version.strategy_kind)?;
    let quantity_unit = parse_quantity_unit(&template_version.quantity_unit)?;
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
        || task.side != side_text(request.side)
        || task.strategy_kind != strategy_text(strategy)
        || task.requested_amount != target_amount
        || task.quantity_unit != quantity_unit_text(quantity_unit)
        || task.strategy_params != expected_strategy_params
        || task.max_slippage_bps != template_version.max_slippage_bps
        || task.slice_interval_ms != template_version.slice_interval_ms
    {
        return Err(ApiError::conflict("idempotency_key_conflict"));
    }
    Ok((StatusCode::CREATED, Json(TaskView::from(task))))
}

#[derive(Debug, Deserialize)]
struct ListTaskQuery {
    limit: Option<i64>,
}

async fn list_tasks(
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

async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> ApiResult<Json<TaskView>> {
    let task = ballast_storage::get_task(&state.database, task_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("task_not_found"))?;
    Ok(Json(TaskView::from(task)))
}

async fn cancel_task(
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

async fn list_slices(
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

#[derive(Debug, Deserialize)]
struct EventQuery {
    after_sequence: Option<i64>,
    limit: Option<i64>,
    task_id: Option<Uuid>,
}

async fn list_events(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
) -> ApiResult<Json<Vec<EventView>>> {
    let limit = query.limit.unwrap_or(500).clamp(1, 1_000);
    let events = if let Some(task_id) = query.task_id {
        ballast_storage::list_task_events(&state.database, task_id, limit)
            .await
            .map_err(ApiError::database)?
    } else {
        ballast_storage::list_events_after(
            &state.database,
            query.after_sequence.unwrap_or(0).max(0),
            limit,
        )
        .await
        .map_err(ApiError::database)?
    };
    Ok(Json(events.into_iter().map(EventView::from).collect()))
}

async fn websocket(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade.on_upgrade(move |socket| stream_events(socket, state, query.after_sequence))
}

async fn stream_events(mut socket: WebSocket, state: AppState, after_sequence: Option<i64>) {
    let latest_sequence = match sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(sequence), 0)::bigint FROM execution_events",
    )
    .fetch_one(&state.database)
    .await
    {
        Ok(sequence) => sequence,
        Err(error) => {
            tracing::error!(%error, "websocket initial event cursor query failed");
            return;
        }
    };
    let mut cursor = after_sequence
        .map(|sequence| sequence.max(0).min(latest_sequence))
        .unwrap_or(latest_sequence);
    let ready = json!({ "type": "stream_ready", "data": { "after_sequence": cursor } });
    if socket
        .send(Message::Text(ready.to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    let mut health_counter = 0_u8;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                match ballast_storage::list_events_after(&state.database, cursor, 500).await {
                    Ok(events) => {
                        for event in events {
                            cursor = event.sequence;
                            let message = json!({ "type": "execution_event", "data": EventView::from(event) });
                            if socket.send(Message::Text(message.to_string().into())).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "websocket event query failed");
                        return;
                    }
                }
                health_counter = health_counter.wrapping_add(1);
                if health_counter.is_multiple_of(10) {
                    let gateway_ready = sqlx::query_scalar::<_, bool>(
                        "SELECT COALESCE(bool_and(status IN ('ready', 'ok', 'idle')), false) FROM exchange_health"
                    )
                    .fetch_one(&state.database)
                    .await
                    .unwrap_or(false);
                    let message = json!({
                        "type": "market_health",
                        "data": {
                            "gateway_status": if gateway_ready { "ready" } else { "degraded" },
                            "observed_at": Utc::now()
                        }
                    });
                    if socket.send(Message::Text(message.to_string().into())).await.is_err() {
                        return;
                    }
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Ping(value))) => {
                        if socket.send(Message::Pong(value)).await.is_err() { return; }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct TaskView {
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
struct SliceView {
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
            average_price: decimal_option(value.average_price),
            worst_price: decimal_option(value.worst_price),
            slippage_bps: decimal_option(value.slippage_bps),
            fee_amount: decimal_option(value.fee_amount),
            fee_asset: value.fee_asset,
            fee_status: value.fee_status,
            status: value.status,
            market_snapshot: value.market_snapshot,
            decision_input: value.decision_input,
            created_at: value.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct StrategyTemplateInput {
    name: Option<String>,
    description: Option<String>,
    strategy: StrategyKind,
    quantity_unit: QuantityUnit,
    duration_seconds: u64,
    slice_interval_ms: u64,
    max_slippage_bps: u32,
    participation_rate: Option<String>,
    max_slice_amount: Option<String>,
    change_note: Option<String>,
    execution_backend: ExecutionBackendInput,
    venue_exchange: Option<Exchange>,
    venue_market_kind: Option<MarketKind>,
    native_configuration: Option<NativeAlgorithmInput>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ExecutionBackendInput {
    ManagedIoc,
    VenueNativeAlgo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NativeAlgorithmInput {
    BinanceSpotTwap {
        limit_price: String,
    },
    BinanceUsdmTwap {
        limit_price: String,
        position_side: String,
        reduce_only: bool,
    },
    BinanceUsdmVp {
        limit_price: String,
        urgency: String,
        position_side: String,
        reduce_only: bool,
    },
    OkxTwap {
        trade_mode: String,
        position_side: String,
        size_limit: String,
        price_limit: String,
        time_interval_seconds: u64,
        price_variance_ratio: Option<String>,
        price_spread: Option<String>,
        reduce_only: bool,
    },
}

#[derive(Debug, Serialize)]
struct StrategyTemplateView {
    id: Uuid,
    name: String,
    description: String,
    status: String,
    current_version: i32,
    usage_count: i64,
    versions: Vec<StrategyTemplateVersionView>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct StrategyTemplateVersionView {
    id: Uuid,
    template_id: Uuid,
    version: i32,
    strategy: String,
    quantity_unit: String,
    duration_seconds: i64,
    slice_interval_ms: i64,
    max_slippage_bps: i32,
    participation_rate: Option<String>,
    max_slice_amount: Option<String>,
    execution_backend: String,
    venue_exchange: Option<String>,
    venue_market_kind: Option<String>,
    native_algorithm: Option<String>,
    native_configuration: Option<NativeAlgorithmInput>,
    change_note: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<StoredStrategyTemplateVersion> for StrategyTemplateVersionView {
    type Error = ApiError;

    fn try_from(value: StoredStrategyTemplateVersion) -> Result<Self, Self::Error> {
        let native_configuration = value
            .native_params
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| {
                tracing::error!(%error, version_id = %value.id, "stored native configuration is invalid");
                ApiError::internal("stored_native_configuration_invalid")
            })?;
        Ok(Self {
            id: value.id,
            template_id: value.template_id,
            version: value.version,
            strategy: value.strategy_kind,
            quantity_unit: value.quantity_unit,
            duration_seconds: value.duration_seconds,
            slice_interval_ms: value.slice_interval_ms,
            max_slippage_bps: value.max_slippage_bps,
            participation_rate: decimal_option(value.participation_rate),
            max_slice_amount: decimal_option(value.max_slice_amount),
            execution_backend: value.execution_backend,
            venue_exchange: value.venue_exchange,
            venue_market_kind: value.venue_market_kind,
            native_algorithm: value.native_algorithm,
            native_configuration,
            change_note: value.change_note,
            created_at: value.created_at,
        })
    }
}

async fn template_view(
    state: &AppState,
    template: StoredStrategyTemplate,
) -> ApiResult<StrategyTemplateView> {
    let versions = ballast_storage::list_strategy_template_versions(&state.database, template.id)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(StrategyTemplateVersionView::try_from)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(StrategyTemplateView {
        id: template.id,
        name: template.name,
        description: template.description,
        status: template.status,
        current_version: template.current_version,
        usage_count: template.usage_count,
        versions,
        created_at: template.created_at,
        updated_at: template.updated_at,
    })
}

async fn list_strategy_templates(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<StrategyTemplateView>>> {
    let templates = ballast_storage::list_strategy_templates(&state.database)
        .await
        .map_err(ApiError::database)?;
    let views = join_all(
        templates
            .into_iter()
            .map(|template| template_view(&state, template)),
    )
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(views))
}

async fn get_strategy_template(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> ApiResult<Json<StrategyTemplateView>> {
    let template = ballast_storage::get_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
    Ok(Json(template_view(&state, template).await?))
}

async fn create_strategy_template(
    State(state): State<AppState>,
    Json(input): Json<StrategyTemplateInput>,
) -> ApiResult<(StatusCode, Json<StrategyTemplateView>)> {
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() <= 120)
        .ok_or_else(|| ApiError::validation("strategy_template_name_required", json!({})))?;
    let version = validated_template_version(&input)?;
    let (template, _) = ballast_storage::create_strategy_template(
        &state.database,
        NewStrategyTemplate {
            name: name.to_owned(),
            description: input.description.unwrap_or_default().trim().to_owned(),
            strategy_kind: version.strategy_kind,
            quantity_unit: version.quantity_unit,
            duration_seconds: version.duration_seconds,
            slice_interval_ms: version.slice_interval_ms,
            max_slippage_bps: version.max_slippage_bps,
            participation_rate: version.participation_rate,
            max_slice_amount: version.max_slice_amount,
            change_note: version.change_note,
            execution_backend: version.execution_backend,
            venue_exchange: version.venue_exchange,
            venue_market_kind: version.venue_market_kind,
            native_algorithm: version.native_algorithm,
            native_params: version.native_params,
        },
    )
    .await
    .map_err(ApiError::template_database)?;
    Ok((
        StatusCode::CREATED,
        Json(template_view(&state, template).await?),
    ))
}

async fn create_strategy_template_version(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
    Json(input): Json<StrategyTemplateInput>,
) -> ApiResult<(StatusCode, Json<StrategyTemplateVersionView>)> {
    if ballast_storage::get_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
        .is_none()
    {
        return Err(ApiError::not_found("strategy_template_not_found"));
    }
    let version = ballast_storage::create_strategy_template_version(
        &state.database,
        template_id,
        validated_template_version(&input)?,
    )
    .await
    .map_err(ApiError::template_database)?;
    Ok((
        StatusCode::CREATED,
        Json(StrategyTemplateVersionView::try_from(version)?),
    ))
}

async fn archive_strategy_template(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> ApiResult<Json<StrategyTemplateView>> {
    if !ballast_storage::archive_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
    {
        let existing = ballast_storage::get_strategy_template(&state.database, template_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
        return Ok(Json(template_view(&state, existing).await?));
    }
    let template = ballast_storage::get_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
    Ok(Json(template_view(&state, template).await?))
}

fn validated_template_version(
    input: &StrategyTemplateInput,
) -> ApiResult<NewStrategyTemplateVersion> {
    if input.duration_seconds == 0 || input.duration_seconds > 86_400 {
        return Err(ApiError::validation(
            "duration_out_of_range",
            json!({ "maximum": 86400 }),
        ));
    }
    if input.slice_interval_ms == 0
        || input.slice_interval_ms > input.duration_seconds.saturating_mul(1_000)
    {
        return Err(ApiError::validation(
            "slice_interval_out_of_range",
            json!({}),
        ));
    }
    if input.max_slippage_bps > 10_000 {
        return Err(ApiError::validation(
            "max_slippage_out_of_range",
            json!({ "maximum": 10000 }),
        ));
    }
    let participation_rate = input
        .participation_rate
        .as_deref()
        .map(|value| positive_decimal(value, "participation_rate"))
        .transpose()?;
    match (input.execution_backend, input.strategy) {
        (ExecutionBackendInput::ManagedIoc, StrategyKind::Twap) if participation_rate.is_some() => {
            return Err(ApiError::validation(
                "participation_rate_not_allowed",
                json!({}),
            ));
        }
        (ExecutionBackendInput::ManagedIoc, StrategyKind::Pov) if participation_rate.is_none() => {
            return Err(ApiError::validation(
                "participation_rate_required",
                json!({}),
            ));
        }
        (ExecutionBackendInput::ManagedIoc, StrategyKind::Pov)
            if participation_rate.is_some_and(|value| value > Decimal::ONE) =>
        {
            return Err(ApiError::validation(
                "participation_rate_out_of_range",
                json!({ "maximum": "1" }),
            ));
        }
        (ExecutionBackendInput::VenueNativeAlgo, _) if participation_rate.is_some() => {
            return Err(ApiError::validation(
                "native_participation_rate_not_allowed",
                json!({}),
            ));
        }
        _ => {}
    }
    let max_slice_amount = input
        .max_slice_amount
        .as_deref()
        .map(|value| positive_decimal(value, "max_slice_amount"))
        .transpose()?;
    if input.execution_backend == ExecutionBackendInput::VenueNativeAlgo
        && max_slice_amount.is_some()
    {
        return Err(ApiError::validation(
            "native_max_slice_not_allowed",
            json!({}),
        ));
    }
    let (execution_backend, venue_exchange, venue_market_kind, native_algorithm, native_params) =
        validate_execution_backend(input)?;
    Ok(NewStrategyTemplateVersion {
        strategy_kind: strategy_text(input.strategy).to_owned(),
        quantity_unit: quantity_unit_text(input.quantity_unit).to_owned(),
        duration_seconds: input.duration_seconds as i64,
        slice_interval_ms: input.slice_interval_ms as i64,
        max_slippage_bps: input.max_slippage_bps as i32,
        participation_rate,
        max_slice_amount,
        execution_backend,
        venue_exchange,
        venue_market_kind,
        native_algorithm,
        native_params,
        change_note: input
            .change_note
            .clone()
            .unwrap_or_default()
            .trim()
            .to_owned(),
    })
}

fn validate_execution_backend(
    input: &StrategyTemplateInput,
) -> ApiResult<(
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
)> {
    if input.execution_backend == ExecutionBackendInput::ManagedIoc {
        if input.venue_exchange.is_some()
            || input.venue_market_kind.is_some()
            || input.native_configuration.is_some()
        {
            return Err(ApiError::validation(
                "managed_backend_shape_invalid",
                json!({}),
            ));
        }
        return Ok(("managed_ioc".to_owned(), None, None, None, None));
    }
    let exchange = input
        .venue_exchange
        .ok_or_else(|| ApiError::validation("native_exchange_required", json!({})))?;
    let market = input
        .venue_market_kind
        .ok_or_else(|| ApiError::validation("native_market_kind_required", json!({})))?;
    let config = input
        .native_configuration
        .as_ref()
        .ok_or_else(|| ApiError::validation("native_configuration_required", json!({})))?;
    let algorithm = match config {
        NativeAlgorithmInput::BinanceSpotTwap { limit_price } => {
            require_native_shape(exchange, market, Exchange::Binance, MarketKind::Spot)?;
            positive_decimal(limit_price, "limit_price")?;
            "binance_spot_twap"
        }
        NativeAlgorithmInput::BinanceUsdmTwap { limit_price, .. } => {
            require_native_shape(exchange, market, Exchange::Binance, MarketKind::Perpetual)?;
            positive_decimal(limit_price, "limit_price")?;
            "binance_usdm_twap"
        }
        NativeAlgorithmInput::BinanceUsdmVp {
            limit_price,
            urgency,
            ..
        } => {
            require_native_shape(exchange, market, Exchange::Binance, MarketKind::Perpetual)?;
            positive_decimal(limit_price, "limit_price")?;
            if !matches!(urgency.as_str(), "LOW" | "MEDIUM" | "HIGH") {
                return Err(ApiError::validation("native_urgency_invalid", json!({})));
            }
            if input.strategy != StrategyKind::Pov {
                return Err(ApiError::validation("native_strategy_mismatch", json!({})));
            }
            "binance_usdm_vp"
        }
        NativeAlgorithmInput::OkxTwap {
            size_limit,
            price_limit,
            time_interval_seconds,
            price_variance_ratio,
            price_spread,
            ..
        } => {
            if exchange != Exchange::Okx {
                return Err(ApiError::validation("native_exchange_mismatch", json!({})));
            }
            positive_decimal(size_limit, "size_limit")?;
            positive_decimal(price_limit, "price_limit")?;
            if *time_interval_seconds == 0 {
                return Err(ApiError::validation("native_interval_invalid", json!({})));
            }
            match (price_variance_ratio, price_spread) {
                (Some(value), None) => {
                    positive_decimal(value, "price_variance_ratio")?;
                }
                (None, Some(value)) => {
                    positive_decimal(value, "price_spread")?;
                }
                _ => {
                    return Err(ApiError::validation(
                        "okx_price_variance_required",
                        json!({}),
                    ));
                }
            }
            "okx_twap"
        }
    };
    Ok((
        "venue_native_algo".to_owned(),
        Some(exchange_text(exchange).to_owned()),
        Some(
            match market {
                MarketKind::Spot => "spot",
                MarketKind::Perpetual => "perpetual",
            }
            .to_owned(),
        ),
        Some(algorithm.to_owned()),
        Some(
            serde_json::to_value(config)
                .map_err(|_| ApiError::validation("native_configuration_invalid", json!({})))?,
        ),
    ))
}

fn require_native_shape(
    exchange: Exchange,
    market: MarketKind,
    expected_exchange: Exchange,
    expected_market: MarketKind,
) -> ApiResult<()> {
    if exchange != expected_exchange || market != expected_market {
        return Err(ApiError::validation(
            "native_backend_shape_mismatch",
            json!({}),
        ));
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
struct AnalyticsQuery {
    window: Option<String>,
    exchange: Option<String>,
    strategy: Option<String>,
    instrument_id: Option<Uuid>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnalyticsView {
    window: String,
    task_count: usize,
    active_count: usize,
    completed_count: usize,
    exception_count: usize,
    average_completion_ratio: String,
    median_slippage_bps: Option<String>,
    p95_slippage_bps: Option<String>,
    fee_unavailable_ratio: String,
    median_runtime_ms: Option<i64>,
    status_counts: BTreeMap<String, usize>,
    pause_reason_counts: BTreeMap<String, usize>,
    strategy_counts: BTreeMap<String, usize>,
    exchange_counts: BTreeMap<String, usize>,
}

async fn execution_analytics(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult<Json<AnalyticsView>> {
    Ok(Json(build_analytics(&state, &query).await?))
}

#[derive(Debug, Serialize)]
struct OperationsDashboardView {
    mode: &'static str,
    generated_at: DateTime<Utc>,
    analytics: AnalyticsView,
    urgent_tasks: Vec<TaskView>,
    exchanges: Vec<ExchangeView>,
}

async fn operations_dashboard(
    State(state): State<AppState>,
    Query(mut query): Query<AnalyticsQuery>,
) -> ApiResult<Json<OperationsDashboardView>> {
    if query.window.is_none() {
        query.window = Some("24h".to_owned());
    }
    let analytics = build_analytics(&state, &query).await?;
    let urgent_tasks = ballast_storage::list_tasks(&state.database, 200)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .filter(|task| task_needs_attention(&task.status, task.deadline_at, Utc::now()))
        .take(12)
        .map(TaskView::from)
        .collect();
    let exchanges = list_exchange_snapshots(&state).await?;
    Ok(Json(OperationsDashboardView {
        mode: "paper",
        generated_at: Utc::now(),
        analytics,
        urgent_tasks,
        exchanges,
    }))
}

async fn build_analytics(state: &AppState, query: &AnalyticsQuery) -> ApiResult<AnalyticsView> {
    let window = query.window.as_deref().unwrap_or("24h");
    let since = match window {
        "24h" => Utc::now() - chrono::Duration::hours(24),
        "7d" => Utc::now() - chrono::Duration::days(7),
        "30d" => Utc::now() - chrono::Duration::days(30),
        _ => {
            return Err(ApiError::validation(
                "analytics_window_invalid",
                json!({ "allowed": ["24h", "7d", "30d"] }),
            ));
        }
    };
    if let Some(exchange) = query.exchange.as_deref() {
        parse_exchange(exchange)?;
    }
    if let Some(strategy) = query.strategy.as_deref()
        && !matches!(strategy, "twap" | "pov")
    {
        return Err(ApiError::validation("strategy_invalid", json!({})));
    }
    let instruments = ballast_storage::list_instruments(&state.database)
        .await
        .map_err(ApiError::database)?;
    let instrument_exchange: BTreeMap<Uuid, String> = instruments
        .into_iter()
        .map(|instrument| {
            (
                instrument.id,
                exchange_text(instrument.instrument.id.exchange).to_owned(),
            )
        })
        .collect();
    let tasks: Vec<_> = ballast_storage::list_tasks(&state.database, 5_000)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .filter(|task| task.created_at >= since)
        .filter(|task| {
            query.exchange.as_ref().is_none_or(|exchange| {
                instrument_exchange.get(&task.instrument_id) == Some(exchange)
            })
        })
        .filter(|task| {
            query
                .strategy
                .as_ref()
                .is_none_or(|strategy| &task.strategy_kind == strategy)
        })
        .filter(|task| {
            query
                .instrument_id
                .is_none_or(|instrument_id| task.instrument_id == instrument_id)
        })
        .filter(|task| {
            query
                .status
                .as_ref()
                .is_none_or(|status| &task.status == status)
        })
        .collect();
    let mut status_counts = BTreeMap::new();
    let mut pause_reason_counts = BTreeMap::new();
    let mut strategy_counts = BTreeMap::new();
    let mut exchange_counts = BTreeMap::new();
    let mut completion_total = Decimal::ZERO;
    let mut runtimes = Vec::new();
    for task in &tasks {
        *status_counts.entry(task.status.clone()).or_insert(0) += 1;
        *strategy_counts
            .entry(task.strategy_kind.clone())
            .or_insert(0) += 1;
        if let Some(exchange) = instrument_exchange.get(&task.instrument_id) {
            *exchange_counts.entry(exchange.clone()).or_insert(0) += 1;
        }
        if let Some(reason) = &task.paused_reason {
            *pause_reason_counts.entry(reason.clone()).or_insert(0) += 1;
        }
        completion_total += if task.requested_amount.is_zero() {
            Decimal::ZERO
        } else {
            (task.executed_amount / task.requested_amount).min(Decimal::ONE)
        };
        if let Some(started_at) = task.started_at {
            runtimes.push((task.updated_at - started_at).num_milliseconds().max(0));
        }
    }
    let task_ids: Vec<Uuid> = tasks.iter().map(|task| task.id).collect();
    let rows = if task_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query("SELECT slippage_bps, fee_status FROM execution_slices WHERE task_id = ANY($1)")
            .bind(&task_ids)
            .fetch_all(&state.database)
            .await
            .map_err(ApiError::database)?
    };
    let mut slippages: Vec<Decimal> = rows
        .iter()
        .filter_map(|row| row.try_get("slippage_bps").ok())
        .collect();
    slippages.sort();
    runtimes.sort_unstable();
    let fee_unavailable = rows
        .iter()
        .filter(|row| {
            row.try_get::<String, _>("fee_status")
                .is_ok_and(|status| status == "unavailable")
        })
        .count();
    let count = tasks.len();
    let active_count = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status.as_str(),
                "scheduled" | "running" | "paused" | "cancelling"
            )
        })
        .count();
    let completed_count = tasks
        .iter()
        .filter(|task| task.status == "completed")
        .count();
    let exception_count = tasks
        .iter()
        .filter(|task| matches!(task.status.as_str(), "paused" | "expired" | "failed"))
        .count();
    Ok(AnalyticsView {
        window: window.to_owned(),
        task_count: count,
        active_count,
        completed_count,
        exception_count,
        average_completion_ratio: if count == 0 {
            Decimal::ZERO
        } else {
            completion_total / Decimal::from(count as u64)
        }
        .to_string(),
        median_slippage_bps: percentile(&slippages, 50).map(|value| value.to_string()),
        p95_slippage_bps: percentile(&slippages, 95).map(|value| value.to_string()),
        fee_unavailable_ratio: if rows.is_empty() {
            Decimal::ZERO
        } else {
            Decimal::from(fee_unavailable as u64) / Decimal::from(rows.len() as u64)
        }
        .to_string(),
        median_runtime_ms: runtimes.get(runtimes.len().saturating_sub(1) / 2).copied(),
        status_counts,
        pause_reason_counts,
        strategy_counts,
        exchange_counts,
    })
}

fn percentile(values: &[Decimal], percentile: usize) -> Option<Decimal> {
    if values.is_empty() {
        return None;
    }
    let index = ((values.len() - 1) * percentile).div_ceil(100);
    values.get(index).copied()
}

#[derive(Debug, Serialize)]
struct EventView {
    sequence: i64,
    event_id: Uuid,
    task_id: Option<Uuid>,
    event_type: String,
    payload: Value,
    created_at: DateTime<Utc>,
}

impl From<StoredExecutionEvent> for EventView {
    fn from(value: StoredExecutionEvent) -> Self {
        Self {
            sequence: value.sequence,
            event_id: value.event_id,
            task_id: value.task_id,
            event_type: value.event_type,
            payload: value.payload,
            created_at: value.created_at,
        }
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    params: Value,
}

impl ApiError {
    fn validation(code: &'static str, params: Value) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code,
            params,
        }
    }

    fn not_found(code: &'static str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            params: json!({}),
        }
    }

    fn conflict(code: &'static str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            params: json!({}),
        }
    }

    fn locked(code: &'static str) -> Self {
        Self {
            status: StatusCode::LOCKED,
            code,
            params: json!({}),
        }
    }

    fn database(error: sqlx::Error) -> Self {
        tracing::error!(%error, "database operation failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "database_error",
            params: json!({}),
        }
    }

    fn internal(code: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            params: json!({}),
        }
    }

    fn template_database(error: sqlx::Error) -> Self {
        if error
            .as_database_error()
            .and_then(|database| database.code())
            .is_some_and(|code| code == "23505")
        {
            return Self::conflict("strategy_template_name_conflict");
        }
        if matches!(&error, sqlx::Error::Protocol(message) if message.contains("archived")) {
            return Self::conflict("strategy_template_archived");
        }
        Self::database(error)
    }

    fn gateway(error: ballast_gateway_client::GatewayClientError) -> Self {
        tracing::warn!(%error, "gateway operation failed");
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "gateway_unavailable",
            params: json!({}),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": { "code": self.code, "params": self.params } })),
        )
            .into_response()
    }
}

fn positive_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::validation("invalid_decimal", json!({ "field": field })))?;
    if parsed <= Decimal::ZERO {
        return Err(ApiError::validation(
            "decimal_must_be_positive",
            json!({ "field": field }),
        ));
    }
    Ok(parsed)
}

fn decimal_option(value: Option<Decimal>) -> Option<String> {
    value.map(|value| value.to_string())
}

fn parse_uuid_list(value: &str) -> ApiResult<Vec<Uuid>> {
    value
        .split(',')
        .filter(|item| !item.is_empty())
        .map(|item| {
            Uuid::parse_str(item).map_err(|_| {
                ApiError::validation("invalid_instrument_id", json!({ "instrument_id": item }))
            })
        })
        .collect()
}

fn task_needs_attention(status: &str, deadline_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    status == "paused"
        || status == "failed"
        || (matches!(
            status,
            "pending_approval" | "scheduled" | "running" | "cancelling"
        ) && deadline_at <= now + chrono::Duration::minutes(10))
}

fn parse_exchange(value: &str) -> ApiResult<Exchange> {
    match value {
        "binance" => Ok(Exchange::Binance),
        "okx" => Ok(Exchange::Okx),
        "bybit" => Ok(Exchange::Bybit),
        "gate_io" => Ok(Exchange::GateIo),
        "bitget" => Ok(Exchange::Bitget),
        _ => Err(ApiError::not_found("exchange_not_found")),
    }
}

fn parse_strategy_kind(value: &str) -> ApiResult<StrategyKind> {
    match value {
        "twap" => Ok(StrategyKind::Twap),
        "pov" => Ok(StrategyKind::Pov),
        _ => Err(ApiError::validation("strategy_invalid", json!({}))),
    }
}

fn parse_quantity_unit(value: &str) -> ApiResult<QuantityUnit> {
    match value {
        "base_quantity" => Ok(QuantityUnit::BaseQuantity),
        "quote_notional" => Ok(QuantityUnit::QuoteNotional),
        "contracts" => Ok(QuantityUnit::Contracts),
        _ => Err(ApiError::validation("quantity_unit_invalid", json!({}))),
    }
}

const fn exchanges() -> &'static [Exchange; 5] {
    &[
        Exchange::Binance,
        Exchange::Okx,
        Exchange::Bybit,
        Exchange::GateIo,
        Exchange::Bitget,
    ]
}

const fn exchange_text(value: Exchange) -> &'static str {
    match value {
        Exchange::Binance => "binance",
        Exchange::Okx => "okx",
        Exchange::Bybit => "bybit",
        Exchange::GateIo => "gate_io",
        Exchange::Bitget => "bitget",
    }
}

const fn exchange_proto_number(value: Exchange) -> i32 {
    match value {
        Exchange::Binance => 1,
        Exchange::Okx => 2,
        Exchange::Bybit => 3,
        Exchange::GateIo => 4,
        Exchange::Bitget => 5,
    }
}

const fn side_text(value: Side) -> &'static str {
    match value {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

const fn strategy_text(value: StrategyKind) -> &'static str {
    match value {
        StrategyKind::Twap => "twap",
        StrategyKind::Pov => "pov",
    }
}

const fn quantity_unit_text(value: QuantityUnit) -> &'static str {
    match value {
        QuantityUnit::BaseQuantity => "base_quantity",
        QuantityUnit::QuoteNotional => "quote_notional",
        QuantityUnit::Contracts => "contracts",
    }
}

#[cfg(test)]
mod tests {
    use super::task_needs_attention;
    use chrono::{Duration, Utc};

    #[test]
    fn attention_queue_excludes_terminal_tasks_with_past_deadlines() {
        let now = Utc::now();
        assert!(!task_needs_attention(
            "completed",
            now - Duration::hours(1),
            now
        ));
        assert!(!task_needs_attention(
            "cancelled",
            now - Duration::hours(1),
            now
        ));
        assert!(task_needs_attention(
            "running",
            now + Duration::minutes(5),
            now
        ));
        assert!(!task_needs_attention(
            "running",
            now + Duration::minutes(20),
            now
        ));
        assert!(task_needs_attention(
            "paused",
            now + Duration::hours(1),
            now
        ));
        assert!(task_needs_attention(
            "failed",
            now - Duration::hours(1),
            now
        ));
    }
}
