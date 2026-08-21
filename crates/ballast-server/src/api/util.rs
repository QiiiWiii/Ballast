use std::str::FromStr;

use ballast_core::{Exchange, QuantityUnit, Side, StrategyKind};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

use super::{ApiError, ApiResult};

pub(super) fn positive_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::validation("invalid_decimal", json!({ "field": field })))?;
    if parsed <= Decimal::ZERO {
        return Err(ApiError::validation(
            "decimal_must_be_positive",
            json!({ "field": field }),
        ));
    }
    Ok(parsed)
}

pub(super) fn non_negative_decimal(value: &str, field: &'static str) -> ApiResult<Decimal> {
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::validation("invalid_decimal", json!({ "field": field })))?;
    if parsed < Decimal::ZERO {
        return Err(ApiError::validation(
            "decimal_must_be_non_negative",
            json!({ "field": field }),
        ));
    }
    Ok(parsed)
}

pub(super) fn decimal_option(value: Option<Decimal>) -> Option<String> {
    value.map(|value| value.to_string())
}

pub(super) fn parse_uuid_list(value: &str) -> ApiResult<Vec<Uuid>> {
    value
        .split(',')
        .filter(|item| !item.is_empty())
        .map(|item| {
            Uuid::parse_str(item).map_err(|_| {
                ApiError::validation("invalid_instrument_id", json!({ "instrument_id": item }))
            })
        })
        .collect()
}

pub(super) fn parse_exchange(value: &str) -> ApiResult<Exchange> {
    match value {
        "binance" => Ok(Exchange::Binance),
        "okx" => Ok(Exchange::Okx),
        "bybit" => Ok(Exchange::Bybit),
        "gate_io" => Ok(Exchange::GateIo),
        "bitget" => Ok(Exchange::Bitget),
        _ => Err(ApiError::not_found("exchange_not_found")),
    }
}

pub(super) fn parse_strategy_kind(value: &str) -> ApiResult<StrategyKind> {
    match value {
        "twap" => Ok(StrategyKind::Twap),
        "pov" => Ok(StrategyKind::Pov),
        _ => Err(ApiError::validation("strategy_invalid", json!({}))),
    }
}

pub(super) fn parse_quantity_unit(value: &str) -> ApiResult<QuantityUnit> {
    match value {
        "base_quantity" => Ok(QuantityUnit::BaseQuantity),
        "quote_notional" => Ok(QuantityUnit::QuoteNotional),
        "contracts" => Ok(QuantityUnit::Contracts),
        _ => Err(ApiError::validation("quantity_unit_invalid", json!({}))),
    }
}

pub(super) const fn exchanges() -> &'static [Exchange; 5] {
    &[
        Exchange::Binance,
        Exchange::Okx,
        Exchange::Bybit,
        Exchange::GateIo,
        Exchange::Bitget,
    ]
}

pub(super) const fn exchange_text(value: Exchange) -> &'static str {
    match value {
        Exchange::Binance => "binance",
        Exchange::Okx => "okx",
        Exchange::Bybit => "bybit",
        Exchange::GateIo => "gate_io",
        Exchange::Bitget => "bitget",
    }
}

pub(super) const fn exchange_proto_number(value: Exchange) -> i32 {
    match value {
        Exchange::Binance => 1,
        Exchange::Okx => 2,
        Exchange::Bybit => 3,
        Exchange::GateIo => 4,
        Exchange::Bitget => 5,
    }
}

pub(super) const fn side_text(value: Side) -> &'static str {
    match value {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

pub(super) const fn strategy_text(value: StrategyKind) -> &'static str {
    match value {
        StrategyKind::Twap => "twap",
        StrategyKind::Pov => "pov",
    }
}

pub(super) const fn quantity_unit_text(value: QuantityUnit) -> &'static str {
    match value {
        QuantityUnit::BaseQuantity => "base_quantity",
        QuantityUnit::QuoteNotional => "quote_notional",
        QuantityUnit::Contracts => "contracts",
    }
}
