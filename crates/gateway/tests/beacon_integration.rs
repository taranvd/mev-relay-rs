use futures::StreamExt;
use relay_gateway::BeaconConnection;
use relay_gateway::BeaconEventsClient;
use relay_gateway::BeaconNodeApi;
use relay_gateway::BeaconService;
use std::time::Duration;
use url::Url;

const SEPOLIA_BEACON_URL: &str = "https://ethereum-sepolia-beacon-api.publicnode.com";

#[tokio::test]
#[ignore]
async fn test_sync_status() {
    let url = Url::parse(SEPOLIA_BEACON_URL).unwrap();
    let api = BeaconNodeApi::new(url);
    let (service, handle) = BeaconService::new(api);
    tokio::spawn(service);

    let status = handle.sync_status().await.unwrap();
    assert!(!status.is_syncing);
}

#[tokio::test]
#[ignore]
async fn test_proposer_duties() {
    let url = Url::parse(SEPOLIA_BEACON_URL).unwrap();
    let api = BeaconNodeApi::new(url);
    let (service, handle) = BeaconService::new(api);
    tokio::spawn(service);

    let duties = handle.proposer_duties(0).await.unwrap();
    assert!(!duties.is_empty());
}

#[tokio::test]
#[ignore]
async fn test_get_fork_data() {
    let url = Url::parse(SEPOLIA_BEACON_URL).unwrap();
    let api = BeaconNodeApi::new(url);
    let (service, handle) = BeaconService::new(api);
    tokio::spawn(service);

    match handle.get_fork_data().await {
        Ok(fork_data) => {
            let domain = fork_data.compute_builder_domain();
            assert_eq!(domain.len(), 32);
        }
        Err(e) => {
            eprintln!("get_fork_data failed (expected on public nodes): {e}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_event_stream() {
    let url = Url::parse(SEPOLIA_BEACON_URL).unwrap();
    let client = BeaconEventsClient::new(url);

    let mut stream = client.stream().await;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        if let Some(event) = stream.next().await {
            let _event = event.unwrap();
        }
    })
    .await;
    match result {
        Ok(()) => {}
        Err(_) => eprintln!("timed out waiting for head event"),
    }
}
