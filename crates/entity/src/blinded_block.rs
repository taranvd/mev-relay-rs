use crate::execution_payload::{BlobsBundle, ExecutionPayload};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindedBlockResponse {
    pub execution_payload: Arc<ExecutionPayload>,
    pub blobs_bundle: Option<Arc<BlobsBundle>>,
}
