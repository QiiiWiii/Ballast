#![forbid(unsafe_code)]

use ballast_core::Side;
use rust_decimal::Decimal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillEstimate {
    pub filled_quantity: Decimal,
    pub unfilled_quantity: Decimal,
    pub quote_quantity: Decimal,
}

pub fn estimate_market_fill(
    side: Side,
    quantity: Decimal,
    bids: &[BookLevel],
    asks: &[BookLevel],
) -> FillEstimate {
    let levels = match side {
        Side::Buy => asks,
        Side::Sell => bids,
    };
    let mut remaining = quantity.max(Decimal::ZERO);
    let mut quote_quantity = Decimal::ZERO;

    for level in levels {
        if remaining.is_zero() {
            break;
        }
        let fill_quantity = remaining.min(level.quantity.max(Decimal::ZERO));
        quote_quantity += fill_quantity * level.price;
        remaining -= fill_quantity;
    }

    FillEstimate {
        filled_quantity: quantity.max(Decimal::ZERO) - remaining,
        unfilled_quantity: remaining,
        quote_quantity,
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;

    #[test]
    fn buy_consumes_asks_in_order() {
        let asks = [
            BookLevel {
                price: Decimal::new(100, 0),
                quantity: Decimal::new(1, 0),
            },
            BookLevel {
                price: Decimal::new(101, 0),
                quantity: Decimal::new(2, 0),
            },
        ];

        let fill = estimate_market_fill(Side::Buy, Decimal::new(2, 0), &[], &asks);

        assert_eq!(fill.filled_quantity, Decimal::new(2, 0));
        assert_eq!(fill.quote_quantity, Decimal::new(201, 0));
        assert_eq!(fill.unfilled_quantity, Decimal::ZERO);
    }
}
