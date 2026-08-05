use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exchange {
    Binance,
    Okx,
    Bybit,
    GateIo,
    Bitget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketKind {
    Spot,
    Perpetual,
    Future,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstrumentId {
    pub exchange: Exchange,
    pub market_kind: MarketKind,
    pub symbol: String,
}

impl InstrumentId {
    pub fn new(
        exchange: Exchange,
        market_kind: MarketKind,
        symbol: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let symbol = symbol.into();
        if symbol.trim().is_empty() {
            return Err(DomainError::EmptyInstrumentField("symbol"));
        }

        Ok(Self {
            exchange,
            market_kind,
            symbol,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instrument {
    pub id: InstrumentId,
    pub base_asset: String,
    pub quote_asset: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub price_tick: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity_step: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub minimum_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub minimum_notional: Decimal,
}
