use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadEvent {
    #[serde_as(as = "DisplayFromStr")]
    pub slot: u64,
    pub block: String,
    pub epoch_transition: bool,
    pub execution_optimistic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributesEvent {
    pub version: String,
    pub data: PayloadAttributesData,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributesData {
    #[serde_as(as = "DisplayFromStr")]
    pub proposal_slot: u64,
    pub parent_block_hash: String,
    pub payload_attributes: InnerPayloadAttributes,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerPayloadAttributes {
    #[serde_as(as = "DisplayFromStr")]
    pub timestamp: u64,
    pub prev_randao: String,
    pub suggested_fee_recipient: String,
}

#[derive(Debug, Clone)]
pub enum BeaconEvent {
    Head(HeadEvent),
    PayloadAttributes(PayloadAttributesEvent),
}
