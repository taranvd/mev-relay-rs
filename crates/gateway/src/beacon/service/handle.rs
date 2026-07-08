use super::BeaconCommands;
use crate::beacon::api::{KnownValidator, SignedBeaconBlockContent, SubmissionType};
use crate::beacon::error::BeaconError;
use alloy_primitives::B256;
use relay_crypto::{BlsPublicKey, ForkDatas};
use relay_entity::{ProposerDuty, SyncStatus};

#[derive(Debug, Clone)]
pub struct BeaconHandle {
    pub(crate) to_service: tokio::sync::mpsc::UnboundedSender<BeaconCommands>,
}

impl BeaconHandle {
    pub async fn sync_status(&self) -> Result<SyncStatus, BeaconError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.to_service
            .send(BeaconCommands::SyncStatus(tx))
            .map_err(|e| BeaconError::Channel(e.to_string()))?;
        rx.await
            .map_err(|e| BeaconError::Channel(e.to_string()))?
            .await
    }

    pub async fn get_fork_data(&self) -> Result<ForkDatas, BeaconError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.to_service
            .send(BeaconCommands::ForkData(tx))
            .map_err(|e| BeaconError::Channel(e.to_string()))?;
        rx.await
            .map_err(|e| BeaconError::Channel(e.to_string()))?
            .await
    }

    pub async fn get_known_validator(
        &self,
        pubkey: BlsPublicKey,
    ) -> Result<KnownValidator, BeaconError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.to_service
            .send(BeaconCommands::KnownValidator(pubkey, tx))
            .map_err(|e| BeaconError::Channel(e.to_string()))?;
        rx.await
            .map_err(|e| BeaconError::Channel(e.to_string()))?
            .await
    }

    pub async fn proposer_duties(&self, epoch: u64) -> Result<Vec<ProposerDuty>, BeaconError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.to_service
            .send(BeaconCommands::ProposerDuties(epoch, tx))
            .map_err(|e| BeaconError::Channel(e.to_string()))?;
        rx.await
            .map_err(|e| BeaconError::Channel(e.to_string()))?
            .await
    }

    pub async fn publish_block(
        &self,
        block: SignedBeaconBlockContent,
        submission_type: SubmissionType,
    ) -> Result<B256, BeaconError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.to_service
            .send(BeaconCommands::PublishBlock(
                Box::new(block),
                submission_type,
                tx,
            ))
            .map_err(|e| BeaconError::Channel(e.to_string()))?;
        rx.await
            .map_err(|e| BeaconError::Channel(e.to_string()))?
            .await
    }
}
