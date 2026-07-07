use crate::UseCaseError;
use relay_crypto::ForkDatas;
use relay_datastore::Storage;
use relay_entity::SignedValidatorRegistration;
use tracing::info;

pub struct RegisterValidatorUseCase<S: Storage> {
    storage: S,
    fork_datas: ForkDatas,
}

impl<S: Storage> RegisterValidatorUseCase<S> {
    pub fn new(storage: S, fork_datas: ForkDatas) -> Self {
        Self {
            storage,
            fork_datas,
        }
    }

    pub async fn execute(
        &self,
        registration: SignedValidatorRegistration,
    ) -> Result<(), UseCaseError> {
        let pubkey = &registration.message.pubkey;

        let domain = self.fork_datas.compute_builder_domain();
        if !registration.verify_signature(domain) {
            info!(
                target: "register_validator",
                ?pubkey,
                "invalid validator signature"
            );
            return Err(UseCaseError::InvalidValidatorSignature);
        }

        info!(
            target: "register_validator",
            ?pubkey,
            "validator registered"
        );

        self.storage
            .set_validator_registration(pubkey.clone(), registration.message);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blst;
    use relay_crypto::{BlsPublicKey, BlsSignature, DST, SignedRoot};
    use relay_entity::{Address, ValidatorRegistration};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockStorage {
        stored: parking_lot::RwLock<Option<(BlsPublicKey, ValidatorRegistration)>>,
        registered: parking_lot::RwLock<bool>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                stored: parking_lot::RwLock::new(None),
                registered: parking_lot::RwLock::new(false),
            }
        }
    }

    impl Storage for MockStorage {
        fn set_head_slot(&self, _slot: relay_entity::HeadSlot) {}
        fn read_head_slot(&self) -> relay_entity::HeadSlot {
            relay_entity::HeadSlot(0)
        }
        fn set_payload_attributes(&self, _attrs: relay_entity::PayloadAttributes) {}
        fn read_payload_attributes(&self) -> relay_entity::PayloadAttributes {
            relay_entity::PayloadAttributes {
                timestamp: 0,
                prev_randao: alloy_primitives::B256::default(),
                suggested_fee_recipient: Address::default(),
            }
        }
        fn set_validator_registration(&self, key: BlsPublicKey, reg: ValidatorRegistration) {
            *self.registered.write() = true;
            *self.stored.write() = Some((key, reg));
        }
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
        fn is_whitelisted_builder(&self, _key: &BlsPublicKey) -> bool {
            false
        }
        fn set_proposer_duties(&self, _duties: Vec<relay_entity::ProposerDuty>) {}
        fn find_duty_by_slot(&self, _slot: u64) -> Option<relay_entity::ProposerDuty> {
            None
        }
        fn read_proposer_duties(&self) -> Vec<relay_entity::ProposerDuty> {
            vec![]
        }
        fn set_blinded_block_response(
            &self,
            _proposer: BlsPublicKey,
            _resp: relay_entity::BlindedBlockResponse,
        ) {
        }
        fn read_blinded_block_response(
            &self,
            _proposer: &BlsPublicKey,
        ) -> Option<relay_entity::BlindedBlockResponse> {
            None
        }
        fn set_delivered_blocks(&self, _block_hash: relay_entity::B256) {}
    }

    fn create_signed_registration(sk: &blst::min_pk::SecretKey) -> SignedValidatorRegistration {
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let registration = ValidatorRegistration {
            fee_recipient: Address::default(),
            gas_limit: 30_000_000,
            timestamp: 1711287496,
            pubkey: pk,
        };

        let domain = ForkDatas::default().compute_builder_domain();
        let signing_root = registration.signing_root(domain.into());
        let sig = BlsSignature::deserialize(&sk.sign(signing_root.as_ref(), DST, &[]).to_bytes())
            .unwrap();

        SignedValidatorRegistration {
            message: registration,
            signature: sig,
        }
    }

    #[tokio::test]
    async fn test_successful_registration() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let storage = MockStorage::new();
        let usecase = RegisterValidatorUseCase::new(storage, ForkDatas::default());

        let registration = create_signed_registration(&sk);
        let result = usecase.execute(registration).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_signature_rejected() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let storage = MockStorage::new();
        let usecase = RegisterValidatorUseCase::new(storage, ForkDatas::default());

        let mut registration = create_signed_registration(&sk);
        let other_sk = blst::min_pk::SecretKey::key_gen(&[99u8; 32], &[]).unwrap();
        registration.signature =
            BlsSignature::deserialize(&other_sk.sign(b"wrong", DST, &[]).to_bytes()).unwrap();

        let err = usecase.execute(registration).await.unwrap_err();
        assert!(matches!(err, UseCaseError::InvalidValidatorSignature));
    }

    #[tokio::test]
    async fn test_storage_persisted() {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let registered = Arc::new(parking_lot::RwLock::new(false));
        let registered_clone = Arc::clone(&registered);

        struct CheckStorage {
            flag: Arc<parking_lot::RwLock<bool>>,
        }

        impl Storage for CheckStorage {
            fn set_head_slot(&self, _: relay_entity::HeadSlot) {}
            fn read_head_slot(&self) -> relay_entity::HeadSlot {
                relay_entity::HeadSlot(0)
            }
            fn set_payload_attributes(&self, _: relay_entity::PayloadAttributes) {}
            fn read_payload_attributes(&self) -> relay_entity::PayloadAttributes {
                relay_entity::PayloadAttributes {
                    timestamp: 0,
                    prev_randao: alloy_primitives::B256::default(),
                    suggested_fee_recipient: Address::default(),
                }
            }
            fn set_validator_registration(&self, _key: BlsPublicKey, _reg: ValidatorRegistration) {
                *self.flag.write() = true;
            }
            fn set_validator_registrations(&self, _: HashMap<BlsPublicKey, ValidatorRegistration>) {
            }
            fn read_validator_registration(
                &self,
                _: &BlsPublicKey,
            ) -> Option<ValidatorRegistration> {
                None
            }
            fn empty_validator_regs(&self) -> bool {
                true
            }
            fn is_whitelisted_builder(&self, _: &BlsPublicKey) -> bool {
                false
            }
            fn set_proposer_duties(&self, _: Vec<relay_entity::ProposerDuty>) {}
            fn find_duty_by_slot(&self, _: u64) -> Option<relay_entity::ProposerDuty> {
                None
            }
            fn read_proposer_duties(&self) -> Vec<relay_entity::ProposerDuty> {
                vec![]
            }
            fn set_blinded_block_response(
                &self,
                _: BlsPublicKey,
                _: relay_entity::BlindedBlockResponse,
            ) {
            }
            fn read_blinded_block_response(
                &self,
                _: &BlsPublicKey,
            ) -> Option<relay_entity::BlindedBlockResponse> {
                None
            }
            fn set_delivered_blocks(&self, _: relay_entity::B256) {}
        }

        let usecase = RegisterValidatorUseCase::new(
            CheckStorage {
                flag: registered_clone,
            },
            ForkDatas::default(),
        );

        let registration = create_signed_registration(&sk);
        usecase.execute(registration).await.unwrap();

        assert!(*registered.read());
    }
}
