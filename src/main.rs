mod cli;
mod core;
mod daemon;

#[tokio::main]
async fn main() {
    cli::init_cli().await;
}