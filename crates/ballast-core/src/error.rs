use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("quantity must be greater than zero")]
    NonPositiveQuantity,
    #[error("instrument field `{0}` must not be empty")]
    EmptyInstrumentField(&'static str),
}
