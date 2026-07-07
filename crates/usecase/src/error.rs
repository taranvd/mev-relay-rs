use relay_datastore::AuctioneerError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UseCaseError {
    #[error("Auctioneer error: {0}")]
    AuctioneerError(#[from] AuctioneerError),

    #[error("bid value is zero")]
    ZeroBid,

    #[error("builder is not whitelisted")]
    UnauthorizedBuilder,

    #[error("unauthorized validator")]
    UnauthorizedGetHeader,

    #[error("bid slot is in the past")]
    InvalidSlot,

    #[error("no bid found for slot")]
    NoBidFound,

    #[error("no proposer duty found for slot")]
    DutyNotFound,

    #[error("invalid builder signature")]
    InvalidBuilderSignature,

    #[error("invalid validator signature")]
    InvalidValidatorSignature,

    #[error("invalid payload attributes: {0}")]
    InvalidPayloadAttributes(String),
}
