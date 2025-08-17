mod cli;
mod core;
mod daemon;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "galleonfs=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    cli::run_cli().await
}