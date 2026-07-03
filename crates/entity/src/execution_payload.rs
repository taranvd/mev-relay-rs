use crate::types::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use ssz_types::{FixedVector, VariableList};
use tree_hash_derive::TreeHash;
use typenum::{U16, U32, U256 as TypenumU256, U1048576, U1073741824};

pub type Bytes48 = alloy_primitives::FixedBytes<48>;
pub type KzgCommitment = Bytes48;
pub type Blob = alloy_primitives::FixedBytes<131072>;

/// Withdrawal represents a validator withdrawal from the consensus layer.
#[serde_as]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TreeHash)]
pub struct Withdrawal {
    #[serde_as(as = "DisplayFromStr")]
    pub index: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub validator_index: u64,
    pub address: Address,
    #[serde_as(as = "DisplayFromStr")]
    pub amount: u64,
}

/// Execution payload.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, TreeHash)]
pub struct ExecutionPayload {
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    #[serde(with = "ssz_types::serde_utils::hex_fixed_vec")]
    pub logs_bloom: FixedVector<u8, TypenumU256>,
    pub prev_randao: B256,
    #[serde_as(as = "DisplayFromStr")]
    pub block_number: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_limit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_used: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    #[serde(with = "ssz_types::serde_utils::hex_var_list")]
    pub extra_data: VariableList<u8, U32>,
    #[serde_as(as = "DisplayFromStr")]
    pub base_fee_per_gas: U256,
    pub block_hash: B256,
    #[serde(with = "ssz_types::serde_utils::list_of_hex_var_list")]
    pub transactions: VariableList<VariableList<u8, U1073741824>, U1048576>,
    pub withdrawals: VariableList<Withdrawal, U16>,
    #[serde_as(as = "DisplayFromStr")]
    pub blob_gas_used: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub excess_blob_gas: u64,
}

/// Execution payload header.
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, TreeHash)]
pub struct ExecutionPayloadHeader {
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    #[serde(with = "ssz_types::serde_utils::hex_fixed_vec")]
    pub logs_bloom: FixedVector<u8, TypenumU256>,
    pub prev_randao: B256,
    #[serde_as(as = "DisplayFromStr")]
    pub block_number: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_limit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub gas_used: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    #[serde(with = "ssz_types::serde_utils::hex_var_list")]
    pub extra_data: VariableList<u8, U32>,
    #[serde_as(as = "DisplayFromStr")]
    pub base_fee_per_gas: U256,
    pub block_hash: B256,
    pub transactions_root: B256,
    pub withdrawals_root: B256,
    #[serde_as(as = "DisplayFromStr")]
    pub blob_gas_used: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub excess_blob_gas: u64,
}

/// Blobs bundle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlobsBundle {
    pub commitments: Vec<Bytes48>,
    pub proofs: Vec<Bytes48>,
    pub blobs: Vec<Blob>,
}
