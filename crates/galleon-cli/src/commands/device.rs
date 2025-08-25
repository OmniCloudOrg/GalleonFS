use clap::Args; use galleon_common::error::GalleonResult; #[derive(Args)] pub struct DeviceArgs; pub async fn handle_device_command(args: DeviceArgs) -> GalleonResult<()> { Ok(()) }
