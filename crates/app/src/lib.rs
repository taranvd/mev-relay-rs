pub mod cli;
pub mod relay_service;

pub use relay_service::RelayService;

use cli::CliArgs;
use relay_api::proto::bidder_service_server::BidderServiceServer;
use relay_api::proto::retriever_service_server::RetrieverServiceServer;
use relay_api::proto::validator_service_server::ValidatorServiceServer;
use relay_api::{
    bidder_service::BidderServiceImpl, retriever_service::RetrieverServiceImpl,
    validator_service::ValidatorServiceImpl,
};
use relay_crypto::{BlsSecretKey, BlsSigner, ForkDatas, ForkName};
use relay_datastore::{MemoryAuctioneer, MemoryStorage};
use relay_gateway::{BeaconConnection, BeaconEventsClient, BeaconNodeApi, BeaconService};
use relay_health::serve_health;
use relay_usecase::{RegisterValidatorUseCase, SubmitBidUseCase};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info};
use tree_hash::Hash256;
use url::Url;

pub struct RelayConfig {
    pub grpc_port: u16,
    pub http_port: u16,
    pub beacon_url: String,
    pub bls_secret_key: BlsSecretKey,
    pub chain: ForkName,
    pub slots_per_epoch: u64,
    pub enabled_builders: Vec<relay_crypto::BlsPublicKey>,
    pub fork_datas: ForkDatas,
}

impl TryFrom<CliArgs> for RelayConfig {
    type Error = String;

    fn try_from(args: CliArgs) -> Result<Self, Self::Error> {
        let sk_bytes = hex::decode(args.bls_secret_key.trim_start_matches("0x"))
            .map_err(|e| format!("invalid bls secret key hex: {e}"))?;
        let bls_secret_key =
            BlsSecretKey::deserialize(&sk_bytes).map_err(|e| format!("invalid bls key: {e}"))?;

        let chain = match args.chain.as_str() {
            "mainnet" | "sepolia" | "holesky" => ForkName::Deneb,
            other => return Err(format!("unknown chain: {other}")),
        };

        let gv_bytes = hex::decode(args.genesis_validators_root.trim_start_matches("0x"))
            .map_err(|e| format!("invalid genesis validators root: {e}"))?;
        if gv_bytes.len() != 32 {
            return Err("genesis validators root must be 32 bytes".into());
        }
        let genesis_validators_root = Hash256::from_slice(&gv_bytes);

        let genesis_fork_bytes = hex::decode(args.genesis_fork_version.trim_start_matches("0x"))
            .map_err(|e| format!("invalid genesis fork version: {e}"))?;
        let current_fork_bytes = hex::decode(args.current_fork_version.trim_start_matches("0x"))
            .map_err(|e| format!("invalid current fork version: {e}"))?;

        if genesis_fork_bytes.len() != 4 || current_fork_bytes.len() != 4 {
            return Err("fork versions must be 4 bytes".into());
        }

        let mut gv_arr = [0u8; 4];
        gv_arr.copy_from_slice(&genesis_fork_bytes);
        let mut cv_arr = [0u8; 4];
        cv_arr.copy_from_slice(&current_fork_bytes);

        let fork_datas =
            ForkDatas::from_genesis_and_current_version(gv_arr, cv_arr, genesis_validators_root);

        let enabled_builders: Vec<_> = args
            .enabled_builders
            .into_iter()
            .map(|hex_str| {
                let bytes = hex::decode(hex_str.trim_start_matches("0x"))
                    .map_err(|e| format!("invalid builder pubkey: {e}"))?;
                relay_crypto::BlsPublicKey::deserialize(&bytes)
                    .map_err(|e| format!("invalid builder pubkey: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            grpc_port: args.grpc_port,
            http_port: args.http_port,
            beacon_url: args.beacon_url,
            bls_secret_key,
            chain,
            slots_per_epoch: args.slots_per_epoch,
            enabled_builders,
            fork_datas,
        })
    }
}

pub async fn run(config: RelayConfig) {
    info!(target: "relay", "starting relay");

    let storage = Arc::new(MemoryStorage::new(config.enabled_builders));
    let auctioneer = Arc::new(MemoryAuctioneer::new(Duration::from_secs(60)));

    let beacon_url = Url::parse(&config.beacon_url).expect("invalid beacon url");
    let api = BeaconNodeApi::new(beacon_url.clone());
    let (beacon_service, beacon_handle) = BeaconService::new(api);
    tokio::spawn(beacon_service);

    let fork_datas = match beacon_handle.get_fork_data().await {
        Ok(fd) => {
            info!(target: "relay", "fork data fetched from beacon node");
            fd
        }
        Err(e) => {
            error!(target: "relay", "failed to fetch fork data from beacon node, using config fallback: {e}");
            config.fork_datas.clone()
        }
    };

    let signer = Arc::new(BlsSigner::new(
        config.bls_secret_key,
        fork_datas.clone(),
        config.chain,
    ));

    let submit_bid = SubmitBidUseCase::new(
        (*storage).clone(),
        (*auctioneer).clone(),
        fork_datas.clone(),
    );
    let register_validator = RegisterValidatorUseCase::new((*storage).clone(), fork_datas);

    let bidder = BidderServiceImpl::new(submit_bid);
    let retriever =
        RetrieverServiceImpl::new((*storage).clone(), (*auctioneer).clone(), signer.clone());
    let validator = ValidatorServiceImpl::new(register_validator);

    let (health_tx, health_rx) = tokio::sync::oneshot::channel::<()>();

    let grpc_addr = format!("0.0.0.0:{}", config.grpc_port).parse().unwrap();
    info!(target: "relay", port = config.grpc_port, "starting gRPC server");
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(BidderServiceServer::new(bidder))
            .add_service(RetrieverServiceServer::new(retriever))
            .add_service(ValidatorServiceServer::new(validator))
            .serve_with_shutdown(grpc_addr, async {
                health_rx.await.ok();
            })
            .await
            .ok();
    });

    info!(target: "relay", port = config.http_port, "starting health check");
    tokio::spawn(serve_health(config.http_port));

    let beacon_client = Arc::new(BeaconEventsClient::new(beacon_url));
    let event_stream = beacon_client.stream().await;

    let relay_service = RelayService::new(
        storage,
        beacon_handle,
        beacon_client,
        config.slots_per_epoch,
        event_stream,
    );
    tokio::spawn(relay_service);

    info!(target: "relay", "relay started, waiting for shutdown signal");
    signal::ctrl_c().await.expect("failed to listen for ctrl-c");
    info!(target: "relay", "shutting down");
    drop(health_tx);
}
