use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{DomainError, QuantityUnit};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractKind {
    Linear,
    Inverse,
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
    pub exchange_symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub settle_asset: Option<String>,
    pub contract_kind: Option<ContractKind>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub contract_size: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub price_tick: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity_step: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub minimum_quantity: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub minimum_notional: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub maker_fee_rate: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub taker_fee_rate: Option<Decimal>,
    pub active: bool,
}

impl Instrument {
    pub fn target_to_native_quantity(
        &self,
        amount: Decimal,
        unit: QuantityUnit,
        reference_price: Decimal,
    ) -> Result<Decimal, DomainError> {
        if reference_price <= Decimal::ZERO {
            return Err(DomainError::NonPositivePrice);
        }
        let quantity = match (self.id.market_kind, self.contract_kind, unit) {
            (MarketKind::Spot, None, QuantityUnit::BaseQuantity) => amount,
            (MarketKind::Spot, None, QuantityUnit::QuoteNotional) => amount / reference_price,
            (MarketKind::Spot, None, QuantityUnit::Contracts) => {
                return Err(DomainError::UnsupportedQuantityUnit);
            }
            (MarketKind::Perpetual, Some(ContractKind::Linear), QuantityUnit::Contracts)
            | (MarketKind::Perpetual, Some(ContractKind::Inverse), QuantityUnit::Contracts) => {
                amount
            }
            (MarketKind::Perpetual, Some(ContractKind::Linear), QuantityUnit::BaseQuantity) => {
                amount / self.required_contract_size()?
            }
            (MarketKind::Perpetual, Some(ContractKind::Linear), QuantityUnit::QuoteNotional) => {
                amount / (self.required_contract_size()? * reference_price)
            }
            (MarketKind::Perpetual, Some(ContractKind::Inverse), QuantityUnit::BaseQuantity) => {
                amount * reference_price / self.required_contract_size()?
            }
            (MarketKind::Perpetual, Some(ContractKind::Inverse), QuantityUnit::QuoteNotional) => {
                amount / self.required_contract_size()?
            }
            _ => return Err(DomainError::IncompleteInstrument("contract kind")),
        };
        Ok(self.round_native_quantity_down(quantity))
    }

    pub fn native_to_base_quantity(
        &self,
        native_quantity: Decimal,
        price: Decimal,
    ) -> Result<Decimal, DomainError> {
        if price <= Decimal::ZERO {
            return Err(DomainError::NonPositivePrice);
        }
        match (self.id.market_kind, self.contract_kind) {
            (MarketKind::Spot, None) => Ok(native_quantity),
            (MarketKind::Perpetual, Some(ContractKind::Linear)) => {
                Ok(native_quantity * self.required_contract_size()?)
            }
            (MarketKind::Perpetual, Some(ContractKind::Inverse)) => {
                Ok(native_quantity * self.required_contract_size()? / price)
            }
            _ => Err(DomainError::IncompleteInstrument("contract kind")),
        }
    }

    pub fn native_to_quote_quantity(
        &self,
        native_quantity: Decimal,
        price: Decimal,
    ) -> Result<Decimal, DomainError> {
        if price <= Decimal::ZERO {
            return Err(DomainError::NonPositivePrice);
        }
        match (self.id.market_kind, self.contract_kind) {
            (MarketKind::Spot, None) => Ok(native_quantity * price),
            (MarketKind::Perpetual, Some(ContractKind::Linear)) => {
                Ok(native_quantity * self.required_contract_size()? * price)
            }
            (MarketKind::Perpetual, Some(ContractKind::Inverse)) => {
                Ok(native_quantity * self.required_contract_size()?)
            }
            _ => Err(DomainError::IncompleteInstrument("contract kind")),
        }
    }

    #[must_use]
    pub fn round_native_quantity_down(&self, quantity: Decimal) -> Decimal {
        if quantity <= Decimal::ZERO || self.quantity_step <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        (quantity / self.quantity_step).floor() * self.quantity_step
    }

    fn required_contract_size(&self) -> Result<Decimal, DomainError> {
        self.contract_size
            .filter(|size| *size > Decimal::ZERO)
            .ok_or(DomainError::IncompleteInstrument("contract size"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perpetual(contract_kind: ContractKind, contract_size: Decimal) -> Instrument {
        Instrument {
            id: InstrumentId::new(Exchange::Binance, MarketKind::Perpetual, "BTC/USDT:USDT")
                .unwrap(),
            exchange_symbol: "BTCUSDT".to_owned(),
            base_asset: "BTC".to_owned(),
            quote_asset: "USDT".to_owned(),
            settle_asset: Some("USDT".to_owned()),
            contract_kind: Some(contract_kind),
            contract_size: Some(contract_size),
            price_tick: Decimal::new(1, 1),
            quantity_step: Decimal::ONE,
            minimum_quantity: Some(Decimal::ONE),
            minimum_notional: None,
            maker_fee_rate: None,
            taker_fee_rate: None,
            active: true,
        }
    }

    #[test]
    fn linear_contract_converts_base_and_quote_targets() {
        let instrument = perpetual(ContractKind::Linear, Decimal::new(1, 3));
        let price = Decimal::new(20_000, 0);
        assert_eq!(
            instrument
                .target_to_native_quantity(Decimal::ONE, QuantityUnit::BaseQuantity, price)
                .unwrap(),
            Decimal::new(1_000, 0)
        );
        assert_eq!(
            instrument
                .target_to_native_quantity(
                    Decimal::new(2_000, 0),
                    QuantityUnit::QuoteNotional,
                    price,
                )
                .unwrap(),
            Decimal::new(100, 0)
        );
    }

    #[test]
    fn inverse_contract_converts_quote_target_to_contracts() {
        let instrument = perpetual(ContractKind::Inverse, Decimal::new(100, 0));
        let price = Decimal::new(20_000, 0);
        let native = instrument
            .target_to_native_quantity(Decimal::new(1_000, 0), QuantityUnit::QuoteNotional, price)
            .unwrap();
        assert_eq!(native, Decimal::new(10, 0));
        assert_eq!(
            instrument.native_to_base_quantity(native, price).unwrap(),
            Decimal::new(5, 2)
        );
    }

    #[test]
    fn quantity_rounding_retains_non_executable_residual() {
        let instrument = perpetual(ContractKind::Linear, Decimal::new(1, 3));
        assert_eq!(
            instrument.round_native_quantity_down(Decimal::new(199, 2)),
            Decimal::ONE
        );
    }
}
