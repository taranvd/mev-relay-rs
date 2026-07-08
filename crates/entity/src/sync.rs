use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    #[serde_as(as = "DisplayFromStr")]
    pub head_slot: u64,
    pub is_syncing: bool,
}
