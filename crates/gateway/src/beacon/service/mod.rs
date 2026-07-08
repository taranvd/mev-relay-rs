use self::handle::BeaconHandle;
use super::api::{
    BeaconApi, BoxedFuture, KnownValidator, SignedBeaconBlockContent, SubmissionType,
};
use alloy_primitives::B256;
use futures::StreamExt;
use relay_crypto::{BlsPublicKey, ForkDatas};
use relay_entity::{ProposerDuty, SyncStatus};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::info;

pub mod handle;

pub enum BeaconCommands {
    SyncStatus(oneshot::Sender<BoxedFuture<SyncStatus>>),
    ForkData(oneshot::Sender<BoxedFuture<ForkDatas>>),
    ProposerDuties(u64, oneshot::Sender<BoxedFuture<Vec<ProposerDuty>>>),
    KnownValidator(BlsPublicKey, oneshot::Sender<BoxedFuture<KnownValidator>>),
    PublishBlock(
        Box<SignedBeaconBlockContent>,
        SubmissionType,
        oneshot::Sender<BoxedFuture<B256>>,
    ),
}

pub struct BeaconService<API: BeaconApi> {
    api: Arc<API>,
    command_rx: UnboundedReceiverStream<BeaconCommands>,
}

impl<API: BeaconApi + 'static> BeaconService<API> {
    pub fn new(api: API) -> (Self, BeaconHandle) {
        let api = Arc::new(api);
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = BeaconHandle { to_service: tx };
        let service = Self {
            api,
            command_rx: UnboundedReceiverStream::new(rx),
        };
        info!(target: "beacon_service", "beacon service started");
        (service, handle)
    }

    fn sync_status(&self) -> BoxedFuture<SyncStatus> {
        let api = self.api.clone();
        Box::pin(async move { api.sync_status().await })
    }

    fn get_fork_data(&self) -> BoxedFuture<ForkDatas> {
        let api = self.api.clone();
        Box::pin(async move { api.get_fork_data().await })
    }

    fn proposer_duties(&self, epoch: u64) -> BoxedFuture<Vec<ProposerDuty>> {
        let api = self.api.clone();
        Box::pin(async move { api.proposer_duties(epoch).await })
    }

    fn get_known_validator(&self, pubkey: BlsPublicKey) -> BoxedFuture<KnownValidator> {
        let api = self.api.clone();
        Box::pin(async move { api.get_known_validator(pubkey).await })
    }

    fn publish_block(
        &self,
        block: SignedBeaconBlockContent,
        submission_type: SubmissionType,
    ) -> BoxedFuture<B256> {
        let api = self.api.clone();
        Box::pin(async move { api.publish_block(block, submission_type).await })
    }
}

impl<API: BeaconApi + 'static> Future for BeaconService<API> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while let Poll::Ready(Some(command)) = self.command_rx.poll_next_unpin(cx) {
            match command {
                BeaconCommands::SyncStatus(tx) => {
                    let fut = self.sync_status();
                    let _ = tx.send(fut);
                }
                BeaconCommands::ForkData(tx) => {
                    let fut = self.get_fork_data();
                    let _ = tx.send(fut);
                }
                BeaconCommands::ProposerDuties(epoch, tx) => {
                    let fut = self.proposer_duties(epoch);
                    let _ = tx.send(fut);
                }
                BeaconCommands::KnownValidator(pubkey, tx) => {
                    let fut = self.get_known_validator(pubkey);
                    let _ = tx.send(fut);
                }
                BeaconCommands::PublishBlock(block, submission_type, tx) => {
                    let fut = self.publish_block(*block, submission_type);
                    let _ = tx.send(fut);
                }
            }
        }
        Poll::Pending
    }
}
