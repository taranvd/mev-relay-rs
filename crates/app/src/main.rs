use clap::Parser;
use relay_app::RelayConfig;
use relay_app::cli::CliArgs;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(true).init();

    let args = CliArgs::parse();
    let config = RelayConfig::try_from(args).unwrap_or_else(|e| {
        eprintln!("config error: {e}");
        std::process::exit(1);
    });
    relay_app::run(config).await;
}
