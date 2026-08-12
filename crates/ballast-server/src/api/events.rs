use std::time::Duration;

use axum::{
    Json,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
};
use ballast_storage::StoredExecutionEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, ApiResult};

#[derive(Debug, Deserialize)]
pub(super) struct EventQuery {
    after_sequence: Option<i64>,
    limit: Option<i64>,
    task_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub(super) struct EventView {
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

pub(super) async fn list_events(
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

pub(super) async fn websocket(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    upgrade
        .protocols([crate::auth::WS_TICKET_SUBPROTOCOL])
        .on_upgrade(move |socket| stream_events(socket, state, query.after_sequence))
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
                        if socket.send(Message::Pong(value)).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    _ => {}
                }
            }
        }
    }
}
