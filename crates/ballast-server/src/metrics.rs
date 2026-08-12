use std::sync::Arc;

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use prometheus::{
    Encoder, Histogram, HistogramOpts, IntCounter, IntCounterVec, IntGauge, Opts, Registry,
    TextEncoder,
};

use crate::AppState;

#[derive(Clone)]
pub struct AppMetrics {
    registry: Arc<Registry>,
    pub claimed_tasks: IntCounter,
    pub worker_batch_size: IntGauge,
    pub paused_tasks: IntCounterVec,
    pub slices: IntCounterVec,
    pub slice_slippage_bps: Histogram,
    pub order_book_lookups: IntCounterVec,
    pub instrument_tick_serialized: IntCounter,
    database_pool_size: IntGauge,
    database_pool_idle: IntGauge,
}

impl AppMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let metrics = Self {
            registry: Arc::new(Registry::new()),
            claimed_tasks: IntCounter::new(
                "ballast_execution_tasks_claimed_total",
                "Paper tasks claimed by the worker",
            )?,
            worker_batch_size: IntGauge::new(
                "ballast_execution_worker_batch_size",
                "Most recent worker claim batch",
            )?,
            paused_tasks: IntCounterVec::new(
                Opts::new(
                    "ballast_execution_paused_total",
                    "Task pauses by explicit reason",
                ),
                &["reason"],
            )?,
            slices: IntCounterVec::new(
                Opts::new(
                    "ballast_execution_slices_total",
                    "Paper IOC slices by result",
                ),
                &["status"],
            )?,
            slice_slippage_bps: Histogram::with_opts(
                HistogramOpts::new(
                    "ballast_execution_slice_slippage_bps",
                    "Paper slice slippage in basis points",
                )
                .buckets(vec![0.0, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0]),
            )?,
            order_book_lookups: IntCounterVec::new(
                Opts::new(
                    "ballast_order_book_lookups_total",
                    "Order book lookups by source",
                ),
                &["source"],
            )?,
            instrument_tick_serialized: IntCounter::new(
                "ballast_execution_instrument_tick_serialized_total",
                "Paper ticks that waited on a same-instrument lock",
            )?,
            database_pool_size: IntGauge::new(
                "ballast_database_pool_size",
                "Open PostgreSQL connections",
            )?,
            database_pool_idle: IntGauge::new(
                "ballast_database_pool_idle",
                "Idle PostgreSQL connections",
            )?,
        };
        metrics
            .registry
            .register(Box::new(metrics.claimed_tasks.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.worker_batch_size.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.paused_tasks.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.slices.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.slice_slippage_bps.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.order_book_lookups.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.instrument_tick_serialized.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.database_pool_size.clone()))?;
        metrics
            .registry
            .register(Box::new(metrics.database_pool_idle.clone()))?;
        Ok(metrics)
    }

    fn encode(&self, state: &AppState) -> Result<Vec<u8>, prometheus::Error> {
        self.database_pool_size
            .set(i64::from(state.database.size()));
        self.database_pool_idle
            .set(state.database.num_idle() as i64);
        let mut output = Vec::new();
        TextEncoder::new().encode(&self.registry.gather(), &mut output)?;
        Ok(output)
    }
}

pub async fn endpoint(State(state): State<AppState>) -> impl IntoResponse {
    match state.metrics.encode(&state) {
        Ok(output) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, TextEncoder::new().format_type())],
            output,
        )
            .into_response(),
        Err(error) => {
            tracing::error!(%error, "failed to encode prometheus metrics");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
