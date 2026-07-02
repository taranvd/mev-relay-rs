use relay_crypto::BlsPublicKey;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposerDuty {
    pub pubkey: BlsPublicKey,
    #[serde_as(as = "DisplayFromStr")]
    pub validator_index: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub slot: u64,
}
