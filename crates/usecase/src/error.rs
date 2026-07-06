use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubmitBidError {
    #[error("bid value below floor: {0}")]
    BelowFloor(String),

    #[error("bid value is zero")]
    ZeroBid,

    #[error("builder is not whitelisted")]
    UnauthorizedBuilder,

    #[error("bid slot is int the past")]
    PastSlot,

    #[error("no proposer duty found for slot")]
    DutyNotFound,

    #[error("invalid builder signature")]
    InvalidBuilderSignature,

    #[error("invalid payload attributes: {0}")]
    InvalidPayloadAttributes(String),
}
