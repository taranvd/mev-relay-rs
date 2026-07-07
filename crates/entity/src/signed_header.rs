use relay_crypto::BlsSignature;
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;

use crate::{B256, BidSubmission, ExecutionPayloadHeader};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedHeader {
    pub message: ExecutionPayloadHeader,
    pub signature: BlsSignature,
}

impl SignedHeader {
    pub fn new(message: ExecutionPayloadHeader, signature: BlsSignature) -> Self {
        Self { message, signature }
    }

    pub fn build_header(bid: &BidSubmission) -> ExecutionPayloadHeader {
        let payload = bid.execution_payload();

        let transactions_root = payload.transactions.tree_hash_root();
        let withdrawals_root = payload.withdrawals.tree_hash_root();

        ExecutionPayloadHeader {
            parent_hash: payload.parent_hash,
            fee_recipient: payload.fee_recipient,
            state_root: payload.state_root,
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom.clone(),
            prev_randao: payload.prev_randao,
            block_number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.clone(),
            base_fee_per_gas: payload.base_fee_per_gas,
            block_hash: payload.block_hash,
            transactions_root: B256(alloy_primitives::B256::from_slice(
                transactions_root.as_ref(),
            )),
            withdrawals_root: B256(alloy_primitives::B256::from_slice(
                withdrawals_root.as_ref(),
            )),
            blob_gas_used: payload.blob_gas_used,
            excess_blob_gas: payload.excess_blob_gas,
        }
    }
}
