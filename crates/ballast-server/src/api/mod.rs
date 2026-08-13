mod analytics;
mod error;
mod events;
mod exchanges;
mod history;
mod instruments;
mod live;
mod strategies;
mod tasks;
mod util;

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub(crate) use error::{ApiError, ApiResult};

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(history::routes())
        .route("/api/v1/exchanges", get(exchanges::list_exchanges))
        .route(
            "/api/v1/exchanges/snapshots",
            get(exchanges::exchange_snapshots),
        )
        .route("/api/v1/exchanges/{exchange}", get(exchanges::get_exchange))
        .route(
            "/api/v1/exchanges/{exchange}/health-events",
            get(exchanges::list_exchange_health_events),
        )
        .route(
            "/api/v1/exchanges/{exchange}/subscriptions",
            get(exchanges::list_exchange_subscriptions),
        )
        .route("/api/v1/instruments", get(instruments::list_instruments))
        .route(
            "/api/v1/instruments/sync",
            post(instruments::sync_instruments),
        )
        .route(
            "/api/v1/tasks",
            get(tasks::list_tasks).post(tasks::create_task),
        )
        .route("/api/v1/tasks/{task_id}", get(tasks::get_task))
        .route("/api/v1/tasks/{task_id}/cancel", post(tasks::cancel_task))
        .route("/api/v1/tasks/{task_id}/slices", get(tasks::list_slices))
        .route(
            "/api/v1/strategy-templates",
            get(strategies::list_strategy_templates).post(strategies::create_strategy_template),
        )
        .route(
            "/api/v1/strategy-templates/{template_id}",
            get(strategies::get_strategy_template),
        )
        .route(
            "/api/v1/strategy-templates/{template_id}/versions",
            post(strategies::create_strategy_template_version),
        )
        .route(
            "/api/v1/strategy-templates/{template_id}/archive",
            post(strategies::archive_strategy_template),
        )
        .route(
            "/api/v1/dashboard/operations",
            get(analytics::operations_dashboard),
        )
        .route(
            "/api/v1/analytics/executions",
            get(analytics::execution_analytics),
        )
        .route(
            "/api/v1/native-algorithms",
            get(live::native_algorithm_capabilities),
        )
        .route("/api/v1/live/readiness", get(live::live_readiness))
        .route("/api/v1/accounts", get(live::list_accounts))
        .route("/api/v1/approvals", get(live::private_plane_locked))
        .route("/api/v1/risk", get(live::private_plane_locked))
        .route("/api/v1/hedges", get(live::private_plane_locked))
        .route("/api/v1/events", get(events::list_events))
        .route("/api/v1/ws-tickets", post(crate::auth::issue_ws_ticket))
        .route("/api/v1/ws", get(events::websocket))
}
