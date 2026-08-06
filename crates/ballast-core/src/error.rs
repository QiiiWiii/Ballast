use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("quantity must be greater than zero")]
    NonPositiveQuantity,
    #[error("instrument field `{0}` must not be empty")]
    EmptyInstrumentField(&'static str),
    #[error("price must be greater than zero")]
    NonPositivePrice,
    #[error("instrument metadata is incomplete: {0}")]
    IncompleteInstrument(&'static str),
    #[error("quantity unit is not supported for this market")]
    UnsupportedQuantityUnit,
}
