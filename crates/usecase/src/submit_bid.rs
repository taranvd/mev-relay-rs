use crate::SubmitBidError;
use relay_crypto::ForkDatas;
use relay_datastore::{Auctioneer, Storage};
use relay_entity::{BidSubmission, U256};
use std::sync::Arc;

pub struct SubmitBidUseCase<S: Storage, A: Auctioneer> {
    storage: S,
    auctioneer: A,
    fork_datas: ForkDatas,
}

impl<S: Storage, A: Auctioneer> SubmitBidUseCase<S, A> {
    pub fn new(storage: S, auctioneer: A, fork_datas: ForkDatas) -> Self {
        Self {
            storage,
            auctioneer,
            fork_datas,
        }
    }

    pub async fn execute(&self, bid: BidSubmission) -> Result<(), SubmitBidError> {
        let slot = bid.message.slot;
        let builder_pubkey = &bid.message.builder_pubkey;
        let value = bid.message.value;

        // 1. Zero bid check
        if value == U256(alloy_primitives::U256::ZERO) {
            return Err(SubmitBidError::ZeroBid);
        };

        //2. Whitelist check
        if !self.storage.is_whitelisted_builder(builder_pubkey) {
            return Err(SubmitBidError::UnauthorizedBuilder);
        }

        //3. Slot validation
        let head_slot = self.storage.read_head_slot();

        if !head_slot.is_next_slot(slot) {
            return Err(SubmitBidError::PastSlot);
        }

        //4. Payload attributes validation
        let attrs = self.storage.read_payload_attributes();
        if attrs.prev_randao != bid.execution_payload.prev_randao() {
            return Err(SubmitBidError::InvalidPayloadAttributes(
                "prev_randao mismatch".into(),
            ));
        }

        //5. Duty lookup
        let duty = self
            .storage
            .find_duty_by_slot(slot)
            .ok_or(SubmitBidError::DutyNotFound);

        //6. BLS signature verification
        let builder_domain = self.fork_datas.compute_builder_domain();
        if !bid.message.verify_signature(&bid.signature, builder_domain) {
            return Err(SubmitBidError::InvalidBuilderSignature);
        }

        //7. Save blinded block response
        let blinded = bid.to_blinded_block_response();
        self.storage
            .set_blinded_block_response(duty?.pubkey, blinded);

        //8. Compare and bit
        let bid_arc = Arc::new(bid);
        if let Err(e) = self.auctioneer.compare_and_bid(slot, bid_arc).await {
            return Err(SubmitBidError::BelowFloor(e.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use relay_crypto::{BlsPublicKey, BlsSignature, DST, SignedRoot};
    use relay_datastore::AuctioneerError;
    use relay_entity::{
        Address, B256, BidTrace, BlindedBlockResponse, BlobsBundle, ExecutionPayload, HeadSlot,
        PayloadAttributes, ProposerDuty, ValidatorRegistration,
    };
    use std::collections::HashMap;

    struct MockStorage {
        head_slot: parking_lot::RwLock<HeadSlot>,
        payload_attributes: parking_lot::RwLock<PayloadAttributes>,
        whitelist: parking_lot::RwLock<Vec<BlsPublicKey>>,
        duties: parking_lot::RwLock<Vec<ProposerDuty>>,
        blinded_saved: parking_lot::RwLock<bool>,
    }

    impl MockStorage {
        fn new(head_slot: HeadSlot, payload_attributes: PayloadAttributes) -> Self {
            Self {
                head_slot: parking_lot::RwLock::new(head_slot),
                payload_attributes: parking_lot::RwLock::new(payload_attributes),
                whitelist: parking_lot::RwLock::new(Vec::new()),
                duties: parking_lot::RwLock::new(Vec::new()),
                blinded_saved: parking_lot::RwLock::new(false),
            }
        }
    }

    impl Storage for MockStorage {
        fn set_head_slot(&self, slot: HeadSlot) {
            *self.head_slot.write() = slot;
        }
        fn read_head_slot(&self) -> HeadSlot {
            *self.head_slot.read()
        }
        fn set_payload_attributes(&self, _attrs: PayloadAttributes) {}
        fn read_payload_attributes(&self) -> PayloadAttributes {
            self.payload_attributes.read().clone()
        }
        fn set_validator_registration(&self, _key: BlsPublicKey, _reg: ValidatorRegistration) {}
        fn set_validator_registrations(&self, _regs: HashMap<BlsPublicKey, ValidatorRegistration>) {
        }
        fn read_validator_registration(
            &self,
            _key: &BlsPublicKey,
        ) -> Option<ValidatorRegistration> {
            None
        }
        fn empty_validator_regs(&self) -> bool {
            true
        }
        fn is_whitelisted_builder(&self, key: &BlsPublicKey) -> bool {
            self.whitelist.read().contains(key)
        }
        fn set_proposer_duties(&self, _duties: Vec<ProposerDuty>) {}
        fn find_duty_by_slot(&self, slot: u64) -> Option<ProposerDuty> {
            self.duties.read().iter().find(|d| d.slot == slot).cloned()
        }
        fn read_proposer_duties(&self) -> Vec<ProposerDuty> {
            self.duties.read().clone()
        }
        fn set_blinded_block_response(&self, _proposer: BlsPublicKey, _resp: BlindedBlockResponse) {
            *self.blinded_saved.write() = true;
        }
        fn read_blinded_block_response(
            &self,
            _proposer: &BlsPublicKey,
        ) -> Option<BlindedBlockResponse> {
            None
        }
        fn set_delivered_blocks(&self, _block_hash: B256) {}
    }

    struct MockAuctioneer {
        should_fail: bool,
    }

    #[async_trait]
    impl Auctioneer for MockAuctioneer {
        async fn compare_and_bid(
            &self,
            _slot: u64,
            _bid: Arc<BidSubmission>,
        ) -> Result<(), AuctioneerError> {
            if self.should_fail {
                Err(AuctioneerError::BidValueNotHighEnough {
                    provided: "50".into(),
                    current: "100".into(),
                })
            } else {
                Ok(())
            }
        }

        async fn get_best_bid(&self, _slot: u64) -> Result<Arc<BidSubmission>, AuctioneerError> {
            Err(AuctioneerError::BidNotFound(0))
        }
    }

    fn default_payload() -> Arc<ExecutionPayload> {
        Arc::new(ExecutionPayload {
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
            base_fee_per_gas: relay_entity::U256(alloy_primitives::U256::ZERO),
            block_hash: B256(alloy_primitives::B256::default()),
            transactions: ssz_types::VariableList::from(vec![]),
            withdrawals: ssz_types::VariableList::from(vec![]),
            blob_gas_used: 0,
            excess_blob_gas: 0,
        })
    }

    fn create_signed_bid(slot: u64, value: u128, prev_randao: B256) -> BidSubmission {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();

        let trace = BidTrace {
            slot,
            parent_hash: B256(alloy_primitives::B256::default()),
            block_hash: B256(alloy_primitives::B256::default()),
            builder_pubkey: pk,
            proposer_pubkey: BlsPublicKey::default(),
            proposer_fee_recipient: Address(alloy_primitives::Address::default()),
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            value: relay_entity::U256(alloy_primitives::U256::from(value)),
        };

        let builder_domain = ForkDatas::default().compute_builder_domain();
        let signing_root = trace.signing_root(builder_domain.into());
        let sig = BlsSignature::deserialize(&sk.sign(signing_root.as_ref(), DST, &[]).to_bytes())
            .unwrap();

        let payload = default_payload();
        // override prev_randao
        let payload = Arc::new(ExecutionPayload {
            prev_randao,
            ..(*payload).clone()
        });

        BidSubmission::new(trace, payload, Arc::new(BlobsBundle::default()), sig)
    }

    fn setup_test(
        slot: u64,
        whitelist: Vec<BlsPublicKey>,
        duties: Vec<ProposerDuty>,
        auctioneer_fails: bool,
    ) -> SubmitBidUseCase<MockStorage, MockAuctioneer> {
        let storage = MockStorage::new(
            HeadSlot(slot.wrapping_sub(1)),
            PayloadAttributes {
                timestamp: 0,
                prev_randao: alloy_primitives::B256::default(),
                suggested_fee_recipient: Address::default(),
            },
        );
        *storage.whitelist.write() = whitelist;
        *storage.duties.write() = duties;
        let auctioneer = MockAuctioneer {
            should_fail: auctioneer_fails,
        };
        SubmitBidUseCase::new(storage, auctioneer, ForkDatas::default())
    }

    #[tokio::test]
    async fn test_successful_submit() {
        let slot = 10;
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let duty = ProposerDuty {
            pubkey: BlsPublicKey::default(),
            validator_index: 1,
            slot,
        };
        let usecase = setup_test(slot, vec![pk], vec![duty], false);

        let bid = create_signed_bid(slot, 100, B256(alloy_primitives::B256::default()));

        let result = usecase.execute(bid).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_zero_bid_rejected() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let usecase = setup_test(10, vec![pk], vec![], false);

        let bid = create_signed_bid(10, 0, B256(alloy_primitives::B256::default()));

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::ZeroBid));
    }

    #[tokio::test]
    async fn test_unauthorized_builder() {
        let usecase = setup_test(10, vec![], vec![], false);

        let bid = create_signed_bid(10, 100, B256(alloy_primitives::B256::default()));

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::UnauthorizedBuilder));
    }

    #[tokio::test]
    async fn test_past_slot_rejected() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let usecase = setup_test(10, vec![pk], vec![], false);

        let bid = create_signed_bid(5, 100, B256(alloy_primitives::B256::default()));

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::PastSlot));
    }

    #[tokio::test]
    async fn test_invalid_payload_attributes() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let usecase = setup_test(10, vec![pk], vec![], false);

        let bid = create_signed_bid(10, 100, B256(alloy_primitives::B256::repeat_byte(0xff)));

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::InvalidPayloadAttributes(_)));
    }

    #[tokio::test]
    async fn test_duty_not_found() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let usecase = setup_test(10, vec![pk], vec![], false);

        let bid = create_signed_bid(10, 100, B256(alloy_primitives::B256::default()));

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::DutyNotFound));
    }

    #[tokio::test]
    async fn test_invalid_signature() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let duty = ProposerDuty {
            pubkey: BlsPublicKey::default(),
            validator_index: 1,
            slot: 10,
        };
        let usecase = setup_test(10, vec![pk], vec![duty], false);

        let mut bid = create_signed_bid(10, 100, B256(alloy_primitives::B256::default()));
        let other_sk = blst::min_pk::SecretKey::key_gen(&[99u8; 32], &[]).unwrap();
        bid.signature =
            BlsSignature::deserialize(&other_sk.sign(b"wrong", DST, &[]).to_bytes()).unwrap();

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::InvalidBuilderSignature));
    }

    #[tokio::test]
    async fn test_below_floor() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let duty = ProposerDuty {
            pubkey: BlsPublicKey::default(),
            validator_index: 1,
            slot: 10,
        };
        let usecase = setup_test(10, vec![pk], vec![duty], true);

        let bid = create_signed_bid(10, 100, B256(alloy_primitives::B256::default()));

        let err = usecase.execute(bid).await.unwrap_err();
        assert!(matches!(err, SubmitBidError::BelowFloor(_)));
    }
}
