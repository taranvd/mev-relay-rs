use clap::Parser;

#[derive(Parser, Debug)]
pub struct CliArgs {
    #[arg(long = "grpc.port", default_value = "50051")]
    pub grpc_port: u16,

    #[arg(long = "http.port", default_value = "9063")]
    pub http_port: u16,

    #[arg(long = "beacon.url", default_value = "http://127.0.0.1:3500")]
    pub beacon_url: String,

    #[arg(long = "bls-secret-key")]
    pub bls_secret_key: String,

    #[arg(long = "chain", default_value = "mainnet")]
    pub chain: String,

    #[arg(long = "epoch.slots", default_value_t = 32)]
    pub slots_per_epoch: u64,

    #[arg(long = "builders.enabled")]
    pub enabled_builders: Vec<String>,

    #[arg(long = "fork-data.genesis-version", default_value = "0x00000000")]
    pub genesis_fork_version: String,

    #[arg(long = "fork-data.current-version", default_value = "0x20000093")]
    pub current_fork_version: String,

    #[arg(
        long = "fork-data.genesis-validators-root",
        default_value = "0x4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95"
    )]
    pub genesis_validators_root: String,
}
