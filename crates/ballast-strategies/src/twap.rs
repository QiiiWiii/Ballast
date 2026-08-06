use rust_decimal::Decimal;

use crate::{Strategy, StrategyError, StrategyTickInput};

#[derive(Debug, Default, Clone, Copy)]
pub struct Twap;

impl Strategy for Twap {
    fn next_slice_quantity(&self, input: StrategyTickInput) -> Result<Decimal, StrategyError> {
        if input.total_ticks == 0 {
            return Err(StrategyError::ZeroTotalTicks);
        }

        let remaining = input.remaining_quantity();
        if remaining.is_zero() {
            return Ok(Decimal::ZERO);
        }

        let remaining_ticks = input.total_ticks.saturating_sub(input.elapsed_ticks).max(1);
        Ok(remaining / Decimal::from(remaining_ticks))
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;

    #[test]
    fn final_tick_consumes_exact_remaining_quantity() {
        let input = StrategyTickInput {
            target_amount: Decimal::new(10, 0),
            executed_amount: Decimal::new(7, 0),
            market_volume: Decimal::ZERO,
            elapsed_ticks: 9,
            total_ticks: 10,
        };

        assert_eq!(Twap.next_slice_quantity(input), Ok(Decimal::new(3, 0)));
    }
}
