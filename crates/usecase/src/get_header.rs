use crate::UseCaseError;
use relay_crypto::BlsPublicKey;
use relay_datastore::{Auctioneer, Storage};
use relay_entity::{B256, SignedHeader, Versioned};
use relay_gateway::BlsSigner;
use std::sync::Arc;
use tracing::info;

pub struct GetHeaderUseCase<S: Storage, A: Auctioneer> {
    storage: S,
    auctioneer: A,
    signer: Arc<BlsSigner>,
}

impl<S: Storage, A: Auctioneer> GetHeaderUseCase<S, A> {
    pub fn new(storage: S, auctioneer: A, signer: Arc<BlsSigner>) -> Self {
        Self {
            storage,
            auctioneer,
            signer,
        }
    }

    pub async fn execute(
        &self,
        slot: u64,
        parent_hash: B256,
        proposer_pubkey: BlsPublicKey,
    ) -> Result<Versioned<SignedHeader>, UseCaseError> {
        if self
            .storage
            .read_validator_registration(&proposer_pubkey)
            .is_none()
        {
            info!(
                target: "get_header",
                ?slot,
                ?proposer_pubkey,
                "validator not registered"
            );
            return Err(UseCaseError::UnauthorizedGetHeader);
        }

        let duty = self
            .storage
            .find_duty_by_slot(slot)
            .ok_or(UseCaseError::DutyNotFound)?;

        if duty.pubkey != proposer_pubkey {
            info!(
                target: "get_header",
                ?slot,
                ?proposer_pubkey,
                duty_proposer = ?duty.pubkey,
                "proposer pubkey mismatch"
            );
            return Err(UseCaseError::UnauthorizedGetHeader);
        }

        let head_slot = self.storage.read_head_slot();
        if !head_slot.is_next_slot(slot) {
            info!(
                target: "get_header",
                ?slot,
                head_slot = %head_slot.0,
                "invalid slot"
            );
            return Err(UseCaseError::InvalidSlot);
        }

        let best_bid = self
            .auctioneer
            .get_best_bid(slot)
            .await
            .map_err(UseCaseError::from)?;

        if best_bid.bid_trace().parent_hash != parent_hash {
            info!(
                target: "get_header",
                ?slot,
                expected = ?parent_hash,
                got = ?best_bid.bid_trace().parent_hash,
                "parent_hash mismatch"
            );
            return Err(UseCaseError::NoBidFound);
        }

        info!(
            target: "get_header",
            ?slot,
            block_hash = ?best_bid.bid_trace().block_hash,
            value = ?best_bid.bid_trace().value,
            "best bid found"
        );

        let version = self.signer.fork_name();
        let header = SignedHeader::build_header(&best_bid);
        let signature = self.signer.sign(&header);
        let signed_header = SignedHeader::new(header, signature);

        info!(target: "get_header", ?slot, ?version, "signed header returned");

        Ok(Versioned::new(version, signed_header))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use blst;
    use relay_crypto::{BlsPublicKey, BlsSecretKey, ForkDatas};
    use relay_datastore::AuctioneerError;
    use relay_entity::{
        Address, B256, BidSubmission, BlindedBlockResponse, ExecutionPayload, HeadSlot,
        PayloadAttributes, ProposerDuty, ValidatorRegistration,
    };
    use std::collections::HashMap;

    struct MockStorage {
        registered: parking_lot::RwLock<Vec<BlsPublicKey>>,
        head_slot: parking_lot::RwLock<HeadSlot>,
        duties: parking_lot::RwLock<Vec<ProposerDuty>>,
    }

    impl MockStorage {
        fn new(head_slot: HeadSlot) -> Self {
            Self {
                registered: parking_lot::RwLock::new(Vec::new()),
                head_slot: parking_lot::RwLock::new(head_slot),
                duties: parking_lot::RwLock::new(Vec::new()),
            }
        }
    }

    impl Storage for MockStorage {
        fn set_head_slot(&self, _slot: HeadSlot) {}
        fn read_head_slot(&self) -> HeadSlot {
            *self.head_slot.read()
        }
        fn set_payload_attributes(&self, _attrs: PayloadAttributes) {}
        fn read_payload_attributes(&self) -> PayloadAttributes {
            PayloadAttributes {
                timestamp: 0,
                prev_randao: alloy_primitives::B256::default(),
                suggested_fee_recipient: Address::default(),
            }
        }
        fn set_validator_registration(&self, key: BlsPublicKey, _reg: ValidatorRegistration) {
            self.registered.write().push(key);
        }
        fn set_validator_registrations(&self, _regs: HashMap<BlsPublicKey, ValidatorRegistration>) {
        }
        fn read_validator_registration(&self, key: &BlsPublicKey) -> Option<ValidatorRegistration> {
            if self.registered.read().contains(key) {
                Some(ValidatorRegistration {
                    fee_recipient: Address::default(),
                    gas_limit: 30_000_000,
                    timestamp: 0,
                    pubkey: key.clone(),
                })
            } else {
                None
            }
        }
        fn empty_validator_regs(&self) -> bool {
            self.registered.read().is_empty()
        }
        fn is_whitelisted_builder(&self, _key: &BlsPublicKey) -> bool {
            false
        }
        fn set_proposer_duties(&self, duties: Vec<ProposerDuty>) {
            *self.duties.write() = duties;
        }
        fn find_duty_by_slot(&self, slot: u64) -> Option<ProposerDuty> {
            self.duties.read().iter().find(|d| d.slot == slot).cloned()
        }
        fn read_proposer_duties(&self) -> Vec<ProposerDuty> {
            self.duties.read().clone()
        }
        fn set_blinded_block_response(&self, _proposer: BlsPublicKey, _resp: BlindedBlockResponse) {
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
        best_bid: Option<Arc<BidSubmission>>,
    }

    #[async_trait]
    impl Auctioneer for MockAuctioneer {
        async fn compare_and_bid(
            &self,
            _slot: u64,
            _bid: Arc<BidSubmission>,
        ) -> Result<(), AuctioneerError> {
            Ok(())
        }

        async fn get_best_bid(&self, slot: u64) -> Result<Arc<BidSubmission>, AuctioneerError> {
            self.best_bid
                .clone()
                .ok_or(AuctioneerError::BidNotFound(slot))
        }
    }

    fn default_bid(prev_randao: B256) -> Arc<BidSubmission> {
        Arc::new(BidSubmission::new(
            relay_entity::BidTrace {
                slot: 0,
                parent_hash: B256(alloy_primitives::B256::default()),
                block_hash: B256(alloy_primitives::B256::default()),
                builder_pubkey: BlsPublicKey::default(),
                proposer_pubkey: BlsPublicKey::default(),
                proposer_fee_recipient: Address::default(),
                gas_limit: 30_000_000,
                gas_used: 15_000_000,
                value: relay_entity::U256(alloy_primitives::U256::from(100)),
            },
            Arc::new(ExecutionPayload {
                parent_hash: B256(alloy_primitives::B256::default()),
                fee_recipient: Address::default(),
                state_root: B256(alloy_primitives::B256::default()),
                receipts_root: B256(alloy_primitives::B256::default()),
                logs_bloom: ssz_types::FixedVector::from(vec![0u8; 256]),
                prev_randao,
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
            }),
            Arc::new(relay_entity::BlobsBundle::default()),
            {
                let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
                relay_crypto::BlsSignature::deserialize(
                    &sk.sign(b"test", relay_crypto::DST, &[]).compress(),
                )
                .unwrap()
            },
        ))
    }

    fn setup_test(
        slot: u64,
        registered_validator: Option<BlsPublicKey>,
        best_bid: Option<Arc<BidSubmission>>,
    ) -> GetHeaderUseCase<MockStorage, MockAuctioneer> {
        let storage = MockStorage::new(HeadSlot(slot.wrapping_sub(1)));
        if let Some(pk) = registered_validator {
            storage.registered.write().push(pk);
        }
        let duty = ProposerDuty {
            pubkey: BlsPublicKey::default(),
            validator_index: 1,
            slot,
        };
        storage.duties.write().push(duty);
        let auctioneer = MockAuctioneer { best_bid };
        let sk = BlsSecretKey::random();
        let signer = Arc::new(BlsSigner::new(
            sk,
            ForkDatas::default(),
            relay_entity::ForkName::Electra,
        ));
        GetHeaderUseCase::new(storage, auctioneer, signer)
    }

    #[tokio::test]
    async fn test_unauthorized_get_header() {
        let usecase = setup_test(10, None, None);
        let err = usecase
            .execute(
                10,
                B256(alloy_primitives::B256::default()),
                BlsPublicKey::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, UseCaseError::UnauthorizedGetHeader));
    }

    #[tokio::test]
    async fn test_duty_not_found() {
        let storage = MockStorage::new(HeadSlot(9));
        let pk = BlsPublicKey::default();
        storage.registered.write().push(pk.clone());
        let auctioneer = MockAuctioneer { best_bid: None };
        let sk = BlsSecretKey::random();
        let signer = Arc::new(BlsSigner::new(
            sk,
            ForkDatas::default(),
            relay_entity::ForkName::Electra,
        ));
        let usecase = GetHeaderUseCase::new(storage, auctioneer, signer);
        let err = usecase
            .execute(10, B256(alloy_primitives::B256::default()), pk)
            .await
            .unwrap_err();
        assert!(matches!(err, UseCaseError::DutyNotFound));
    }

    #[tokio::test]
    async fn test_invalid_slot() {
        let pk = BlsPublicKey::default();
        let storage = MockStorage::new(HeadSlot(10));
        storage.registered.write().push(pk.clone());
        storage.duties.write().push(ProposerDuty {
            pubkey: BlsPublicKey::default(),
            validator_index: 1,
            slot: 5,
        });
        let auctioneer = MockAuctioneer { best_bid: None };
        let sk = BlsSecretKey::random();
        let signer = Arc::new(BlsSigner::new(
            sk,
            ForkDatas::default(),
            relay_entity::ForkName::Electra,
        ));
        let usecase = GetHeaderUseCase::new(storage, auctioneer, signer);
        let err = usecase
            .execute(5, B256(alloy_primitives::B256::default()), pk)
            .await
            .unwrap_err();
        assert!(matches!(err, UseCaseError::InvalidSlot));
    }

    #[tokio::test]
    async fn test_bid_not_found() {
        let pk = BlsPublicKey::default();
        let usecase = setup_test(10, Some(pk.clone()), None);
        let err = usecase
            .execute(10, B256(alloy_primitives::B256::default()), pk)
            .await
            .unwrap_err();
        assert!(matches!(err, UseCaseError::AuctioneerError(_)));
    }

    #[tokio::test]
    async fn test_successful_get_header() {
        let pk = BlsPublicKey::default();
        let bid = default_bid(B256(alloy_primitives::B256::default()));
        let usecase = setup_test(10, Some(pk.clone()), Some(bid));
        let result = usecase
            .execute(10, B256(alloy_primitives::B256::default()), pk)
            .await;
        assert!(result.is_ok());
        let versioned = result.unwrap();
        assert_eq!(versioned.version, relay_entity::ForkName::Electra);
    }
}
