use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

use super::{
    ApiError, ApiResult,
    exchanges::{self, ExchangeView},
    tasks::TaskView,
    util,
};

#[derive(Debug, Default, Deserialize)]
pub(super) struct AnalyticsQuery {
    window: Option<String>,
    exchange: Option<String>,
    strategy: Option<String>,
    instrument_id: Option<Uuid>,
    status: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct AnalyticsView {
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

pub(super) async fn execution_analytics(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult<Json<AnalyticsView>> {
    Ok(Json(build_analytics(&state, &query).await?))
}

#[derive(Debug, Serialize)]
pub(super) struct OperationsDashboardView {
    mode: &'static str,
    generated_at: DateTime<Utc>,
    analytics: AnalyticsView,
    urgent_tasks: Vec<TaskView>,
    exchanges: Vec<ExchangeView>,
}

pub(super) async fn operations_dashboard(
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
    let exchanges = exchanges::list_exchange_snapshots(&state).await?;
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
        util::parse_exchange(exchange)?;
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
                util::exchange_text(instrument.instrument.id.exchange).to_owned(),
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

fn task_needs_attention(status: &str, deadline_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    status == "paused"
        || status == "failed"
        || (matches!(
            status,
            "pending_approval" | "scheduled" | "running" | "cancelling"
        ) && deadline_at <= now + chrono::Duration::minutes(10))
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
