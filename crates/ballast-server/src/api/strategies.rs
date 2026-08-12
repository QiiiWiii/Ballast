use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use ballast_core::{Exchange, MarketKind, QuantityUnit, StrategyKind};
use ballast_storage::{
    NewStrategyTemplate, NewStrategyTemplateVersion, StoredStrategyTemplate,
    StoredStrategyTemplateVersion,
};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;

use super::{ApiError, ApiResult, util};

#[derive(Debug, Deserialize)]
pub(super) struct StrategyTemplateInput {
    name: Option<String>,
    description: Option<String>,
    strategy: StrategyKind,
    quantity_unit: QuantityUnit,
    duration_seconds: u64,
    slice_interval_ms: u64,
    max_slippage_bps: u32,
    participation_rate: Option<String>,
    max_slice_amount: Option<String>,
    change_note: Option<String>,
    execution_backend: ExecutionBackendInput,
    venue_exchange: Option<Exchange>,
    venue_market_kind: Option<MarketKind>,
    native_configuration: Option<NativeAlgorithmInput>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ExecutionBackendInput {
    ManagedIoc,
    VenueNativeAlgo,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum NativeAlgorithmInput {
    BinanceSpotTwap {
        limit_price: String,
    },
    BinanceUsdmTwap {
        limit_price: String,
        position_side: String,
        reduce_only: bool,
    },
    BinanceUsdmVp {
        limit_price: String,
        urgency: String,
        position_side: String,
        reduce_only: bool,
    },
    OkxTwap {
        trade_mode: String,
        position_side: String,
        size_limit: String,
        price_limit: String,
        time_interval_seconds: u64,
        price_variance_ratio: Option<String>,
        price_spread: Option<String>,
        reduce_only: bool,
    },
}

#[derive(Debug, Serialize)]
pub(super) struct StrategyTemplateView {
    id: Uuid,
    name: String,
    description: String,
    status: String,
    current_version: i32,
    usage_count: i64,
    versions: Vec<StrategyTemplateVersionView>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub(super) struct StrategyTemplateVersionView {
    id: Uuid,
    template_id: Uuid,
    version: i32,
    strategy: String,
    quantity_unit: String,
    duration_seconds: i64,
    slice_interval_ms: i64,
    max_slippage_bps: i32,
    participation_rate: Option<String>,
    max_slice_amount: Option<String>,
    execution_backend: String,
    venue_exchange: Option<String>,
    venue_market_kind: Option<String>,
    native_algorithm: Option<String>,
    native_configuration: Option<NativeAlgorithmInput>,
    change_note: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<StoredStrategyTemplateVersion> for StrategyTemplateVersionView {
    type Error = ApiError;

    fn try_from(value: StoredStrategyTemplateVersion) -> Result<Self, Self::Error> {
        let native_configuration = value
            .native_params
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| {
                tracing::error!(%error, version_id = %value.id, "stored native configuration is invalid");
                ApiError::internal("stored_native_configuration_invalid")
            })?;
        Ok(Self {
            id: value.id,
            template_id: value.template_id,
            version: value.version,
            strategy: value.strategy_kind,
            quantity_unit: value.quantity_unit,
            duration_seconds: value.duration_seconds,
            slice_interval_ms: value.slice_interval_ms,
            max_slippage_bps: value.max_slippage_bps,
            participation_rate: util::decimal_option(value.participation_rate),
            max_slice_amount: util::decimal_option(value.max_slice_amount),
            execution_backend: value.execution_backend,
            venue_exchange: value.venue_exchange,
            venue_market_kind: value.venue_market_kind,
            native_algorithm: value.native_algorithm,
            native_configuration,
            change_note: value.change_note,
            created_at: value.created_at,
        })
    }
}

async fn template_view(
    state: &AppState,
    template: StoredStrategyTemplate,
) -> ApiResult<StrategyTemplateView> {
    let versions = ballast_storage::list_strategy_template_versions(&state.database, template.id)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(StrategyTemplateVersionView::try_from)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(StrategyTemplateView {
        id: template.id,
        name: template.name,
        description: template.description,
        status: template.status,
        current_version: template.current_version,
        usage_count: template.usage_count,
        versions,
        created_at: template.created_at,
        updated_at: template.updated_at,
    })
}

pub(super) async fn list_strategy_templates(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<StrategyTemplateView>>> {
    let templates = ballast_storage::list_strategy_templates(&state.database)
        .await
        .map_err(ApiError::database)?;
    let views = join_all(
        templates
            .into_iter()
            .map(|template| template_view(&state, template)),
    )
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(views))
}

pub(super) async fn get_strategy_template(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> ApiResult<Json<StrategyTemplateView>> {
    let template = ballast_storage::get_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
    Ok(Json(template_view(&state, template).await?))
}

pub(super) async fn create_strategy_template(
    State(state): State<AppState>,
    Json(input): Json<StrategyTemplateInput>,
) -> ApiResult<(StatusCode, Json<StrategyTemplateView>)> {
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() <= 120)
        .ok_or_else(|| ApiError::validation("strategy_template_name_required", json!({})))?;
    let version = validated_template_version(&input)?;
    let (template, _) = ballast_storage::create_strategy_template(
        &state.database,
        NewStrategyTemplate {
            name: name.to_owned(),
            description: input.description.unwrap_or_default().trim().to_owned(),
            strategy_kind: version.strategy_kind,
            quantity_unit: version.quantity_unit,
            duration_seconds: version.duration_seconds,
            slice_interval_ms: version.slice_interval_ms,
            max_slippage_bps: version.max_slippage_bps,
            participation_rate: version.participation_rate,
            max_slice_amount: version.max_slice_amount,
            change_note: version.change_note,
            execution_backend: version.execution_backend,
            venue_exchange: version.venue_exchange,
            venue_market_kind: version.venue_market_kind,
            native_algorithm: version.native_algorithm,
            native_params: version.native_params,
        },
    )
    .await
    .map_err(ApiError::template_database)?;
    Ok((
        StatusCode::CREATED,
        Json(template_view(&state, template).await?),
    ))
}

pub(super) async fn create_strategy_template_version(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
    Json(input): Json<StrategyTemplateInput>,
) -> ApiResult<(StatusCode, Json<StrategyTemplateVersionView>)> {
    if ballast_storage::get_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
        .is_none()
    {
        return Err(ApiError::not_found("strategy_template_not_found"));
    }
    let version = ballast_storage::create_strategy_template_version(
        &state.database,
        template_id,
        validated_template_version(&input)?,
    )
    .await
    .map_err(ApiError::template_database)?;
    Ok((
        StatusCode::CREATED,
        Json(StrategyTemplateVersionView::try_from(version)?),
    ))
}

pub(super) async fn archive_strategy_template(
    State(state): State<AppState>,
    Path(template_id): Path<Uuid>,
) -> ApiResult<Json<StrategyTemplateView>> {
    if !ballast_storage::archive_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
    {
        let existing = ballast_storage::get_strategy_template(&state.database, template_id)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
        return Ok(Json(template_view(&state, existing).await?));
    }
    let template = ballast_storage::get_strategy_template(&state.database, template_id)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("strategy_template_not_found"))?;
    Ok(Json(template_view(&state, template).await?))
}

fn validated_template_version(
    input: &StrategyTemplateInput,
) -> ApiResult<NewStrategyTemplateVersion> {
    if input.duration_seconds == 0 || input.duration_seconds > 86_400 {
        return Err(ApiError::validation(
            "duration_out_of_range",
            json!({ "maximum": 86400 }),
        ));
    }
    if input.slice_interval_ms == 0
        || input.slice_interval_ms > input.duration_seconds.saturating_mul(1_000)
    {
        return Err(ApiError::validation(
            "slice_interval_out_of_range",
            json!({}),
        ));
    }
    if input.max_slippage_bps > 10_000 {
        return Err(ApiError::validation(
            "max_slippage_out_of_range",
            json!({ "maximum": 10000 }),
        ));
    }
    let participation_rate = input
        .participation_rate
        .as_deref()
        .map(|value| util::positive_decimal(value, "participation_rate"))
        .transpose()?;
    match (input.execution_backend, input.strategy) {
        (ExecutionBackendInput::ManagedIoc, StrategyKind::Twap) if participation_rate.is_some() => {
            return Err(ApiError::validation(
                "participation_rate_not_allowed",
                json!({}),
            ));
        }
        (ExecutionBackendInput::ManagedIoc, StrategyKind::Pov) if participation_rate.is_none() => {
            return Err(ApiError::validation(
                "participation_rate_required",
                json!({}),
            ));
        }
        (ExecutionBackendInput::ManagedIoc, StrategyKind::Pov)
            if participation_rate.is_some_and(|value| value > Decimal::ONE) =>
        {
            return Err(ApiError::validation(
                "participation_rate_out_of_range",
                json!({ "maximum": "1" }),
            ));
        }
        (ExecutionBackendInput::VenueNativeAlgo, _) if participation_rate.is_some() => {
            return Err(ApiError::validation(
                "native_participation_rate_not_allowed",
                json!({}),
            ));
        }
        _ => {}
    }
    let max_slice_amount = input
        .max_slice_amount
        .as_deref()
        .map(|value| util::positive_decimal(value, "max_slice_amount"))
        .transpose()?;
    if input.execution_backend == ExecutionBackendInput::VenueNativeAlgo
        && max_slice_amount.is_some()
    {
        return Err(ApiError::validation(
            "native_max_slice_not_allowed",
            json!({}),
        ));
    }
    let (execution_backend, venue_exchange, venue_market_kind, native_algorithm, native_params) =
        validate_execution_backend(input)?;
    Ok(NewStrategyTemplateVersion {
        strategy_kind: util::strategy_text(input.strategy).to_owned(),
        quantity_unit: util::quantity_unit_text(input.quantity_unit).to_owned(),
        duration_seconds: input.duration_seconds as i64,
        slice_interval_ms: input.slice_interval_ms as i64,
        max_slippage_bps: input.max_slippage_bps as i32,
        participation_rate,
        max_slice_amount,
        execution_backend,
        venue_exchange,
        venue_market_kind,
        native_algorithm,
        native_params,
        change_note: input
            .change_note
            .clone()
            .unwrap_or_default()
            .trim()
            .to_owned(),
    })
}

fn validate_execution_backend(
    input: &StrategyTemplateInput,
) -> ApiResult<(
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
)> {
    if input.execution_backend == ExecutionBackendInput::ManagedIoc {
        if input.venue_exchange.is_some()
            || input.venue_market_kind.is_some()
            || input.native_configuration.is_some()
        {
            return Err(ApiError::validation(
                "managed_backend_shape_invalid",
                json!({}),
            ));
        }
        return Ok(("managed_ioc".to_owned(), None, None, None, None));
    }
    let exchange = input
        .venue_exchange
        .ok_or_else(|| ApiError::validation("native_exchange_required", json!({})))?;
    let market = input
        .venue_market_kind
        .ok_or_else(|| ApiError::validation("native_market_kind_required", json!({})))?;
    let config = input
        .native_configuration
        .as_ref()
        .ok_or_else(|| ApiError::validation("native_configuration_required", json!({})))?;
    let algorithm = match config {
        NativeAlgorithmInput::BinanceSpotTwap { limit_price } => {
            require_native_shape(exchange, market, Exchange::Binance, MarketKind::Spot)?;
            util::positive_decimal(limit_price, "limit_price")?;
            "binance_spot_twap"
        }
        NativeAlgorithmInput::BinanceUsdmTwap { limit_price, .. } => {
            require_native_shape(exchange, market, Exchange::Binance, MarketKind::Perpetual)?;
            util::positive_decimal(limit_price, "limit_price")?;
            "binance_usdm_twap"
        }
        NativeAlgorithmInput::BinanceUsdmVp {
            limit_price,
            urgency,
            ..
        } => {
            require_native_shape(exchange, market, Exchange::Binance, MarketKind::Perpetual)?;
            util::positive_decimal(limit_price, "limit_price")?;
            if !matches!(urgency.as_str(), "LOW" | "MEDIUM" | "HIGH") {
                return Err(ApiError::validation("native_urgency_invalid", json!({})));
            }
            if input.strategy != StrategyKind::Pov {
                return Err(ApiError::validation("native_strategy_mismatch", json!({})));
            }
            "binance_usdm_vp"
        }
        NativeAlgorithmInput::OkxTwap {
            size_limit,
            price_limit,
            time_interval_seconds,
            price_variance_ratio,
            price_spread,
            ..
        } => {
            if exchange != Exchange::Okx {
                return Err(ApiError::validation("native_exchange_mismatch", json!({})));
            }
            util::positive_decimal(size_limit, "size_limit")?;
            util::positive_decimal(price_limit, "price_limit")?;
            if *time_interval_seconds == 0 {
                return Err(ApiError::validation("native_interval_invalid", json!({})));
            }
            match (price_variance_ratio, price_spread) {
                (Some(value), None) => {
                    util::positive_decimal(value, "price_variance_ratio")?;
                }
                (None, Some(value)) => {
                    util::positive_decimal(value, "price_spread")?;
                }
                _ => {
                    return Err(ApiError::validation(
                        "okx_price_variance_required",
                        json!({}),
                    ));
                }
            }
            "okx_twap"
        }
    };
    Ok((
        "venue_native_algo".to_owned(),
        Some(util::exchange_text(exchange).to_owned()),
        Some(
            match market {
                MarketKind::Spot => "spot",
                MarketKind::Perpetual => "perpetual",
            }
            .to_owned(),
        ),
        Some(algorithm.to_owned()),
        Some(
            serde_json::to_value(config)
                .map_err(|_| ApiError::validation("native_configuration_invalid", json!({})))?,
        ),
    ))
}

fn require_native_shape(
    exchange: Exchange,
    market: MarketKind,
    expected_exchange: Exchange,
    expected_market: MarketKind,
) -> ApiResult<()> {
    if exchange != expected_exchange || market != expected_market {
        return Err(ApiError::validation(
            "native_backend_shape_mismatch",
            json!({}),
        ));
    }
    Ok(())
}
