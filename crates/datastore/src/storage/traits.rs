use relay_crypto::BlsPublicKey;
use relay_entity::{
    BlindedBlockResponse, HeadSlot, PayloadAttributes, ProposerDuty, ValidatorRegistration, B256,
};
use std::collections::HashMap;

pub trait Storage: Send + Sync {
    fn set_head_slot(&self, slot: HeadSlot);
    fn read_head_slot(&self) -> HeadSlot;
    fn set_payload_attributes(&self, attrs: PayloadAttributes);
    fn read_payload_attributes(&self) -> PayloadAttributes;
    fn set_validator_registration(&self, key: BlsPublicKey, reg: ValidatorRegistration);
    fn set_validator_registrations(&self, regs: HashMap<BlsPublicKey, ValidatorRegistration>);
    fn read_validator_registration(&self, key: &BlsPublicKey) -> Option<ValidatorRegistration>;
    fn empty_validator_regs(&self) -> bool;
    fn is_whitelisted_builder(&self, key: &BlsPublicKey) -> bool;
    fn set_proposer_duties(&self, duties: Vec<ProposerDuty>);
    fn find_duty_by_slot(&self, slot: u64) -> Option<ProposerDuty>;
    fn read_proposer_duties(&self) -> Vec<ProposerDuty>;
    fn set_blinded_block_response(&self, proposer: BlsPublicKey, resp: BlindedBlockResponse);
    fn read_blinded_block_response(&self, proposer: &BlsPublicKey) -> Option<BlindedBlockResponse>;
    fn set_delivered_blocks(&self, block_hash: B256);
}
