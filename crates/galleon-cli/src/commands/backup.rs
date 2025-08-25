use clap::Args; use galleon_common::error::GalleonResult; #[derive(Args)] pub struct BackupArgs; pub async fn handle_backup_command(args: BackupArgs) -> GalleonResult<()> { Ok(()) }
