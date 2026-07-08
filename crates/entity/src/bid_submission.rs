use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::bid_trace::BidTrace;
use crate::blinded_block;
use crate::execution_payload::{BlobsBundle, ExecutionPayload};
use relay_crypto::BlsSignature;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidSubmission {
    pub message: BidTrace,
    pub execution_payload: Arc<ExecutionPayload>,
    pub blobs_bundle: Arc<BlobsBundle>,
    pub signature: BlsSignature,
}

impl BidSubmission {
    pub fn new(
        message: BidTrace,
        execution_payload: Arc<ExecutionPayload>,
        blobs_bundle: Arc<BlobsBundle>,
        signature: BlsSignature,
    ) -> Self {
        Self {
            message,
            execution_payload,
            blobs_bundle,
            signature,
        }
    }

    pub fn bid_trace(&self) -> &BidTrace {
        &self.message
    }

    pub fn signature(&self) -> &BlsSignature {
        &self.signature
    }

    pub fn execution_payload(&self) -> Arc<ExecutionPayload> {
        self.execution_payload.clone()
    }

    pub fn to_blinded_block_response(&self) -> blinded_block::BlindedBlockResponse {
        blinded_block::BlindedBlockResponse {
            execution_payload: self.execution_payload.clone(),
            blobs_bundle: Some(self.blobs_bundle.clone()),
        }
    }
}
