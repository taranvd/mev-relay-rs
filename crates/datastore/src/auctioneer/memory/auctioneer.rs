use crate::auctioneer::{Auctioneer, AuctioneerError};
use async_trait::async_trait;
use moka::future::Cache;
use relay_entity::BidSubmission;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct MemoryAuctioneer {
    cache: Cache<u64, Arc<BidSubmission>>,
}

impl MemoryAuctioneer {
    pub fn new(ttl: Duration) -> Self {
        let cache = Cache::builder().time_to_live(ttl).time_to_idle(ttl).build();
        Self { cache }
    }
}

#[async_trait]
impl Auctioneer for MemoryAuctioneer {
    async fn compare_and_bid(
        &self,
        slot: u64,
        bid: Arc<BidSubmission>,
    ) -> Result<(), AuctioneerError> {
        let entry = self
            .cache
            .entry(slot)
            .or_insert_with_if(async { bid.clone() }, |existing| {
                bid.message.value > existing.message.value
            })
            .await;

        if !Arc::ptr_eq(entry.value(), &bid) {
            return Err(AuctioneerError::BidValueNotHighEnough {
                provided: bid.message.value.to_string(),
                current: entry.value().message.value.to_string(),
            });
        }

        Ok(())
    }

    async fn get_best_bid(&self, slot: u64) -> Result<Arc<BidSubmission>, AuctioneerError> {
        self.cache
            .get(&slot)
            .await
            .ok_or(AuctioneerError::BidNotFound(slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_crypto::BlsSignature;
    use relay_entity::{Address, B256, BidTrace, BlobsBundle, ExecutionPayload, U256};

    fn dummy_sig() -> BlsSignature {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let sig = sk.sign(b"dummy", &[], b"");
        BlsSignature::deserialize(&sig.to_bytes()).unwrap()
    }

    fn dummy_bid(slot: u64, value: u128) -> Arc<BidSubmission> {
        let payload = Arc::new(ExecutionPayload {
            parent_hash: B256(alloy_primitives::B256::default()),
            fee_recipient: Address(alloy_primitives::Address::default()),
            state_root: B256(alloy_primitives::B256::default()),
            receipts_root: B256(alloy_primitives::B256::default()),
            logs_bloom: ssz_types::FixedVector::from(vec![0u8; 256]),
            prev_randao: B256(alloy_primitives::B256::default()),
            block_number: 0,
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            timestamp: 0,
            extra_data: ssz_types::VariableList::from(vec![]),
            base_fee_per_gas: U256(alloy_primitives::U256::ZERO),
            block_hash: B256(alloy_primitives::B256::default()),
            transactions: ssz_types::VariableList::from(vec![]),
            withdrawals: ssz_types::VariableList::from(vec![]),
            blob_gas_used: 0,
            excess_blob_gas: 0,
        });

        Arc::new(BidSubmission::new(
            BidTrace {
                slot,
                parent_hash: B256(alloy_primitives::B256::default()),
                block_hash: B256(alloy_primitives::B256::default()),
                builder_pubkey: relay_crypto::BlsPublicKey::default(),
                proposer_pubkey: relay_crypto::BlsPublicKey::default(),
                proposer_fee_recipient: Address(alloy_primitives::Address::default()),
                gas_limit: 30_000_000,
                gas_used: 15_000_000,
                value: U256(alloy_primitives::U256::from(value)),
            },
            payload,
            Arc::new(BlobsBundle::default()),
            dummy_sig(),
        ))
    }

    #[tokio::test]
    async fn test_first_bid_wins() {
        let auctioneer = MemoryAuctioneer::new(Duration::from_secs(60));
        let bid = dummy_bid(1, 100);
        assert!(auctioneer.compare_and_bid(1, bid).await.is_ok());
    }

    #[tokio::test]
    async fn test_higher_bid_replaces_lower() {
        let auctioneer = MemoryAuctioneer::new(Duration::from_secs(60));
        let low = dummy_bid(1, 50);
        let high = dummy_bid(1, 100);
        assert!(auctioneer.compare_and_bid(1, low).await.is_ok());
        assert!(auctioneer.compare_and_bid(1, high).await.is_ok());
        let best = auctioneer.get_best_bid(1).await.unwrap();
        assert_eq!(best.message.value, U256(alloy_primitives::U256::from(100)));
    }

    #[tokio::test]
    async fn test_lower_bid_rejected() {
        let auctioneer = MemoryAuctioneer::new(Duration::from_secs(60));
        let high = dummy_bid(1, 100);
        let low = dummy_bid(1, 50);
        assert!(auctioneer.compare_and_bid(1, high).await.is_ok());
        let err = auctioneer.compare_and_bid(1, low).await.unwrap_err();
        assert_eq!(
            err,
            AuctioneerError::BidValueNotHighEnough {
                provided: "50".into(),
                current: "100".into(),
            }
        );
    }

    #[tokio::test]
    async fn test_get_best_bid_not_found() {
        let auctioneer = MemoryAuctioneer::new(Duration::from_secs(60));
        let err = auctioneer.get_best_bid(99).await.unwrap_err();
        assert_eq!(err, AuctioneerError::BidNotFound(99));
    }

    #[tokio::test]
    async fn test_different_slots_independent() {
        let auctioneer = MemoryAuctioneer::new(Duration::from_secs(60));
        let bid1 = dummy_bid(1, 100);
        let bid2 = dummy_bid(2, 200);
        assert!(auctioneer.compare_and_bid(1, bid1).await.is_ok());
        assert!(auctioneer.compare_and_bid(2, bid2).await.is_ok());
        let best1 = auctioneer.get_best_bid(1).await.unwrap();
        let best2 = auctioneer.get_best_bid(2).await.unwrap();
        assert_eq!(best1.message.value, U256(alloy_primitives::U256::from(100)));
        assert_eq!(best2.message.value, U256(alloy_primitives::U256::from(200)));
    }
}
