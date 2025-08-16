mod cli;
mod core;
mod daemon;
mod vfs;

#[tokio::main]
async fn main() {
    cli::init_cli().await;
}