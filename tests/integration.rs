use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::StreamExt;
use relay_api::proto::bidder_service_client::BidderServiceClient;
use relay_api::proto::retriever_service_client::RetrieverServiceClient;
use relay_api::proto::validator_service_client::ValidatorServiceClient;
use relay_app::{RelayConfig, run_inner};
use relay_crypto::{
    BlsPublicKey, BlsSecretKey, BlsSignature, DST, ForkDatas, ForkName, SignedRoot,
};
use relay_entity::{B256, BidTrace};
use tonic::Request;
use tonic::transport::Endpoint;
use tracing::{info, warn};

fn random_port() -> u16 {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn deterministic_key(ikm: &[u8; 32]) -> (blst::min_pk::SecretKey, BlsPublicKey) {
    let sk = blst::min_pk::SecretKey::key_gen(ikm, &[]).unwrap();
    let pk = BlsPublicKey::deserialize(&sk.sk_to_pk().compress()).unwrap();
    (sk, pk)
}

fn register_validator_payload(
    validator_key: &BlsPublicKey,
    signing_sk: &blst::min_pk::SecretKey,
) -> relay_api::proto::RegisterValidatorRequest {
    let registration = relay_entity::ValidatorRegistration {
        fee_recipient: relay_entity::Address::default(),
        gas_limit: 30_000_000,
        timestamp: 1000,
        pubkey: validator_key.clone(),
    };

    let domain = ForkDatas::default().compute_builder_domain();
    let signing_root = registration.signing_root(domain.into());
    let sig =
        BlsSignature::deserialize(&signing_sk.sign(signing_root.as_ref(), DST, &[]).to_bytes())
            .unwrap();

    relay_api::proto::RegisterValidatorRequest {
        registration: Some(relay_api::proto::SignedValidatorRegistration {
            message: Some(relay_api::proto::ValidatorRegistrationMessage {
                fee_recipient: registration.fee_recipient.0.to_vec(),
                gas_limit: registration.gas_limit,
                timestamp: registration.timestamp,
                pubkey: registration.pubkey.serialize().to_vec(),
            }),
            signature: sig.serialize().to_vec(),
        }),
    }
}

fn create_signed_bid_request(
    slot: u64,
    value: u128,
    builder_pk: &BlsPublicKey,
    proposer_pk: &BlsPublicKey,
    signing_sk: &blst::min_pk::SecretKey,
) -> relay_api::proto::BidRequest {
    let entity_trace = BidTrace {
        slot,
        parent_hash: B256(alloy_primitives::B256::default()),
        block_hash: B256(alloy_primitives::B256::default()),
        builder_pubkey: builder_pk.clone(),
        proposer_pubkey: proposer_pk.clone(),
        proposer_fee_recipient: relay_entity::Address::default(),
        gas_limit: 30_000_000,
        gas_used: 15_000_000,
        value: relay_entity::U256(alloy_primitives::U256::from(value)),
    };

    let builder_domain = ForkDatas::default().compute_builder_domain();
    let signing_root = entity_trace.signing_root(builder_domain.into());
    let sig_bytes = signing_sk.sign(signing_root.as_ref(), DST, &[]).to_bytes();

    relay_api::proto::BidRequest {
        bid_trace: Some(relay_api::proto::BidTrace {
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
        execution_payload: Some(relay_api::proto::ExecutionPayload {
            parent_hash: vec![0u8; 32],
            state_root: vec![0u8; 32],
            receipts_root: vec![0u8; 32],
            logs_bloom: vec![0u8; 256],
            prev_randao: vec![0u8; 32],
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
        blobs_bundle: Some(relay_api::proto::BlobsBundle {
            commitments: vec![],
            proofs: vec![],
            blobs: vec![],
        }),
    }
}

/// Mock beacon node with controlled head slot progression:
/// - First `/eth/v1/beacon/headers/head` returns slot 0 (avoids duty refresh before registration)
/// - Subsequent calls return slot 1 (triggers duty refresh after registration)
async fn start_mock_beacon(validator_pk: &BlsPublicKey) -> u16 {
    let pk_hex = validator_pk
        .serialize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let first_head = r#"{"data":{"header":{"message":{"slot":"0"}}}}"#;
    let later_head = r#"{"data":{"header":{"message":{"slot":"1"}}}}"#;
    let proposer_duties_body = format!(
        r#"{{"data":[{{"pubkey":"0x{}","validator_index":"1","slot":"2"}}]}}"#,
        pk_hex
    );
    let fork_body = r#"{
        "data": {
            "genesis_validators_root": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "fork": {
                "current_version": "0x00000000"
            }
        }
    }"#;

    let head_call_count = Arc::new(AtomicU64::new(0));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    info!(port, "mock beacon node listening");

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(x) => x,
                Err(_) => break,
            };

            let proposer_duties_body = proposer_duties_body.clone();
            let head_call_count = Arc::clone(&head_call_count);

            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                let mut reader = BufReader::new(&mut socket);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).await.ok();

                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();

                let mut header = String::new();
                loop {
                    header.clear();
                    match reader.read_line(&mut header).await {
                        Ok(0) | Err(_) => break,
                        _ => {}
                    }
                    if header.trim().is_empty() {
                        break;
                    }
                }

                let (status, body) = if path.starts_with("/eth/v1/validator/duties/proposer/") {
                    ("200 OK", proposer_duties_body.as_str())
                } else if path == "/eth/v1/beacon/headers/head" {
                    let count = head_call_count.fetch_add(1, Ordering::SeqCst);
                    if count == 0 {
                        ("200 OK", first_head)
                    } else {
                        ("200 OK", later_head)
                    }
                } else if path.starts_with("/eth/v2/debug/beacon/states/head") {
                    ("200 OK", fork_body)
                } else if path.starts_with("/eth/v1/events") {
                    ("200 OK", "data: {}\n\n")
                } else if path.starts_with("/eth/v1/node/syncing") {
                    ("200 OK", r#"{"data":{"is_syncing":false}}"#)
                } else {
                    warn!(%path, "mock beacon: unknown endpoint");
                    ("404 Not Found", "not found")
                };

                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.ok();
            });
        }
    });

    port
}

async fn wait_for_server(endpoint: &Endpoint, max_retries: u32) {
    for i in 0..max_retries {
        match BidderServiceClient::connect(endpoint.clone()).await {
            Ok(_) => {
                info!("gRPC server ready after {} retries", i + 1);
                return;
            }
            Err(e) => {
                if i == max_retries - 1 {
                    panic!("gRPC server not ready after {} retries: {}", max_retries, e);
                }
                warn!("waiting for gRPC server (attempt {})...", i + 1);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

#[tokio::test]
async fn test_relay_full_flow() {
    init_logging();

    let grpc_port = random_port();
    let http_port = random_port();
    let relayer_sk = BlsSecretKey::random();
    let (builder_sk, builder_pk) = deterministic_key(&[42u8; 32]);
    let (validator_sk, validator_pk) = deterministic_key(&[43u8; 32]);
    let proposer_pk = BlsPublicKey::deserialize(
        &blst::min_pk::SecretKey::key_gen(&[99u8; 32], &[])
            .unwrap()
            .sk_to_pk()
            .compress(),
    )
    .unwrap();

    // Start mock beacon node
    let beacon_port = start_mock_beacon(&validator_pk).await;

    let config = RelayConfig {
        grpc_port,
        http_port,
        beacon_url: format!("http://127.0.0.1:{}/", beacon_port),
        bls_secret_key: relayer_sk,
        chain: ForkName::Deneb,
        slots_per_epoch: 32,
        enabled_builders: vec![builder_pk.clone()],
        fork_datas: ForkDatas::default(),
        polling_interval_secs: 1,
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let relay_handle = tokio::spawn(async move {
        run_inner(config, async {
            shutdown_rx.await.ok();
        })
        .await;
    });

    let endpoint = Endpoint::from_shared(format!("http://127.0.0.1:{}", grpc_port)).unwrap();
    wait_for_server(&endpoint, 30).await;

    {
        let mut client = ValidatorServiceClient::connect(endpoint.clone())
            .await
            .unwrap();
        let req = register_validator_payload(&validator_pk, &validator_sk);
        let resp = client
            .register_validator(Request::new(req))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            resp.code, 0,
            "validator registration failed: {}",
            resp.message
        );
    }
    // Wait for polling to sync duties with our registered validator
    tokio::time::sleep(Duration::from_secs(1)).await;

    {
        let mut client = BidderServiceClient::connect(endpoint.clone())
            .await
            .unwrap();
        let req = create_signed_bid_request(2, 101, &builder_pk, &proposer_pk, &builder_sk);
        let mut stream = client
            .bid(Request::new(tokio_stream::iter(vec![req])))
            .await
            .unwrap()
            .into_inner();
        let resp = stream.next().await.unwrap().unwrap();
        assert_eq!(resp.code, 0, "bid failed: {}", resp.message);
    }
    {
        let mut client = RetrieverServiceClient::connect(endpoint.clone())
            .await
            .unwrap();
        let req = relay_api::proto::RetrieveRequest {
            slot: 2,
            parent_hash: vec![0u8; 32],
            proposer_pubkey: validator_pk.serialize().to_vec(),
        };
        let mut stream = client
            .retrieve(Request::new(tokio_stream::iter(vec![req])))
            .await
            .unwrap()
            .into_inner();
        let resp = stream.next().await.unwrap().unwrap();
        assert!(
            !resp.signed_header.is_empty(),
            "retrieve should return signed header"
        );
    }
    shutdown_tx.send(()).ok();
    relay_handle.await.unwrap();
}
