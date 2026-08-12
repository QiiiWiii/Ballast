use axum::{Json, extract::State};
use futures_util::future::join_all;
use serde::Serialize;
use serde_json::{Value, json};

use crate::AppState;

use super::{ApiError, ApiResult, util};

#[derive(Debug, Serialize)]
pub(super) struct NativeAlgorithmCapabilityView {
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

pub(super) async fn native_algorithm_capabilities(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<NativeAlgorithmCapabilityView>>> {
    let responses = join_all(
        util::exchanges()
            .iter()
            .copied()
            .map(|exchange| state.gateway.algorithmic_capabilities(exchange)),
    )
    .await;
    let mut result = Vec::new();
    for (exchange, response) in util::exchanges().iter().copied().zip(responses) {
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
                exchange: util::exchange_text(exchange),
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

pub(super) async fn live_readiness() -> Json<Value> {
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

pub(super) async fn private_plane_locked() -> ApiResult<Json<Value>> {
    Err(ApiError::locked("live_execution_disabled"))
}
