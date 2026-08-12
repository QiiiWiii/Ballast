use std::{collections::HashMap, sync::Arc, time::Duration};

use ballast_core::Instrument;
use ballast_gateway_client::{GatewayClient, OrderBook, proto};
use ballast_storage::DatabasePool;
use chrono::{DateTime, Utc};
use tokio::sync::{Mutex, RwLock};
use tracing::warn;
use uuid::Uuid;

use crate::metrics::AppMetrics;

/// Prefer a shared snapshot at most this old before issuing another REST fetch.
const REST_CACHE_FRESH_MS: i64 = 750;
/// Execution must not use books older than this (matches paper worker stale threshold).
pub const MAX_BOOK_AGE_MS: i64 = 5_000;
const ORDER_BOOK_DEPTH: u32 = 50;

#[derive(Clone, Default)]
pub struct OrderBookCache {
    entries: Arc<Mutex<HashMap<Uuid, Arc<BookEntry>>>>,
}

struct BookEntry {
    book: RwLock<Option<OrderBook>>,
    connected: RwLock<bool>,
    rest_lock: Mutex<()>,
}

impl Default for BookEntry {
    fn default() -> Self {
        Self {
            book: RwLock::new(None),
            connected: RwLock::new(false),
            rest_lock: Mutex::new(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderBookSource {
    /// Fresh snapshot already present, last updated by the websocket stream.
    CacheWebsocket,
    /// Fresh snapshot already present, last updated by REST.
    CacheRest,
    /// Another concurrent waiter already populated the cache under the REST lock.
    CacheCoalesced,
    /// Issued a REST fetch against the gateway.
    RestFetch,
}

impl OrderBookSource {
    pub const fn as_metric_label(self) -> &'static str {
        match self {
            Self::CacheWebsocket => "cache_websocket",
            Self::CacheRest => "cache_rest",
            Self::CacheCoalesced => "cache_coalesced",
            Self::RestFetch => "rest_fetch",
        }
    }
}

#[derive(Debug)]
pub enum OrderBookCacheError {
    Gateway(ballast_gateway_client::GatewayClientError),
}

impl std::fmt::Display for OrderBookCacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gateway(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for OrderBookCacheError {}

impl OrderBookCache {
    pub async fn ensure_subscription(
        &self,
        instrument_id: Uuid,
        instrument: Instrument,
        gateway: GatewayClient,
        database: DatabasePool,
    ) {
        let mut entries = self.entries.lock().await;
        if entries.contains_key(&instrument_id) {
            return;
        }
        let entry = Arc::new(BookEntry::default());
        entries.insert(instrument_id, entry.clone());
        drop(entries);
        tokio::spawn(run_order_book_subscription(
            entry,
            instrument_id,
            instrument,
            gateway,
            database,
        ));
    }

    pub async fn get(
        &self,
        instrument_id: Uuid,
        instrument: &Instrument,
        gateway: &GatewayClient,
        metrics: &AppMetrics,
    ) -> Result<OrderBook, OrderBookCacheError> {
        let entry = self.entry(instrument_id).await;
        if let Some(book) = self.usable_snapshot(&entry, REST_CACHE_FRESH_MS).await {
            let source = if *entry.connected.read().await {
                OrderBookSource::CacheWebsocket
            } else {
                OrderBookSource::CacheRest
            };
            metrics
                .order_book_lookups
                .with_label_values(&[source.as_metric_label()])
                .inc();
            return Ok(book);
        }

        let _guard = entry.rest_lock.lock().await;
        if let Some(book) = self.usable_snapshot(&entry, REST_CACHE_FRESH_MS).await {
            metrics
                .order_book_lookups
                .with_label_values(&[OrderBookSource::CacheCoalesced.as_metric_label()])
                .inc();
            return Ok(book);
        }

        let book = gateway
            .get_order_book(&instrument.id, ORDER_BOOK_DEPTH)
            .await
            .map_err(OrderBookCacheError::Gateway)?;
        *entry.book.write().await = Some(book.clone());
        metrics
            .order_book_lookups
            .with_label_values(&[OrderBookSource::RestFetch.as_metric_label()])
            .inc();
        Ok(book)
    }

    async fn entry(&self, instrument_id: Uuid) -> Arc<BookEntry> {
        let mut entries = self.entries.lock().await;
        entries
            .entry(instrument_id)
            .or_insert_with(|| Arc::new(BookEntry::default()))
            .clone()
    }

    async fn usable_snapshot(&self, entry: &BookEntry, max_age_ms: i64) -> Option<OrderBook> {
        let book = entry.book.read().await.clone()?;
        let age_ms = Utc::now().timestamp_millis() - book.gateway_received_at_ms;
        if age_ms <= max_age_ms {
            Some(book)
        } else {
            None
        }
    }
}

async fn run_order_book_subscription(
    entry: Arc<BookEntry>,
    instrument_id: Uuid,
    instrument: Instrument,
    gateway: GatewayClient,
    database: DatabasePool,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match gateway
            .watch_order_book(&instrument.id, ORDER_BOOK_DEPTH)
            .await
        {
            Ok(mut stream) => {
                while let Ok(Some(event)) = stream.message().await {
                    match event.payload {
                        Some(proto::order_book_stream_event::Payload::Status(status)) => {
                            let connected = status.state == proto::StreamState::Connected as i32;
                            *entry.connected.write().await = connected;
                            if let Err(error) = upsert_subscription_health(
                                &database,
                                &instrument,
                                instrument_id,
                                stream_state_text(status.state),
                                status.reconnect_attempt as i32,
                                status.error_code.as_deref(),
                                None,
                            )
                            .await
                            {
                                warn!(%error, "failed to persist order book subscription status");
                            }
                            if connected {
                                backoff = Duration::from_secs(1);
                            }
                        }
                        Some(proto::order_book_stream_event::Payload::OrderBook(value)) => {
                            match ballast_gateway_client::order_book_from_proto(value) {
                                Ok(book) => {
                                    let received_at = book.gateway_received_at_ms;
                                    *entry.book.write().await = Some(book);
                                    *entry.connected.write().await = true;
                                    if let Err(error) = upsert_subscription_health(
                                        &database,
                                        &instrument,
                                        instrument_id,
                                        "connected",
                                        0,
                                        None,
                                        DateTime::<Utc>::from_timestamp_millis(received_at),
                                    )
                                    .await
                                    {
                                        warn!(
                                            %error,
                                            "failed to persist order book subscription activity"
                                        );
                                    }
                                }
                                Err(error) => {
                                    warn!(%error, "discarding invalid gateway order book")
                                }
                            }
                        }
                        None => {}
                    }
                }
            }
            Err(error) => {
                warn!(
                    %error,
                    instrument = %instrument.id.symbol,
                    "order book subscription failed"
                )
            }
        }
        *entry.connected.write().await = false;
        if let Err(error) = upsert_subscription_health(
            &database,
            &instrument,
            instrument_id,
            "closed",
            0,
            Some("stream_ended"),
            None,
        )
        .await
        {
            warn!(%error, "failed to persist closed order book subscription");
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

async fn upsert_subscription_health(
    database: &DatabasePool,
    instrument: &Instrument,
    instrument_id: Uuid,
    status: &str,
    reconnect_attempt: i32,
    error_code: Option<&str>,
    last_event_at: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO market_subscription_health (
            exchange, instrument_id, stream_kind, status, reconnect_attempt,
            error_code, last_event_at, observed_at
        ) VALUES ($1, $2, 'order_book', $3, $4, $5, $6, now())
        ON CONFLICT (instrument_id, stream_kind) DO UPDATE SET
            status = EXCLUDED.status,
            reconnect_attempt = EXCLUDED.reconnect_attempt,
            error_code = EXCLUDED.error_code,
            last_event_at = COALESCE(EXCLUDED.last_event_at, market_subscription_health.last_event_at),
            observed_at = now()
        "#,
    )
    .bind(exchange_text(instrument.id.exchange))
    .bind(instrument_id)
    .bind(status)
    .bind(reconnect_attempt)
    .bind(error_code)
    .bind(last_event_at)
    .execute(database)
    .await?;
    Ok(())
}

fn stream_state_text(value: i32) -> &'static str {
    match proto::StreamState::try_from(value).unwrap_or(proto::StreamState::Closed) {
        proto::StreamState::Connecting | proto::StreamState::Unspecified => "connecting",
        proto::StreamState::Connected => "connected",
        proto::StreamState::Reconnecting => "reconnecting",
        proto::StreamState::Stale => "stale",
        proto::StreamState::Closed => "closed",
    }
}

const fn exchange_text(value: ballast_core::Exchange) -> &'static str {
    match value {
        ballast_core::Exchange::Binance => "binance",
        ballast_core::Exchange::Okx => "okx",
        ballast_core::Exchange::Bybit => "bybit",
        ballast_core::Exchange::GateIo => "gate_io",
        ballast_core::Exchange::Bitget => "bitget",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_cache_window_is_tighter_than_execution_stale_threshold() {
        assert!(REST_CACHE_FRESH_MS < MAX_BOOK_AGE_MS);
        assert_eq!(MAX_BOOK_AGE_MS, 5_000);
    }
}
