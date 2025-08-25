use clap::Args; use galleon_common::error::GalleonResult; #[derive(Args)] pub struct AdminArgs; pub async fn handle_admin_command(args: AdminArgs) -> GalleonResult<()> { Ok(()) }
