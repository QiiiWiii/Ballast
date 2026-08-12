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

mod api;
mod auth;
mod execution_worker;
mod metrics;
mod order_book_cache;
mod order_reconciliation_worker;

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if std::env::var("BALLAST_LIVE_ENABLED").is_ok_and(|value| value == "true") {
        return Err("live execution cannot start before OIDC verification and private reconciliation are implemented".into());
    }

    let bind = std::env::var("BALLAST_SERVER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse::<SocketAddr>()?;
    let database_url = std::env::var("DATABASE_URL")?;
    let database = ballast_storage::connect(&database_url).await?;
    ballast_storage::migrate(&database).await?;
    let gateway_endpoint = std::env::var("BALLAST_GATEWAY_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_owned());
    let gateway = GatewayClient::connect(gateway_endpoint).await?;
    let metrics = metrics::AppMetrics::new()?;
    let auth = auth::AuthState::from_env(database.clone())?;
    let trade_volume = execution_worker::TradeVolumeTracker::default();
    let order_books = order_book_cache::OrderBookCache::default();
    execution_worker::spawn_worker(
        database.clone(),
        gateway.clone(),
        trade_volume,
        order_books,
        metrics.clone(),
    );
    order_reconciliation_worker::spawn_worker(database.clone(), gateway.clone());

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
