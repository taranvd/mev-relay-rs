use relay_crypto::{BlsPublicKey, BlsSignature, BlsSigner, ForkDatas};
use relay_datastore::{Auctioneer, Storage};
use relay_entity::B256;
use relay_usecase::{GetHeaderUseCase, UnblindBlockUseCase};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc::{Sender, channel};
use tonic::codegen::tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info};

use crate::proto;

const BUF_SIZE: usize = 16;

pub struct RetrieverServiceImpl<S: Storage, A: Auctioneer> {
    auctioneer: Arc<A>,
    get_header_usecase: Arc<GetHeaderUseCase<S, A>>,
    unblind_usecase: Arc<UnblindBlockUseCase<S>>,
}

impl<S: Storage + Clone, A: Auctioneer + Clone> RetrieverServiceImpl<S, A> {
    pub fn new(storage: S, auctioneer: A, signer: Arc<BlsSigner>) -> Self {
        let auctioneer = Arc::new(auctioneer);
        let get_header_usecase = Arc::new(GetHeaderUseCase::new(
            storage.clone(),
            (*auctioneer).clone(),
            signer,
        ));
        let unblind_usecase = Arc::new(UnblindBlockUseCase::new(storage, ForkDatas::default()));
        Self {
            auctioneer,
            get_header_usecase,
            unblind_usecase,
        }
    }

    async fn handle_request(
        get_header_uc: Arc<GetHeaderUseCase<S, A>>,
        auctioneer: Arc<A>,
        mut in_stream: impl Stream<Item = Result<proto::RetrieveRequest, Status>>
        + Send
        + Unpin
        + 'static,
        tx: Sender<Result<proto::RetrieveResponse, Status>>,
    ) {
        while let Some(msg_result) = in_stream.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(status) => {
                    error!("retriever: stream error, closing");
                    tx.send(Err(status)).await.ok();
                    break;
                }
            };

            let slot = msg.slot;
            let parent_hash = B256(alloy_primitives::B256::from_slice(&msg.parent_hash));
            let proposer_pubkey = match BlsPublicKey::deserialize(&msg.proposer_pubkey) {
                Ok(k) => k,
                Err(e) => {
                    error!(slot, "retriever: invalid proposer pubkey: {}", e);
                    tx.send(Err(Status::invalid_argument(e))).await.ok();
                    continue;
                }
            };

            info!(slot, "retriever: retrieving header");

            let versioned = match get_header_uc
                .execute(slot, parent_hash, proposer_pubkey)
                .await
            {
                Ok(v) => v,
                Err(err) => {
                    error!(slot, "retriever: get header failed: {}", err);
                    tx.send(Err(Status::internal(err.to_string()))).await.ok();
                    continue;
                }
            };

            let bid = auctioneer.get_best_bid(slot).await.ok();

            if bid.is_some() {
                info!(slot, "retriever: bid found, sending full response");
            } else {
                info!(slot, "retriever: bid not found, sending partial response");
            }

            let response = proto::RetrieveResponse {
                signed_header: serde_json::to_vec(&versioned).unwrap_or_default(),
                execution_payload: bid
                    .as_ref()
                    .map(|b| serde_json::to_vec(&*b.execution_payload()).unwrap_or_default())
                    .unwrap_or_default(),
                blobs_bundle: bid
                    .as_ref()
                    .map(|b| serde_json::to_vec(&*b.blobs_bundle).unwrap_or_default())
                    .unwrap_or_default(),
            };

            if tx.send(Ok(response)).await.is_err() {
                info!("retriever: receiver dropped, closing");
                break;
            }

            info!(slot, "retriever: response sent");
        }
    }
}

#[tonic::async_trait]
impl<S, A> proto::retriever_service_server::RetrieverService for RetrieverServiceImpl<S, A>
where
    S: Storage + Send + Sync + Clone + 'static,
    A: Auctioneer + Clone + 'static,
{
    type RetrieveStream =
        Pin<Box<dyn Stream<Item = Result<proto::RetrieveResponse, Status>> + Send>>;

    async fn retrieve(
        &self,
        request: Request<Streaming<proto::RetrieveRequest>>,
    ) -> Result<Response<Self::RetrieveStream>, Status> {
        info!("retriever: connected");

        let (tx, rx) = channel(BUF_SIZE);
        let get_header_uc = self.get_header_usecase.clone();
        let auctioneer = self.auctioneer.clone();
        let in_stream = request.into_inner();

        tokio::spawn(async move {
            Self::handle_request(get_header_uc, auctioneer, in_stream, tx).await;
            info!("retriever: disconnected");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn submit_blinded_block(
        &self,
        request: Request<proto::SubmitBlindedBlockRequest>,
    ) -> Result<Response<proto::BlindedBlockResponse>, Status> {
        let req = request.into_inner();

        let block_hash = B256(alloy_primitives::B256::from_slice(&req.block_hash));
        let signature = BlsSignature::deserialize(&req.signature)
            .map_err(|e| Status::invalid_argument(format!("invalid signature: {e}")))?;

        let resp = self
            .unblind_usecase
            .execute(req.slot, req.proposer_index, block_hash, signature)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(proto::BlindedBlockResponse {
            execution_payload: serde_json::to_vec(&*resp.execution_payload)
                .map_err(|e| Status::internal(format!("serialization error: {e}")))?,
            blobs_bundle: resp
                .blobs_bundle
                .map(|b| serde_json::to_vec(&*b).unwrap_or_default())
                .unwrap_or_default(),
            block_hash: resp.execution_payload.block_hash.0.to_vec(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_crypto::{BlsPublicKey, BlsSecretKey, BlsSignature, ForkDatas};
    use relay_datastore::{MemoryAuctioneer, MemoryStorage};
    use relay_entity::{
        Address, B256, BidSubmission, BidTrace, BlobsBundle, ExecutionPayload, HeadSlot,
        PayloadAttributes, ProposerDuty, U256, ValidatorRegistration,
    };
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_stream::wrappers::ReceiverStream;

    fn valid_pubkey() -> BlsPublicKey {
        let sk = blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[]).unwrap();
        BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap()
    }

    fn dummy_signer() -> Arc<BlsSigner> {
        Arc::new(BlsSigner::new(
            BlsSecretKey::random(),
            ForkDatas::default(),
            relay_crypto::ForkName::Deneb,
        ))
    }

    fn dummy_execution_payload(prev_randao: B256) -> Arc<ExecutionPayload> {
        Arc::new(ExecutionPayload {
            parent_hash: B256(alloy_primitives::B256::default()),
            fee_recipient: Address(alloy_primitives::Address::default()),
            state_root: B256(alloy_primitives::B256::default()),
            receipts_root: B256(alloy_primitives::B256::default()),
            logs_bloom: ssz_types::FixedVector::from(vec![0u8; 256]),
            prev_randao,
            block_number: 1,
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            timestamp: 0,
            extra_data: ssz_types::VariableList::from(vec![]),
            base_fee_per_gas: U256(alloy_primitives::U256::ZERO),
            block_hash: B256(alloy_primitives::B256::default()),
            transactions: ssz_types::VariableList::from(vec![]),
            withdrawals: ssz_types::VariableList::from(vec![]),
            blob_gas_used: 0,
            excess_blob_gas: 0,
        })
    }

    fn dummy_bid(slot: u64, prev_randao: B256) -> Arc<BidSubmission> {
        let sig = BlsSignature::deserialize(
            &blst::min_pk::SecretKey::key_gen(&[42u8; 32], &[])
                .unwrap()
                .sign(b"dummy", relay_crypto::DST, &[])
                .compress(),
        )
        .unwrap();
        Arc::new(BidSubmission::new(
            BidTrace {
                slot,
                parent_hash: B256(alloy_primitives::B256::default()),
                block_hash: B256(alloy_primitives::B256::default()),
                builder_pubkey: valid_pubkey(),
                proposer_pubkey: valid_pubkey(),
                proposer_fee_recipient: Address(alloy_primitives::Address::default()),
                gas_limit: 30_000_000,
                gas_used: 15_000_000,
                value: U256(alloy_primitives::U256::from(100)),
            },
            dummy_execution_payload(prev_randao),
            Arc::new(BlobsBundle::default()),
            sig,
        ))
    }

    async fn setup() -> (
        Arc<GetHeaderUseCase<MemoryStorage, MemoryAuctioneer>>,
        Arc<MemoryAuctioneer>,
    ) {
        let builder_pk = valid_pubkey();
        let proposer_pk = valid_pubkey();
        let prev_randao = B256(alloy_primitives::B256::default());

        let storage = MemoryStorage::new(vec![builder_pk]);
        storage.set_head_slot(HeadSlot(0));
        storage.set_payload_attributes(PayloadAttributes {
            timestamp: 0,
            prev_randao: prev_randao.0,
            suggested_fee_recipient: Address::default(),
        });
        storage.set_validator_registration(
            proposer_pk.clone(),
            ValidatorRegistration {
                fee_recipient: Address::default(),
                gas_limit: 30_000_000,
                timestamp: 1000,
                pubkey: proposer_pk.clone(),
            },
        );
        storage.set_proposer_duties(vec![ProposerDuty {
            pubkey: proposer_pk,
            validator_index: 1,
            slot: 1,
        }]);

        let auctioneer = Arc::new(MemoryAuctioneer::new(Duration::from_secs(60)));
        let bid = dummy_bid(1, prev_randao);
        auctioneer.compare_and_bid(1, bid).await.unwrap();

        let signer = dummy_signer();
        let get_header_uc = Arc::new(GetHeaderUseCase::new(
            storage,
            (*auctioneer).clone(),
            signer,
        ));
        (get_header_uc, auctioneer)
    }

    #[tokio::test]
    async fn test_retrieve_success() {
        let (get_header_uc, auctioneer) = setup().await;
        let proposer_pk = valid_pubkey();

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx
            .send(Ok(proto::RetrieveRequest {
                slot: 1,
                parent_hash: vec![0u8; 32],
                proposer_pubkey: proposer_pk.serialize().to_vec(),
            }))
            .await
            .unwrap();
        drop(in_tx);

        RetrieverServiceImpl::<MemoryStorage, MemoryAuctioneer>::handle_request(
            get_header_uc,
            auctioneer,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert!(!resp.signed_header.is_empty());
        assert!(!resp.execution_payload.is_empty());
        assert!(!resp.blobs_bundle.is_empty());
    }

    #[tokio::test]
    async fn test_retrieve_multiple_messages() {
        let (get_header_uc, auctioneer) = setup().await;
        let proposer_pk = valid_pubkey();

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        let msg = proto::RetrieveRequest {
            slot: 1,
            parent_hash: vec![0u8; 32],
            proposer_pubkey: proposer_pk.serialize().to_vec(),
        };
        in_tx.send(Ok(msg.clone())).await.unwrap();
        in_tx.send(Ok(msg)).await.unwrap();
        drop(in_tx);

        RetrieverServiceImpl::<MemoryStorage, MemoryAuctioneer>::handle_request(
            get_header_uc,
            auctioneer,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp1 = out_rx.recv().await.unwrap().unwrap();
        let resp2 = out_rx.recv().await.unwrap().unwrap();
        assert!(!resp1.signed_header.is_empty());
        assert!(!resp2.signed_header.is_empty());
        assert!(out_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_retrieve_error_on_invalid_pubkey() {
        let (get_header_uc, auctioneer) = setup().await;

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx
            .send(Ok(proto::RetrieveRequest {
                slot: 1,
                parent_hash: vec![0u8; 32],
                proposer_pubkey: vec![0u8; 16],
            }))
            .await
            .unwrap();
        drop(in_tx);

        RetrieverServiceImpl::<MemoryStorage, MemoryAuctioneer>::handle_request(
            get_header_uc,
            auctioneer,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap();
        assert!(resp.is_err());
    }
}
