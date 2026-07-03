use alloy_primitives::B256 as AlloyB256;
use parking_lot::RwLock;
use relay_crypto::BlsPublicKey;
use relay_entity::{
    Address, BlindedBlockResponse, HeadSlot, PayloadAttributes, ProposerDuty, ValidatorRegistration,
    B256,
};
use std::collections::HashMap;
use std::sync::Arc;

use crate::Storage;

struct MemoryStorageInner {
    head_slot: HeadSlot,
    payload_attributes: PayloadAttributes,
    validator_registrations: HashMap<BlsPublicKey, ValidatorRegistration>,
    proposer_duties: Vec<ProposerDuty>,
    whitelist: Vec<BlsPublicKey>,
    blinded_blocks: HashMap<BlsPublicKey, BlindedBlockResponse>,
    delivered_blocks: Vec<B256>,
}

pub struct MemoryStorage {
    inner: Arc<RwLock<MemoryStorageInner>>,
}

impl MemoryStorage {
    pub fn new(whitelist: Vec<BlsPublicKey>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(MemoryStorageInner {
                head_slot: HeadSlot(0),
                payload_attributes: PayloadAttributes {
                    timestamp: 0,
                    prev_randao: AlloyB256::default(),
                    suggested_fee_recipient: Address::default(),
                },
                validator_registrations: HashMap::new(),
                proposer_duties: Vec::new(),
                whitelist,
                blinded_blocks: HashMap::new(),
                delivered_blocks: Vec::new(),
            })),
        }
    }
}

impl Storage for MemoryStorage {
    fn set_head_slot(&self, slot: HeadSlot) {
        self.inner.write().head_slot = slot;
    }

    fn read_head_slot(&self) -> HeadSlot {
        self.inner.read().head_slot
    }

    fn set_payload_attributes(&self, attrs: PayloadAttributes) {
        self.inner.write().payload_attributes = attrs;
    }

    fn read_payload_attributes(&self) -> PayloadAttributes {
        self.inner.read().payload_attributes.clone()
    }

    fn set_validator_registration(&self, key: BlsPublicKey, reg: ValidatorRegistration) {
        self.inner.write().validator_registrations.insert(key, reg);
    }

    fn set_validator_registrations(&self, regs: HashMap<BlsPublicKey, ValidatorRegistration>) {
        self.inner.write().validator_registrations = regs;
    }

    fn read_validator_registration(&self, key: &BlsPublicKey) -> Option<ValidatorRegistration> {
        self.inner.read().validator_registrations.get(key).cloned()
    }

    fn empty_validator_regs(&self) -> bool {
        self.inner.read().validator_registrations.is_empty()
    }

    fn is_whitelisted_builder(&self, key: &BlsPublicKey) -> bool {
        self.inner.read().whitelist.contains(key)
    }

    fn set_proposer_duties(&self, duties: Vec<ProposerDuty>) {
        self.inner.write().proposer_duties = duties;
    }

    fn find_duty_by_slot(&self, slot: u64) -> Option<ProposerDuty> {
        self.inner
            .read()
            .proposer_duties
            .iter()
            .find(|d| d.slot == slot)
            .cloned()
    }

    fn read_proposer_duties(&self) -> Vec<ProposerDuty> {
        self.inner.read().proposer_duties.clone()
    }

    fn set_blinded_block_response(&self, proposer: BlsPublicKey, resp: BlindedBlockResponse) {
        self.inner.write().blinded_blocks.insert(proposer, resp);
    }

    fn read_blinded_block_response(&self, proposer: &BlsPublicKey) -> Option<BlindedBlockResponse> {
        self.inner.read().blinded_blocks.get(&proposer).cloned()
    }

    fn set_delivered_blocks(&self, block_hash: B256) {
        self.inner.write().delivered_blocks.push(block_hash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_head_slot_roundtrip() {
        let storage = MemoryStorage::new(vec![]);
        storage.set_head_slot(HeadSlot(42));
        assert_eq!(storage.read_head_slot(), HeadSlot(42));
    }

    #[test]
    fn test_payload_attributes_roundtrip() {
        let storage = MemoryStorage::new(vec![]);
        let attrs = PayloadAttributes {
            timestamp: 12345,
            prev_randao: AlloyB256::default(),
            suggested_fee_recipient: Address::default(),
        };
        storage.set_payload_attributes(attrs.clone());
        let read = storage.read_payload_attributes();
        assert_eq!(read.timestamp, 12345);
    }

    #[test]
    fn test_validator_registration() {
        let storage = MemoryStorage::new(vec![]);
        let key = BlsPublicKey::default();
        let reg = ValidatorRegistration {
            fee_recipient: Address::default(),
            gas_limit: 30_000_000,
            timestamp: 1000,
            pubkey: key.clone(),
        };

        assert!(storage.empty_validator_regs());
        assert!(storage.read_validator_registration(&key).is_none());

        storage.set_validator_registration(key.clone(), reg.clone());
        assert!(!storage.empty_validator_regs());

        let read = storage.read_validator_registration(&key).unwrap();
        assert_eq!(read.gas_limit, 30_000_000);
    }

    #[test]
    fn test_whitelist() {
        let key = BlsPublicKey::default();
        let sk = blst::min_pk::SecretKey::key_gen(&[1u8; 32], &[]).unwrap();
        let other = relay_crypto::BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        let storage = MemoryStorage::new(vec![key.clone()]);
        assert!(storage.is_whitelisted_builder(&key));
        assert!(!storage.is_whitelisted_builder(&other));
    }

    #[test]
    fn test_proposer_duties() {
        let storage = MemoryStorage::new(vec![]);
        let duty = ProposerDuty {
            pubkey: BlsPublicKey::default(),
            validator_index: 1,
            slot: 10,
        };

        assert!(storage.find_duty_by_slot(10).is_none());

        storage.set_proposer_duties(vec![duty.clone()]);
        assert_eq!(storage.find_duty_by_slot(10).unwrap().validator_index, 1);
        assert_eq!(storage.read_proposer_duties().len(), 1);
    }

    #[test]
    fn test_blinded_block_response() {
        let storage = MemoryStorage::new(vec![]);
        let key = BlsPublicKey::default();
        let resp = BlindedBlockResponse {
            execution_payload: Arc::new(relay_entity::ExecutionPayload {
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
            }),
            blobs_bundle: None,
        };

        assert!(storage.read_blinded_block_response(&key).is_none());

        storage.set_blinded_block_response(key.clone(), resp.clone());
        let read = storage.read_blinded_block_response(&key).unwrap();
        assert!(read.blobs_bundle.is_none());
    }

    #[test]
    fn test_delivered_blocks() {
        let storage = MemoryStorage::new(vec![]);
        let hash = B256(alloy_primitives::B256::repeat_byte(0xab));

        // no way to read delivered blocks, just ensure it doesn't panic
        storage.set_delivered_blocks(hash);
        storage.set_delivered_blocks(hash);
    }

    #[test]
    fn test_set_validator_registrations_overwrites() {
        let storage = MemoryStorage::new(vec![]);
        let key1 = BlsPublicKey::default();
        let sk2 = blst::min_pk::SecretKey::key_gen(&[2u8; 32], &[]).unwrap();
        let key2 = relay_crypto::BlsPublicKey::deserialize(&sk2.sk_to_pk().compress()).unwrap();

        let reg1 = ValidatorRegistration {
            fee_recipient: Address::default(),
            gas_limit: 30_000_000,
            timestamp: 1000,
            pubkey: key1.clone(),
        };
        let reg2 = ValidatorRegistration {
            fee_recipient: Address::default(),
            gas_limit: 36_000_000,
            timestamp: 2000,
            pubkey: key2,
        };

        let mut regs = HashMap::new();
        regs.insert(key1.clone(), reg1.clone());
        storage.set_validator_registrations(regs);

        assert_eq!(
            storage
                .read_validator_registration(&key1)
                .unwrap()
                .gas_limit,
            30_000_000
        );

        let mut regs2 = HashMap::new();
        regs2.insert(key1.clone(), reg2.clone());
        storage.set_validator_registrations(regs2);

        assert_eq!(
            storage
                .read_validator_registration(&key1)
                .unwrap()
                .gas_limit,
            36_000_000
        );
    }
}
