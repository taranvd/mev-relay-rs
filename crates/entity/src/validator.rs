use crate::types::Address;
use relay_crypto::{BlsPublicKey, BlsSignature, SignedRoot};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use tree_hash_derive::TreeHash;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, TreeHash, PartialEq, Eq)]
pub struct ValidatorRegistration {
    pub fee_recipient: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_limit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    pub pubkey: BlsPublicKey,
}

impl SignedRoot for ValidatorRegistration {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedValidatorRegistration {
    pub message: ValidatorRegistration,
    pub signature: BlsSignature,
}

impl SignedValidatorRegistration {
    pub fn verify_signature(&self, domain: [u8; 32]) -> bool {
        let signing_root = self.message.signing_root(domain.into());
        self.signature
            .verify(&self.message.pubkey, signing_root.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_hash::TreeHash;
    use std::str::FromStr;

    #[test]
    fn test_validator_registration_tree_hash() {
        let reg = ValidatorRegistration {
            fee_recipient: Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            gas_limit: 30_000_000,
            timestamp: 1711287496,
            pubkey: relay_crypto::BlsPublicKey::default(),
        };

        // Just ensure it doesn't panic on TreeHash
        let root = reg.tree_hash_root();
        assert_ne!(root, tree_hash::Hash256::zero());
    }
}
