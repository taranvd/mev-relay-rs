use crate::auctioneer::AuctioneerError;
use async_trait::async_trait;
use relay_entity::BidSubmission;
use std::sync::Arc;

#[async_trait]
pub trait Auctioneer: Send + Sync {
    async fn compare_and_bid(
        &self,
        slot: u64,
        bid: Arc<BidSubmission>,
    ) -> Result<(), AuctioneerError>;
    async fn get_best_bid(&self, slot: u64) -> Result<Arc<BidSubmission>, AuctioneerError>;
}
