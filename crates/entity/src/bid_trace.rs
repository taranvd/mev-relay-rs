use std::cmp::Ordering;

use crate::types::{Address, B256, U256};
use relay_crypto::{BlsPublicKey, BlsSignature, SignedRoot};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use tree_hash_derive::TreeHash;

/// The Bid trace type.
#[serde_as]
#[derive(Debug, Clone, TreeHash, Serialize, Deserialize, PartialEq, Eq)]
pub struct BidTrace {
    #[serde_as(as = "DisplayFromStr")]
    pub slot: u64,
    pub parent_hash: B256,
    pub block_hash: B256,
    pub builder_pubkey: BlsPublicKey,
    pub proposer_pubkey: BlsPublicKey,
    pub proposer_fee_recipient: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_limit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_used: u64,
    pub value: U256,
}

impl BidTrace {
    pub fn verify_signature(&self, signature: &BlsSignature, domain: [u8; 32]) -> bool {
        let signing_root = self.signing_root(domain.into());
        signature.verify(&self.builder_pubkey, signing_root.as_ref())
    }
}

impl PartialOrd for BidTrace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BidTrace {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl SignedRoot for BidTrace {}
