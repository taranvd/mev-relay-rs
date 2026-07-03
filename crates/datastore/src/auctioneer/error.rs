use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum AuctioneerError {
    #[error("bid not found for slot {0}")]
    BidNotFound(u64),

    #[error("bid value not high enough: provided {provided}, current {current}")]
    BidValueNotHighEnough { provided: String, current: String },
}
