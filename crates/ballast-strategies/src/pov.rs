use rust_decimal::Decimal;

use crate::{Strategy, StrategyError, StrategyTickInput};

#[derive(Debug, Clone, Copy)]
pub struct Pov {
    participation_rate: Decimal,
}

impl Pov {
    pub fn new(participation_rate: Decimal) -> Result<Self, StrategyError> {
        if participation_rate <= Decimal::ZERO || participation_rate > Decimal::ONE {
            return Err(StrategyError::InvalidParticipationRate);
        }

        Ok(Self { participation_rate })
    }
}

impl Strategy for Pov {
    fn next_slice_quantity(&self, input: StrategyTickInput) -> Result<Decimal, StrategyError> {
        let desired = input.market_volume.max(Decimal::ZERO) * self.participation_rate;
        Ok(desired.min(input.remaining_quantity()))
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;

    #[test]
    fn slice_is_capped_by_remaining_quantity() {
        let strategy = Pov::new(Decimal::new(2, 1)).expect("valid participation rate");
        let input = StrategyTickInput {
            target_quantity: Decimal::new(10, 0),
            executed_quantity: Decimal::new(9, 0),
            market_volume: Decimal::new(20, 0),
            elapsed_ticks: 0,
            total_ticks: 1,
        };

        assert_eq!(strategy.next_slice_quantity(input), Ok(Decimal::new(1, 0)));
    }
}
