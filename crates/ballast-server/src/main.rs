#![forbid(unsafe_code)]

use std::net::SocketAddr;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    routing::get,
};
use ballast_gateway_client::GatewayClient;
use ballast_storage::DatabasePool;
use serde::Serialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

mod account_reconciliation_worker;
mod api;
mod auth;
mod execution_worker;
mod metrics;
mod order_book_cache;
mod order_reconciliation_worker;
mod private_event_worker;
mod reconciliation_alert_config;
mod reconciliation_alert_worker;
mod risk;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

#[derive(Clone)]
pub(crate) struct AppState {
    database: DatabasePool,
    gateway: GatewayClient,
    metrics: metrics::AppMetrics,
    auth: auth::AuthState,
    pub(crate) live_enabled: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let live_requested = live_enabled_requested()?;

    let bind = std::env::var("BALLAST_SERVER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse::<SocketAddr>()?;
    let database_url = std::env::var("DATABASE_URL")?;
    let database = ballast_storage::connect(&database_url).await?;
    ballast_storage::migrate(&database).await?;
    let private_reconciliation = private_reconciliation_enabled()?;
    let gateway_endpoint = std::env::var("BALLAST_GATEWAY_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_owned());
    let gateway = GatewayClient::connect(gateway_endpoint).await?;
    let metrics = metrics::AppMetrics::new()?;
    let auth = auth::AuthState::from_env(database.clone())?;
    let reconciliation_alert_config =
        reconciliation_alert_config::ReconciliationAlertConfig::from_env()
            .map_err(|error| format!("invalid reconciliation alert configuration: {error}"))?;
    if live_requested {
        validate_live_prerequisites(
            &database,
            &gateway,
            &auth,
            private_reconciliation,
            reconciliation_alert_config.as_ref(),
        )
        .await?;
    }
    if let Some(config) = reconciliation_alert_config.clone() {
        reconciliation_alert_worker::spawn_worker(database.clone(), config);
    } else {
        info!("reconciliation alert webhooks are disabled");
    }
    let trade_volume = execution_worker::TradeVolumeTracker::default();
    let order_books = order_book_cache::OrderBookCache::default();
    execution_worker::spawn_worker(
        database.clone(),
        gateway.clone(),
        trade_volume,
        order_books,
        metrics.clone(),
    );
    if private_reconciliation {
        account_reconciliation_worker::spawn_worker(
            database.clone(),
            gateway.clone(),
            reconciliation_alert_config.is_some(),
        );
        order_reconciliation_worker::spawn_worker(database.clone(), gateway.clone());
        private_event_worker::spawn_worker(database.clone(), gateway.clone());
    } else {
        info!("private reconciliation workers are disabled");
    }

    let cors_origin = std::env::var("BALLAST_CORS_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173".to_owned())
        .parse::<HeaderValue>()?;
    let cors = CorsLayer::new()
        .allow_origin(cors_origin)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static("idempotency-key"),
        ]);

    let app_state = AppState {
        database,
        gateway,
        metrics,
        auth,
        live_enabled: live_requested,
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics::endpoint))
        .merge(api::routes())
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            auth::auth_middleware,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);
    let listener = tokio::net::TcpListener::bind(bind).await?;

    info!(%bind, "ballast server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn private_reconciliation_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    let value = std::env::var("BALLAST_PRIVATE_RECONCILIATION_ENABLED").ok();
    parse_enabled_flag(value.as_deref()).map_err(Into::into)
}

fn live_enabled_requested() -> Result<bool, &'static str> {
    let value = match std::env::var("BALLAST_LIVE_ENABLED") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("BALLAST_LIVE_ENABLED must be valid UTF-8");
        }
    };
    live_enabled_from_value(value.as_deref())
}

fn live_enabled_from_value(value: Option<&str>) -> Result<bool, &'static str> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err("BALLAST_LIVE_ENABLED must be true or false"),
    }
}

async fn validate_live_prerequisites(
    database: &DatabasePool,
    gateway: &GatewayClient,
    auth: &auth::AuthState,
    private_reconciliation: bool,
    alert_config: Option<&reconciliation_alert_config::ReconciliationAlertConfig>,
) -> Result<(), Box<dyn std::error::Error>> {
    if auth.mode != auth::AuthMode::Oidc {
        return Err("BALLAST_LIVE_ENABLED requires OIDC issuer and audience".into());
    }
    if !private_reconciliation {
        return Err("BALLAST_LIVE_ENABLED requires private reconciliation".into());
    }
    if alert_config.is_none() {
        return Err("BALLAST_LIVE_ENABLED requires an execution alert webhook".into());
    }
    let capabilities = gateway
        .trading_capabilities(ballast_core::Exchange::Okx)
        .await?;
    if !capabilities.private_order_stream || !capabilities.private_fill_stream {
        return Err("BALLAST_LIVE_ENABLED requires private order and fill streams".into());
    }
    let accounts = ballast_storage::list_accounts(database).await?;
    if !accounts
        .iter()
        .any(|account| account.enabled && account.exchange == "okx")
    {
        return Err("BALLAST_LIVE_ENABLED requires an enabled private account".into());
    }
    if accounts
        .iter()
        .filter(|account| account.enabled)
        .any(|account| !account.withdrawals_disabled || !account.ip_restricted)
    {
        return Err(
            "enabled private accounts must disable withdrawals and restrict source IPs".into(),
        );
    }
    let limits = ballast_storage::list_risk_limits(database).await?;
    if !limits
        .iter()
        .any(|limit| limit.scope_type == "global" && limit.scope_id == "global")
        || !limits.iter().any(|limit| limit.scope_type == "exchange")
        || !limits.iter().any(|limit| limit.scope_type == "account")
    {
        return Err(
            "BALLAST_LIVE_ENABLED requires global, exchange, and account risk limits".into(),
        );
    }
    let switches = ballast_storage::list_kill_switches(database).await?;
    if !switches
        .iter()
        .any(|switch| switch.scope_type == "global" && switch.scope_id == "global")
    {
        return Err("BALLAST_LIVE_ENABLED requires a global kill switch record".into());
    }
    Ok(())
}

fn parse_enabled_flag(value: Option<&str>) -> Result<bool, &'static str> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err("BALLAST_PRIVATE_RECONCILIATION_ENABLED must be true or false"),
    }
}

async fn health(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    if let Err(error) = ballast_storage::ping(&state.database).await {
        tracing::error!(%error, "database health check failed");
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unavailable",
                service: "ballast-server",
                version: env!("CARGO_PKG_VERSION"),
            }),
        ));
    }

    Ok(Json(HealthResponse {
        status: "ok",
        service: "ballast-server",
        version: env!("CARGO_PKG_VERSION"),
    }))
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install shutdown signal handler");
    }
}

#[cfg(test)]
mod tests {
    use super::{live_enabled_from_value, parse_enabled_flag};

    #[test]
    fn reconciliation_flag_is_explicit() {
        assert_eq!(parse_enabled_flag(None), Ok(false));
        assert_eq!(parse_enabled_flag(Some("false")), Ok(false));
        assert_eq!(parse_enabled_flag(Some("true")), Ok(true));
        assert!(parse_enabled_flag(Some("1")).is_err());
    }

    #[test]
    fn live_flag_is_explicit() {
        assert_eq!(live_enabled_from_value(None), Ok(false));
        assert_eq!(live_enabled_from_value(Some("true")), Ok(true));
        assert!(live_enabled_from_value(Some("1")).is_err());
    }
}
