use crate::types::Address;
use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributes {
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    pub prev_randao: B256,
    pub suggested_fee_recipient: Address,
}
