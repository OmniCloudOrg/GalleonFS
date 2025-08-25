use clap::Args; use galleon_common::error::GalleonResult; #[derive(Args)] pub struct VolumeArgs; pub async fn handle_volume_command(args: VolumeArgs) -> GalleonResult<()> { Ok(()) }
