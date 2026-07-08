use crate::UseCaseError;
use relay_crypto::{BlsSignature, ForkDatas};
use relay_datastore::Storage;
use relay_entity::{B256, BlindedBlockResponse};
use tracing::info;

pub struct UnblindBlockUseCase<S: Storage> {
    storage: S,
    fork_datas: ForkDatas,
}

impl<S: Storage> UnblindBlockUseCase<S> {
    pub fn new(storage: S, fork_datas: ForkDatas) -> Self {
        Self {
            storage,
            fork_datas,
        }
    }

    pub async fn execute(
        &self,
        slot: u64,
        proposer_index: u64,
        block_hash: B256,
        signature: BlsSignature,
    ) -> Result<BlindedBlockResponse, UseCaseError> {
        let duty = self
            .storage
            .find_duty_by_slot(slot)
            .ok_or(UseCaseError::DutyNotFound)?;

        if duty.validator_index != proposer_index {
            info!(
                target: "unblind_block",
                ?slot,
                expected = duty.validator_index,
                actual = proposer_index,
                "proposer index mismatch"
            );
            return Err(UseCaseError::ProposerIndexMismatch {
                expected: duty.validator_index,
                actual: proposer_index,
            });
        }

        let proposer_pubkey = &duty.pubkey;
        if self
            .storage
            .read_validator_registration(proposer_pubkey)
            .is_none()
        {
            info!(
                target: "unblind_block",
                ?slot,
                ?proposer_pubkey,
                "validator not registered"
            );
            return Err(UseCaseError::UnauthorizedSubmission);
        }

        let domain = self.fork_datas.compute_proposer_domain();
        if !signature.verify(
            proposer_pubkey,
            &blinded_block_signing_root(slot, proposer_index, block_hash, domain),
        ) {
            info!(
                target: "unblind_block",
                ?slot,
                ?proposer_pubkey,
                "invalid proposer signature"
            );
            return Err(UseCaseError::InvalidValidatorSignature);
        }

        let resp = self
            .storage
            .read_blinded_block_response(proposer_pubkey)
            .ok_or(UseCaseError::NoBlindedBlockResponse)?;

        if resp.execution_payload.block_hash != block_hash {
            info!(
                target: "unblind_block",
                ?slot,
                actual = ?resp.execution_payload.block_hash,
                expected = ?block_hash,
                "block hash mismatch"
            );
            return Err(UseCaseError::BlockHashMismatch);
        }

        self.storage.set_delivered_blocks(block_hash);

        info!(
            target: "unblind_block",
            ?slot,
            ?block_hash,
            "block unblinded successfully"
        );

        Ok(resp)
    }
}

fn blinded_block_signing_root(
    slot: u64,
    proposer_index: u64,
    block_hash: B256,
    domain: [u8; 32],
) -> Vec<u8> {
    let mut data = Vec::with_capacity(8 + 8 + 32 + 32);
    data.extend_from_slice(&slot.to_le_bytes());
    data.extend_from_slice(&proposer_index.to_le_bytes());
    data.extend_from_slice(block_hash.0.as_ref());
    let hash = alloy_primitives::keccak256(&data);
    let mut root = hash.to_vec();
    root.extend_from_slice(&domain);
    alloy_primitives::keccak256(&root).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_crypto::BlsPublicKey;
    use relay_entity::{
        Address, BlindedBlockResponse, BlobsBundle, ExecutionPayload, ProposerDuty,
        ValidatorRegistration,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockStorage {
        duties: parking_lot::RwLock<Vec<ProposerDuty>>,
        registrations: parking_lot::RwLock<HashMap<BlsPublicKey, ValidatorRegistration>>,
        blinded_responses: parking_lot::RwLock<HashMap<BlsPublicKey, BlindedBlockResponse>>,
        delivered: parking_lot::RwLock<Vec<B256>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                duties: parking_lot::RwLock::new(Vec::new()),
                registrations: parking_lot::RwLock::new(HashMap::new()),
                blinded_responses: parking_lot::RwLock::new(HashMap::new()),
                delivered: parking_lot::RwLock::new(Vec::new()),
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
            self.registrations.write().insert(key, reg);
        }
        fn set_validator_registrations(&self, regs: HashMap<BlsPublicKey, ValidatorRegistration>) {
            self.registrations.write().extend(regs);
        }
        fn read_validator_registration(&self, key: &BlsPublicKey) -> Option<ValidatorRegistration> {
            self.registrations.read().get(key).cloned()
        }
        fn empty_validator_regs(&self) -> bool {
            self.registrations.read().is_empty()
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
        fn set_blinded_block_response(&self, proposer: BlsPublicKey, resp: BlindedBlockResponse) {
            self.blinded_responses.write().insert(proposer, resp);
        }
        fn read_blinded_block_response(
            &self,
            proposer: &BlsPublicKey,
        ) -> Option<BlindedBlockResponse> {
            self.blinded_responses.read().get(proposer).cloned()
        }
        fn set_delivered_blocks(&self, block_hash: B256) {
            self.delivered.write().push(block_hash);
        }
    }

    fn make_payload(block_hash: B256) -> Arc<ExecutionPayload> {
        Arc::new(ExecutionPayload {
            parent_hash: B256(alloy_primitives::B256::default()),
            fee_recipient: Address::default(),
            state_root: B256(alloy_primitives::B256::default()),
            receipts_root: B256(alloy_primitives::B256::default()),
            logs_bloom: ssz_types::FixedVector::from(vec![0u8; 256]),
            prev_randao: B256(alloy_primitives::B256::default()),
            block_number: 1,
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            timestamp: 100,
            extra_data: ssz_types::VariableList::from(vec![]),
            base_fee_per_gas: relay_entity::U256(alloy_primitives::U256::ZERO),
            block_hash,
            transactions: ssz_types::VariableList::from(vec![]),
            withdrawals: ssz_types::VariableList::from(vec![]),
            blob_gas_used: 0,
            excess_blob_gas: 0,
        })
    }

    fn make_blinded_block_response(block_hash: B256) -> BlindedBlockResponse {
        BlindedBlockResponse {
            execution_payload: make_payload(block_hash),
            blobs_bundle: Some(Arc::new(BlobsBundle::default())),
        }
    }

    fn generate_keypair() -> (blst::min_pk::SecretKey, BlsPublicKey) {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        (sk, pk)
    }

    fn sign_message(
        sk: &blst::min_pk::SecretKey,
        slot: u64,
        proposer_index: u64,
        block_hash: B256,
    ) -> BlsSignature {
        let domain = ForkDatas::default().compute_proposer_domain();
        let root = blinded_block_signing_root(slot, proposer_index, block_hash, domain);
        BlsSignature::deserialize(&sk.sign(&root, relay_crypto::DST, &[]).to_bytes()).unwrap()
    }

    fn invalid_signature() -> BlsSignature {
        let sk = blst::min_pk::SecretKey::key_gen(&[99u8; 32], &[]).unwrap();
        BlsSignature::deserialize(&sk.sign(b"wrong", relay_crypto::DST, &[]).to_bytes()).unwrap()
    }

    fn make_duty(pk: BlsPublicKey) -> ProposerDuty {
        ProposerDuty {
            pubkey: pk,
            validator_index: 42,
            slot: 100,
        }
    }

    fn make_reg(pk: BlsPublicKey) -> ValidatorRegistration {
        ValidatorRegistration {
            fee_recipient: Address::default(),
            gas_limit: 30_000_000,
            timestamp: 100,
            pubkey: pk,
        }
    }

    #[tokio::test]
    async fn test_unblind_block_success() {
        let storage = MockStorage::new();
        let (sk, pk) = generate_keypair();

        storage.set_proposer_duties(vec![make_duty(pk.clone())]);
        storage.set_validator_registration(pk.clone(), make_reg(pk.clone()));

        let block_hash = B256(alloy_primitives::B256::repeat_byte(0xab));
        storage.set_blinded_block_response(pk.clone(), make_blinded_block_response(block_hash));

        let signature = sign_message(&sk, 100, 42, block_hash);

        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let result = usecase.execute(100, 42, block_hash, signature).await;
        assert!(result.is_ok());
        let unblinded = result.unwrap();
        assert_eq!(unblinded.execution_payload.block_hash, block_hash);
    }

    #[tokio::test]
    async fn test_unblind_block_duty_not_found() {
        let storage = MockStorage::new();
        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let sig = sign_message(
            &blst::min_pk::SecretKey::key_gen(&[0u8; 32], &[]).unwrap(),
            0,
            0,
            B256::default(),
        );
        let result = usecase.execute(999, 0, B256::default(), sig).await;
        assert!(matches!(result, Err(UseCaseError::DutyNotFound)));
    }

    #[tokio::test]
    async fn test_unblind_block_proposer_index_mismatch() {
        let storage = MockStorage::new();
        let (_, pk) = generate_keypair();

        storage.set_proposer_duties(vec![make_duty(pk)]);

        let sig = sign_message(
            &blst::min_pk::SecretKey::key_gen(&[0u8; 32], &[]).unwrap(),
            100,
            99,
            B256::default(),
        );
        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let result = usecase.execute(100, 99, B256::default(), sig).await;
        assert!(matches!(
            result,
            Err(UseCaseError::ProposerIndexMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn test_unblind_block_unauthorized() {
        let storage = MockStorage::new();
        let (_, pk) = generate_keypair();

        storage.set_proposer_duties(vec![make_duty(pk)]);

        let sig = sign_message(
            &blst::min_pk::SecretKey::key_gen(&[0u8; 32], &[]).unwrap(),
            100,
            42,
            B256::default(),
        );
        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let result = usecase.execute(100, 42, B256::default(), sig).await;
        assert!(matches!(result, Err(UseCaseError::UnauthorizedSubmission)));
    }

    #[tokio::test]
    async fn test_unblind_block_invalid_signature() {
        let storage = MockStorage::new();
        let (_, pk) = generate_keypair();

        storage.set_proposer_duties(vec![make_duty(pk.clone())]);
        storage.set_validator_registration(pk.clone(), make_reg(pk.clone()));

        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let result = usecase
            .execute(100, 42, B256::default(), invalid_signature())
            .await;
        assert!(matches!(
            result,
            Err(UseCaseError::InvalidValidatorSignature)
        ));
    }

    #[tokio::test]
    async fn test_unblind_block_no_blinded_response() {
        let storage = MockStorage::new();
        let (sk, pk) = generate_keypair();

        storage.set_proposer_duties(vec![make_duty(pk.clone())]);
        storage.set_validator_registration(pk.clone(), make_reg(pk.clone()));

        let block_hash = B256(alloy_primitives::B256::repeat_byte(0xab));
        let signature = sign_message(&sk, 100, 42, block_hash);

        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let result = usecase.execute(100, 42, block_hash, signature).await;
        assert!(matches!(result, Err(UseCaseError::NoBlindedBlockResponse)));
    }

    #[tokio::test]
    async fn test_unblind_block_hash_mismatch() {
        let storage = MockStorage::new();
        let (sk, pk) = generate_keypair();

        storage.set_proposer_duties(vec![make_duty(pk.clone())]);
        storage.set_validator_registration(pk.clone(), make_reg(pk.clone()));

        let stored_hash = B256(alloy_primitives::B256::repeat_byte(0xab));
        storage.set_blinded_block_response(pk, make_blinded_block_response(stored_hash));

        let wrong_hash = B256(alloy_primitives::B256::repeat_byte(0xff));
        let signature = sign_message(&sk, 100, 42, wrong_hash);

        let usecase = UnblindBlockUseCase::new(storage, ForkDatas::default());
        let result = usecase.execute(100, 42, wrong_hash, signature).await;
        assert!(matches!(result, Err(UseCaseError::BlockHashMismatch)));
    }
}
