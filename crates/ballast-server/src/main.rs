#![forbid(unsafe_code)]

use std::net::SocketAddr;

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use ballast_storage::DatabasePool;
use serde::Serialize;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

#[derive(Clone)]
struct AppState {
    database: DatabasePool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let bind = std::env::var("BALLAST_SERVER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse::<SocketAddr>()?;
    let database_url = std::env::var("DATABASE_URL")?;
    let database = ballast_storage::connect(&database_url).await?;
    ballast_storage::migrate(&database).await?;

    let app = Router::new()
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .with_state(AppState { database });
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
