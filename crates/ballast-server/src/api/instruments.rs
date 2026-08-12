use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Query, State},
};
use ballast_core::{Exchange, MarketKind};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, ApiResult, exchanges, util};

#[derive(Debug, Deserialize)]
pub(super) struct InstrumentQuery {
    exchange: Option<Exchange>,
    market_kind: Option<MarketKind>,
    active_only: Option<bool>,
    search: Option<String>,
    ids: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(super) struct InstrumentView {
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
pub(super) struct InstrumentPageView {
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
            contract_size: util::decimal_option(stored.instrument.contract_size),
            price_tick: stored.instrument.price_tick.to_string(),
            quantity_step: stored.instrument.quantity_step.to_string(),
            minimum_quantity: util::decimal_option(stored.instrument.minimum_quantity),
            minimum_notional: util::decimal_option(stored.instrument.minimum_notional),
            maker_fee_rate: util::decimal_option(stored.instrument.maker_fee_rate),
            taker_fee_rate: util::decimal_option(stored.instrument.taker_fee_rate),
            active: stored.instrument.active,
            observed_at: stored.observed_at,
        }
    }
}

pub(super) async fn list_instruments(
    State(state): State<AppState>,
    Query(query): Query<InstrumentQuery>,
) -> ApiResult<Json<InstrumentPageView>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 250);
    let offset = query.offset.unwrap_or(0).max(0);
    let ids = query
        .ids
        .as_deref()
        .map(util::parse_uuid_list)
        .transpose()?;
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
pub(super) struct SyncInstrumentsRequest {
    exchanges: Option<Vec<Exchange>>,
    reload: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(super) struct SyncInstrumentsResponse {
    synchronized: BTreeMap<&'static str, usize>,
    failed: BTreeMap<&'static str, &'static str>,
}

pub(super) async fn sync_instruments(
    State(state): State<AppState>,
    Json(request): Json<SyncInstrumentsRequest>,
) -> ApiResult<Json<SyncInstrumentsResponse>> {
    let requested = request
        .exchanges
        .unwrap_or_else(|| util::exchanges().to_vec());
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
                synchronized.insert(util::exchange_text(exchange), count);
            }
            Err(error) => {
                tracing::warn!(exchange = util::exchange_text(exchange), %error, "instrument synchronization failed");
                failed.insert(util::exchange_text(exchange), "gateway_unavailable");
            }
        }
    }
    let Json(_) = exchanges::list_exchanges(State(state)).await?;
    Ok(Json(SyncInstrumentsResponse {
        synchronized,
        failed,
    }))
}
