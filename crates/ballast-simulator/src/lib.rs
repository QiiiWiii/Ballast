#![forbid(unsafe_code)]

use ballast_core::{ContractKind, DomainError, Instrument, Side};
use rust_decimal::Decimal;

const BPS_DENOMINATOR: Decimal = Decimal::from_parts(10_000, 0, 0, false, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillEstimate {
    pub filled_native_quantity: Decimal,
    pub unfilled_native_quantity: Decimal,
    pub filled_base_quantity: Decimal,
    pub filled_quote_quantity: Decimal,
    pub average_price: Option<Decimal>,
    pub worst_price: Option<Decimal>,
    pub slippage_bps: Option<Decimal>,
    pub fee_amount: Option<Decimal>,
    pub fee_asset: Option<String>,
}

pub fn estimate_protected_ioc_fill(
    instrument: &Instrument,
    side: Side,
    native_quantity: Decimal,
    max_slippage_bps: u32,
    bids: &[BookLevel],
    asks: &[BookLevel],
) -> Result<FillEstimate, DomainError> {
    let levels = match side {
        Side::Buy => asks,
        Side::Sell => bids,
    };
    let reference_price = levels
        .first()
        .map(|level| level.price)
        .filter(|price| *price > Decimal::ZERO)
        .ok_or(DomainError::NonPositivePrice)?;
    let slippage_ratio = Decimal::from(max_slippage_bps) / BPS_DENOMINATOR;
    let protected_price = match side {
        Side::Buy => reference_price * (Decimal::ONE + slippage_ratio),
        Side::Sell => reference_price * (Decimal::ONE - slippage_ratio),
    };
    let mut remaining = native_quantity.max(Decimal::ZERO);
    let mut native_quote_cost = Decimal::ZERO;
    let mut worst_price = None;

    for level in levels {
        if remaining.is_zero() || !within_price_protection(side, level.price, protected_price) {
            break;
        }
        let fill_quantity = remaining.min(level.quantity.max(Decimal::ZERO));
        if fill_quantity.is_zero() {
            continue;
        }
        native_quote_cost += fill_quantity * level.price;
        remaining -= fill_quantity;
        worst_price = Some(level.price);
    }

    let filled_native_quantity = native_quantity.max(Decimal::ZERO) - remaining;
    let average_price = if filled_native_quantity.is_zero() {
        None
    } else {
        Some(native_quote_cost / filled_native_quantity)
    };
    let (filled_base_quantity, filled_quote_quantity) = match average_price {
        Some(price) => (
            instrument.native_to_base_quantity(filled_native_quantity, price)?,
            instrument.native_to_quote_quantity(filled_native_quantity, price)?,
        ),
        None => (Decimal::ZERO, Decimal::ZERO),
    };
    let slippage_bps = average_price.map(|price| match side {
        Side::Buy => (price / reference_price - Decimal::ONE) * BPS_DENOMINATOR,
        Side::Sell => (Decimal::ONE - price / reference_price) * BPS_DENOMINATOR,
    });
    let (fee_amount, fee_asset) =
        estimate_fee(instrument, filled_base_quantity, filled_quote_quantity);

    Ok(FillEstimate {
        filled_native_quantity,
        unfilled_native_quantity: remaining,
        filled_base_quantity,
        filled_quote_quantity,
        average_price,
        worst_price,
        slippage_bps,
        fee_amount,
        fee_asset,
    })
}

fn within_price_protection(side: Side, price: Decimal, protected_price: Decimal) -> bool {
    match side {
        Side::Buy => price <= protected_price,
        Side::Sell => price >= protected_price,
    }
}

fn estimate_fee(
    instrument: &Instrument,
    base_quantity: Decimal,
    quote_quantity: Decimal,
) -> (Option<Decimal>, Option<String>) {
    let Some(rate) = instrument.taker_fee_rate else {
        return (None, None);
    };
    if instrument.contract_kind == Some(ContractKind::Inverse) {
        (
            Some(base_quantity * rate),
            instrument
                .settle_asset
                .clone()
                .or_else(|| Some(instrument.base_asset.clone())),
        )
    } else {
        (
            Some(quote_quantity * rate),
            Some(instrument.quote_asset.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use ballast_core::{Exchange, InstrumentId, MarketKind};

    use super::*;

    fn spot_instrument() -> Instrument {
        Instrument {
            id: InstrumentId::new(Exchange::Binance, MarketKind::Spot, "BTC/USDT").unwrap(),
            exchange_symbol: "BTCUSDT".to_owned(),
            base_asset: "BTC".to_owned(),
            quote_asset: "USDT".to_owned(),
            settle_asset: None,
            contract_kind: None,
            contract_size: None,
            price_tick: Decimal::new(1, 1),
            quantity_step: Decimal::new(1, 3),
            minimum_quantity: None,
            minimum_notional: None,
            maker_fee_rate: None,
            taker_fee_rate: Some(Decimal::new(1, 3)),
            active: true,
        }
    }

    #[test]
    fn buy_stops_at_price_protection() {
        let asks = [
            BookLevel {
                price: Decimal::new(100, 0),
                quantity: Decimal::ONE,
            },
            BookLevel {
                price: Decimal::new(101, 0),
                quantity: Decimal::ONE,
            },
            BookLevel {
                price: Decimal::new(103, 0),
                quantity: Decimal::ONE,
            },
        ];
        let fill = estimate_protected_ioc_fill(
            &spot_instrument(),
            Side::Buy,
            Decimal::new(3, 0),
            200,
            &[],
            &asks,
        )
        .unwrap();

        assert_eq!(fill.filled_native_quantity, Decimal::new(2, 0));
        assert_eq!(fill.unfilled_native_quantity, Decimal::ONE);
        assert_eq!(fill.average_price, Some(Decimal::new(1005, 1)));
        assert_eq!(fill.fee_asset.as_deref(), Some("USDT"));
    }
}
