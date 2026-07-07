use crate::proto;
use crate::proto::bidder_service_server::BidderService;

use relay_datastore::{Auctioneer, Storage};
use relay_entity::BidSubmission;
use relay_usecase::SubmitBidUseCase;

use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc::{Sender, channel};
use tonic::codegen::tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};
use tracing::{Instrument, error, info};

const BUF_SIZE: usize = 16;

pub struct BidderServiceImpl<S, A>
where
    S: Storage,
    A: Auctioneer,
{
    usecase: Arc<SubmitBidUseCase<S, A>>,
}

impl<S, A> BidderServiceImpl<S, A>
where
    S: Storage,
    A: Auctioneer,
{
    pub fn new(usecase: SubmitBidUseCase<S, A>) -> Self {
        Self {
            usecase: Arc::new(usecase),
        }
    }

    async fn handle_request(
        usecase: Arc<SubmitBidUseCase<S, A>>,
        mut in_stream: impl Stream<Item = Result<proto::BidRequest, Status>> + Send + Unpin + 'static,
        tx: Sender<Result<proto::BidResponse, Status>>,
    ) {
        while let Some(msg_result) = in_stream.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(status) => {
                    error!("bidder: stream error, closing");
                    tx.send(Err(status)).await.ok();
                    break;
                }
            };

            let submission = match BidSubmission::try_from(msg) {
                Ok(s) => s,
                Err(e) => {
                    error!("bidder: conversion error: {}", e);
                    let response = proto::BidResponse {
                        code: 1,
                        message: format!("conversion error: {}", e),
                    };
                    if tx.send(Ok(response)).await.is_err() {
                        break;
                    }
                    continue;
                }
            };

            info!(
                slot = submission.message.slot,
                value = %submission.message.value,
                "bidder: processing bid"
            );

            let result = usecase
                .execute(submission)
                .instrument(tracing::info_span!(
                    "execute_bid",
                    slot = tracing::field::Empty,
                    value = tracing::field::Empty,
                    err = tracing::field::Empty,
                ))
                .await;

            match result {
                Ok(()) => {
                    info!("bidder: bid accepted");
                    if tx
                        .send(Ok(proto::BidResponse {
                            code: 0,
                            message: "ok".into(),
                        }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    error!("bidder: bid rejected: {}", err);
                    if tx
                        .send(Ok(proto::BidResponse {
                            code: 1,
                            message: err.to_string(),
                        }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }
}

#[tonic::async_trait]
impl<S, A> BidderService for BidderServiceImpl<S, A>
where
    S: Storage + Send + Sync + 'static,
    A: Auctioneer + 'static,
{
    type BidStream = Pin<Box<dyn Stream<Item = Result<proto::BidResponse, Status>> + Send>>;

    async fn bid(
        &self,
        request: Request<Streaming<proto::BidRequest>>,
    ) -> Result<Response<Self::BidStream>, Status> {
        info!("bidder: connected");

        let (tx, rx) = channel(BUF_SIZE);
        let usecase = self.usecase.clone();
        let in_stream = request.into_inner();

        tokio::spawn(async move {
            Self::handle_request(usecase, in_stream, tx).await;
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use parking_lot::RwLock;
    use relay_crypto::{BlsPublicKey, DST, ForkDatas, SignedRoot};
    use relay_datastore::AuctioneerError;
    use relay_entity::{
        Address, B256, BidTrace, BlindedBlockResponse, HeadSlot, PayloadAttributes, ProposerDuty,
        ValidatorRegistration,
    };
    use std::collections::HashMap;
    use tokio_stream::wrappers::ReceiverStream;

    struct MockStorage {
        head_slot: RwLock<HeadSlot>,
        payload_attributes: RwLock<PayloadAttributes>,
        whitelist: RwLock<Vec<BlsPublicKey>>,
        duties: RwLock<Vec<ProposerDuty>>,
    }

    impl MockStorage {
        fn new(
            head_slot: u64,
            prev_randao: B256,
            whitelist: Vec<BlsPublicKey>,
            duties: Vec<ProposerDuty>,
        ) -> Self {
            Self {
                head_slot: RwLock::new(HeadSlot(head_slot)),
                payload_attributes: RwLock::new(PayloadAttributes {
                    timestamp: 0,
                    prev_randao: prev_randao.0,
                    suggested_fee_recipient: Address::default(),
                }),
                whitelist: RwLock::new(whitelist),
                duties: RwLock::new(duties),
            }
        }
    }

    impl Storage for MockStorage {
        fn is_whitelisted_builder(&self, key: &BlsPublicKey) -> bool {
            self.whitelist.read().contains(key)
        }
        fn read_head_slot(&self) -> HeadSlot {
            *self.head_slot.read()
        }
        fn read_payload_attributes(&self) -> PayloadAttributes {
            self.payload_attributes.read().clone()
        }
        fn find_duty_by_slot(&self, slot: u64) -> Option<ProposerDuty> {
            self.duties.read().iter().find(|d| d.slot == slot).cloned()
        }
        fn set_head_slot(&self, _slot: HeadSlot) {}
        fn set_payload_attributes(&self, _attrs: PayloadAttributes) {}
        fn set_validator_registration(&self, _key: BlsPublicKey, _reg: ValidatorRegistration) {}
        fn set_validator_registrations(&self, _regs: HashMap<BlsPublicKey, ValidatorRegistration>) {
        }
        fn read_validator_registration(
            &self,
            _key: &BlsPublicKey,
        ) -> Option<ValidatorRegistration> {
            None
        }
        fn empty_validator_regs(&self) -> bool {
            true
        }
        fn set_proposer_duties(&self, _duties: Vec<ProposerDuty>) {}
        fn read_proposer_duties(&self) -> Vec<ProposerDuty> {
            self.duties.read().clone()
        }
        fn set_blinded_block_response(&self, _proposer: BlsPublicKey, _resp: BlindedBlockResponse) {
        }
        fn read_blinded_block_response(
            &self,
            _proposer: &BlsPublicKey,
        ) -> Option<BlindedBlockResponse> {
            None
        }
        fn set_delivered_blocks(&self, _block_hash: B256) {}
    }

    struct MockAuctioneer;

    #[async_trait]
    impl Auctioneer for MockAuctioneer {
        async fn compare_and_bid(
            &self,
            _slot: u64,
            _bid: Arc<BidSubmission>,
        ) -> Result<(), AuctioneerError> {
            Ok(())
        }
        async fn get_best_bid(&self, _slot: u64) -> Result<Arc<BidSubmission>, AuctioneerError> {
            Err(AuctioneerError::BidNotFound(0))
        }
    }

    fn deterministic_key(ikm: &[u8; 32]) -> (blst::min_pk::SecretKey, BlsPublicKey) {
        let sk = blst::min_pk::SecretKey::key_gen(ikm, &[]).unwrap();
        let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
        (sk, pk)
    }

    fn create_signed_bid_request(
        slot: u64,
        value: u128,
        prev_randao: [u8; 32],
        builder_pk: &BlsPublicKey,
        proposer_pk: &BlsPublicKey,
        signing_sk: &blst::min_pk::SecretKey,
    ) -> proto::BidRequest {
        let entity_trace = BidTrace {
            slot,
            parent_hash: B256(alloy_primitives::B256::default()),
            block_hash: B256(alloy_primitives::B256::default()),
            builder_pubkey: builder_pk.clone(),
            proposer_pubkey: proposer_pk.clone(),
            proposer_fee_recipient: Address::default(),
            gas_limit: 30_000_000,
            gas_used: 15_000_000,
            value: relay_entity::U256(alloy_primitives::U256::from(value)),
        };

        let builder_domain = ForkDatas::default().compute_builder_domain();
        let signing_root = entity_trace.signing_root(builder_domain.into());
        let sig_bytes = signing_sk.sign(signing_root.as_ref(), DST, &[]).to_bytes();

        proto::BidRequest {
            bid_trace: Some(proto::BidTrace {
                slot,
                parent_hash: vec![0u8; 32],
                block_hash: vec![0u8; 32],
                builder_pubkey: builder_pk.serialize().to_vec(),
                proposer_pubkey: proposer_pk.serialize().to_vec(),
                proposer_fee_recipient: vec![0u8; 20],
                gas_limit: 30_000_000,
                gas_used: 15_000_000,
                value: value.to_string(),
            }),
            execution_payload: Some(proto::ExecutionPayload {
                parent_hash: vec![0u8; 32],
                state_root: vec![0u8; 32],
                receipts_root: vec![0u8; 32],
                logs_bloom: vec![0u8; 256],
                prev_randao: prev_randao.to_vec(),
                extra_data: vec![],
                base_fee_per_gas: vec![0u8; 32],
                fee_recipient: vec![0u8; 20],
                block_hash: vec![0u8; 32],
                transactions: vec![],
                withdrawals: vec![],
                block_number: slot,
                gas_limit: 30_000_000,
                gas_used: 15_000_000,
                timestamp: 0,
                blob_gas_used: 0,
                excess_blob_gas: 0,
            }),
            signature: sig_bytes.to_vec(),
            blobs_bundle: Some(proto::BlobsBundle {
                commitments: vec![],
                proofs: vec![],
                blobs: vec![],
            }),
        }
    }

    fn setup() -> (
        Arc<SubmitBidUseCase<MockStorage, MockAuctioneer>>,
        blst::min_pk::SecretKey,
        BlsPublicKey,
        BlsPublicKey,
    ) {
        let (builder_sk, builder_pk) = deterministic_key(&[42u8; 32]);
        let (_proposer_sk, proposer_pk) = deterministic_key(&[43u8; 32]);

        let storage = MockStorage::new(
            0,
            B256(alloy_primitives::B256::default()),
            vec![builder_pk.clone()],
            vec![ProposerDuty {
                pubkey: proposer_pk.clone(),
                validator_index: 0,
                slot: 1,
            }],
        );
        let auctioneer = MockAuctioneer;
        let fork_datas = ForkDatas::default();
        let usecase = Arc::new(SubmitBidUseCase::new(storage, auctioneer, fork_datas));
        (usecase, builder_sk, builder_pk, proposer_pk)
    }

    #[tokio::test]
    async fn test_handle_bid_success() {
        let (usecase, sk, builder_pk, proposer_pk) = setup();
        let req = create_signed_bid_request(1, 100, [0u8; 32], &builder_pk, &proposer_pk, &sk);

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 0);
        assert_eq!(resp.message, "ok");
    }

    #[tokio::test]
    async fn test_handle_bid_zero_bid() {
        let (usecase, sk, builder_pk, proposer_pk) = setup();
        let mut req = create_signed_bid_request(1, 100, [0u8; 32], &builder_pk, &proposer_pk, &sk);
        req.bid_trace.as_mut().unwrap().value = String::from("0");

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 1);
    }

    #[tokio::test]
    async fn test_handle_bid_unauthorized_builder() {
        let (usecase, _sk, _builder_pk, proposer_pk) = setup();
        let (other_sk, other_pk) = deterministic_key(&[99u8; 32]);
        let req = create_signed_bid_request(1, 100, [0u8; 32], &other_pk, &proposer_pk, &other_sk);

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 1);
    }

    #[tokio::test]
    async fn test_handle_bid_past_slot() {
        let (usecase, sk, builder_pk, proposer_pk) = setup();
        let req = create_signed_bid_request(5, 100, [0u8; 32], &builder_pk, &proposer_pk, &sk);

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 1);
    }

    #[tokio::test]
    async fn test_handle_bid_invalid_payload_attributes() {
        let (usecase, sk, builder_pk, proposer_pk) = setup();
        let req = create_signed_bid_request(1, 100, [3u8; 32], &builder_pk, &proposer_pk, &sk);

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 1);
    }

    #[tokio::test]
    async fn test_handle_bid_missing_fields() {
        let (usecase, _sk, _builder_pk, _proposer_pk) = setup();
        let req = proto::BidRequest {
            bid_trace: None,
            execution_payload: None,
            signature: vec![],
            blobs_bundle: None,
        };

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 1);
    }

    #[tokio::test]
    async fn test_handle_bid_invalid_signature() {
        let (usecase, sk, builder_pk, proposer_pk) = setup();
        let mut req = create_signed_bid_request(1, 100, [0u8; 32], &builder_pk, &proposer_pk, &sk);
        req.signature = vec![0u8; 10];

        let (in_tx, in_rx) = tokio::sync::mpsc::channel(16);
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(16);

        in_tx.send(Ok(req)).await.unwrap();
        drop(in_tx);

        BidderServiceImpl::<MockStorage, MockAuctioneer>::handle_request(
            usecase,
            ReceiverStream::new(in_rx),
            out_tx,
        )
        .await;

        let resp = out_rx.recv().await.unwrap().unwrap();
        assert_eq!(resp.code, 1);
    }
}
