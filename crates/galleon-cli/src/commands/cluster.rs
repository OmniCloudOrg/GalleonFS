use clap::Args; use galleon_common::error::GalleonResult; #[derive(Args)] pub struct ClusterArgs; pub async fn handle_cluster_command(args: ClusterArgs) -> GalleonResult<()> { Ok(()) }
