use super::error::BeaconError;
use alloy_primitives::B256;
use relay_crypto::{BlsPublicKey, ForkData, ForkDatas};
use relay_entity::{ProposerDuty, SyncStatus};
use reqwest::Client;
use serde::Deserialize;
use serde_with::{DisplayFromStr, serde_as};
use std::future::Future;
use std::pin::Pin;
use tracing::info;
use url::Url;

pub type BoxedFuture<T> = Pin<Box<dyn Future<Output = Result<T, BeaconError>> + Send>>;

pub trait BeaconApi: Send + Sync {
    fn sync_status(&self) -> BoxedFuture<SyncStatus>;
    fn proposer_duties(&self, epoch: u64) -> BoxedFuture<Vec<ProposerDuty>>;
    fn get_fork_data(&self) -> BoxedFuture<ForkDatas>;
    fn get_known_validator(&self, pubkey: BlsPublicKey) -> BoxedFuture<KnownValidator>;
    fn publish_block(
        &self,
        block: SignedBeaconBlockContent,
        submission_type: SubmissionType,
    ) -> BoxedFuture<B256>;
}

#[derive(Debug, Clone)]
pub struct SignedBeaconBlockContent {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BeaconBlockBody,
}

#[derive(Debug, Clone)]
pub struct BeaconBlockBody {
    pub execution_payload: relay_entity::ExecutionPayload,
    pub blobs: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionType {
    Json,
    Ssz,
}

#[derive(Debug, Clone)]
pub struct KnownValidator {
    pub index: u64,
    pub balance: u64,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct BeaconNodeApi {
    client: Client,
    url: Url,
}

impl BeaconNodeApi {
    pub fn new(url: Url) -> Self {
        info!(target: "beacon_client", %url, "creating beacon node api client");
        Self {
            client: Client::new(),
            url,
        }
    }

    pub async fn fetch_head_slot(&self) -> Result<u64, BeaconError> {
        let url = format!("{}eth/v1/beacon/headers/head", self.url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(BeaconError::Http)?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(BeaconError::Api {
                status: status.as_u16(),
                body,
            });
        }
        let body: serde_json::Value = resp.json().await.map_err(BeaconError::Deserialize)?;
        let slot = body["data"]["header"]["message"]["slot"]
            .as_str()
            .ok_or_else(|| BeaconError::Sse("no slot in head response".into()))?
            .parse::<u64>()
            .map_err(|e| BeaconError::Sse(format!("slot parse failed: {e}")))?;
        Ok(slot)
    }

    pub async fn subscribe_events(&self) -> Result<reqwest::Response, BeaconError> {
        let url = format!("{}eth/v1/events?topics=head,payload_attributes", self.url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(BeaconError::Http)?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(BeaconError::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(resp)
    }
}

impl BeaconApi for BeaconNodeApi {
    fn sync_status(&self) -> BoxedFuture<SyncStatus> {
        let client = self.client.clone();
        let url = format!("{}{}", self.url, "eth/v1/node/syncing");
        Box::pin(async move {
            let resp: SyncStatusResponse = get_json(&client, &url).await?;
            Ok(resp.data)
        })
    }

    fn proposer_duties(&self, epoch: u64) -> BoxedFuture<Vec<ProposerDuty>> {
        let client = self.client.clone();
        let url = format!(
            "{}{}{}",
            self.url, "eth/v1/validator/duties/proposer/", epoch
        );
        Box::pin(async move {
            let resp: ProposerDutiesResponse = get_json(&client, &url).await?;
            Ok(resp.data.into_iter().map(Into::into).collect())
        })
    }

    fn get_fork_data(&self) -> BoxedFuture<ForkDatas> {
        let client = self.client.clone();
        let url = format!("{}{}", self.url, "eth/v2/debug/beacon/states/head");
        Box::pin(async move {
            let resp: GetForkResponse = get_json(&client, &url).await?;
            let gvr_bytes = hex::decode(resp.data.genesis_validators_root.trim_start_matches("0x"))
                .map_err(|e| {
                    BeaconError::Sse(format!("hex decode genesis_validators_root: {e}"))
                })?;
            if gvr_bytes.len() != 32 {
                return Err(BeaconError::Sse(format!(
                    "genesis_validators_root length mismatch: expected 32 bytes, got {}",
                    gvr_bytes.len()
                )));
            }
            let gvr = tree_hash::Hash256::from_slice(&gvr_bytes);
            let cv: [u8; 4] = hex_array_4(&resp.data.fork.current_version)?;
            Ok(ForkDatas::new(
                ForkData {
                    current_version: cv,
                    genesis_validators_root: gvr,
                },
                ForkData {
                    current_version: cv,
                    genesis_validators_root: gvr,
                },
            ))
        })
    }

    fn get_known_validator(&self, pubkey: BlsPublicKey) -> BoxedFuture<KnownValidator> {
        let client = self.client.clone();
        let pk_hex = hex::encode(pubkey.serialize());
        let url = format!(
            "{}{}{}",
            self.url, "eth/v1/beacon/states/head/validators/0x", pk_hex
        );
        Box::pin(async move {
            let resp: ValidatorResponse = get_json(&client, &url).await?;
            Ok(KnownValidator {
                index: resp.data.index.parse::<u64>().map_err(|e| {
                    BeaconError::Sse(format!(
                        "invalid validator index '{}': {}",
                        resp.data.index, e
                    ))
                })?,
                balance: resp.data.balance.parse::<u64>().map_err(|e| {
                    BeaconError::Sse(format!("invalid balance '{}': {}", resp.data.balance, e))
                })?,
                status: resp.data.status,
            })
        })
    }

    fn publish_block(
        &self,
        _block: SignedBeaconBlockContent,
        _submission_type: SubmissionType,
    ) -> BoxedFuture<B256> {
        Box::pin(async move {
            Err(BeaconError::Api {
                status: 501,
                body: "publish_block not yet implemented".into(),
            })
        })
    }
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: &str,
) -> Result<T, BeaconError> {
    let resp = client.get(url).send().await.map_err(BeaconError::Http)?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(BeaconError::Api {
            status: status.as_u16(),
            body,
        });
    }
    resp.json().await.map_err(BeaconError::Deserialize)
}

fn hex_array_4<const N: usize>(s: &str) -> Result<[u8; N], BeaconError> {
    let bytes = hex::decode(s.trim_start_matches("0x"))
        .map_err(|e| BeaconError::Sse(format!("hex decode: {e}")))?;
    if bytes.len() != N {
        return Err(BeaconError::Sse(format!(
            "expected {N} bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[derive(Debug, Clone, Deserialize)]
struct SyncStatusResponse {
    data: SyncStatus,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
struct ProposerDutiesResponse {
    data: Vec<ProposerDutyData>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
struct ProposerDutyData {
    #[serde_as(as = "DisplayFromStr")]
    slot: u64,
    pubkey: BlsPublicKey,
    #[serde_as(as = "DisplayFromStr")]
    validator_index: u64,
}

impl From<ProposerDutyData> for ProposerDuty {
    fn from(d: ProposerDutyData) -> Self {
        Self {
            slot: d.slot,
            pubkey: d.pubkey,
            validator_index: d.validator_index,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GetForkResponse {
    data: ForkDataResp,
}

#[derive(Debug, Clone, Deserialize)]
struct ForkDataResp {
    genesis_validators_root: String,
    fork: CurrentFork,
}

#[derive(Debug, Clone, Deserialize)]
struct CurrentFork {
    current_version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ValidatorResponse {
    data: ValidatorData,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ValidatorData {
    index: String,
    balance: String,
    status: String,
    validator: ValidatorInner,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ValidatorInner {
    pubkey: String,
    withdrawal_credentials: String,
    effective_balance: String,
    slashed: bool,
    activation_eligibility_epoch: String,
    activation_epoch: String,
    exit_epoch: String,
    withdrawable_epoch: String,
}
