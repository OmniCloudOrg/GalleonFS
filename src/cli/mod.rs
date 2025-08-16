use clap::{Arg, Command};
use crate::daemon::{Daemon, DaemonClient, DaemonServer};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};


pub async fn init_cli() {
    let matches = Command::new("galleonfs")
        .version(env!("CARGO_PKG_VERSION"))
        .author("OmniCloud Community")
        .about("A basic CLI for GalleonFS")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
        )
        .subcommand(
            Command::new("init")
                .about("Initializes the GalleonFS daemon")
                .arg(
                    Arg::new("force")
                        .short('f')
                        .long("force")
                        .help("Force initialization even if already initialized")
                        .action(clap::ArgAction::SetTrue)
                )
        )
        .subcommand(
            Command::new("volume")
                .about("Manages virtual volumes")
                .subcommand(
                    Command::new("create")
                        .about("Creates a new virtual volume")
                        .arg(
                            Arg::new("name")
                                .help("Name of the new volume")
                                .required(true)
                                .index(1),
                        )
                        .arg(
                            Arg::new("path")
                                .short('p')
                                .long("path")
                                .value_name("PATH")
                                .help("Mount path for the volume")
                                .required(true)
                        )
                )
                .subcommand(
                    Command::new("list")
                        .about("Lists all virtual volumes")
                        .alias("ls")
                        .arg(
                            Arg::new("verbose")
                                .short('v')
                                .long("verbose")
                                .help("Show detailed information")
                        )
                )
                .subcommand(
                    Command::new("remove")
                        .about("Removes a virtual volume")
                        .alias("rm")
                        .arg(
                            Arg::new("name")
                                .help("Name of the volume to remove")
                                .required(true)
                                .index(1),
                        )
                        .arg(
                            Arg::new("force")
                                .short('f')
                                .long("force")
                                .help("Force removal without confirmation")
                                .action(clap::ArgAction::SetTrue)
                        )
                )
                .subcommand(
                    Command::new("cd")
                        .about("Changes the current directory to the mount point of the specified virtual volume (WINDOWS ONLY)")
                        .arg(
                            Arg::new("name")
                                .help("Name of the volume to change directory to")
                                .required(true)
                                .index(1),
                        )
                )
        )
        .get_matches();

    // if let Some(config) = matches.get_one::<String>("config") {
    //     println!("Using config file: {}", config);
    // } else {
    //     println!("No config file specified.");
    // }

    if let Some(("init", sub_matches)) = matches.subcommand() {
        let force = sub_matches.get_flag("force");
        
        // Initialize logging
        tracing_subscriber::fmt()
            .with_env_filter("galleonfs=info")
            .init();

        println!("🚀 Initializing GalleonFS daemon... Force: {}", force);

        // Check if daemon is already running
        let client = DaemonClient::default();
        if !force && client.is_daemon_running().await {
            eprintln!("❌ Daemon is already running! Use --force to restart or 'galleonfs volume' commands to interact with it.");
            return;
        }

        let mount_path = std::path::PathBuf::from("C:\\temp\\galleonfs_default");
        if !mount_path.exists() {
            if let Err(e) = std::fs::create_dir_all(&mount_path) {
                eprintln!("❌ Failed to create mount path: {:?}", e);
                return;
            }
        }

        // Start the daemon
        let daemon = Arc::new(Daemon::new());
        if let Err(e) = daemon.start().await {
            eprintln!("❌ Failed to start daemon: {}", e);
            return;
        }

        // Create default volume
        match daemon.create_volume("default".to_string(), mount_path).await {
            Ok(volume_id) => {
                println!("✅ Default volume created with ID: {}", volume_id);
                println!("📂 Monitoring path: C:\\temp\\galleonfs_default");
            }
            Err(e) => {
                eprintln!("⚠️  Failed to create default volume: {}", e);
            }
        }

        // Start the IPC server
        let mut server = DaemonServer::new(daemon.clone());
        println!("🌐 Starting IPC server on port 8847...");
        println!("✅ GalleonFS daemon initialized successfully!");
        println!();
        println!("🔄 File system events will be displayed below...");
        println!("💡 Use 'galleonfs volume' commands in another terminal to manage volumes");
        println!("Press Ctrl+C to exit.");
        println!();

        tokio::select! {
            result = server.start(8847) => {
                if let Err(e) = result {
                    eprintln!("❌ Server error: {}", e);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n🛑 Ctrl+C received, shutting down daemon...");
            }
        }

        if let Err(e) = server.shutdown().await {
            eprintln!("⚠️  Error stopping server: {}", e);
        }

        if let Err(e) = daemon.stop().await {
            eprintln!("⚠️  Error stopping daemon: {}", e);
        } else {
            println!("✅ GalleonFS daemon stopped successfully");
        }
    }

    if let Some(("volume", sub_m)) = matches.subcommand() {
        let client = DaemonClient::default();

        // Check if daemon is running
        if !client.is_daemon_running().await {
            eprintln!("❌ Daemon is not running! Start it with: galleonfs init");
            return;
        }

        match sub_m.subcommand() {
            Some(("create", create_m)) => {
                let name = create_m.get_one::<String>("name").unwrap();
                let path_str = create_m.get_one::<String>("path").unwrap();
                let path = PathBuf::from(path_str);

                println!("📁 Creating volume '{}' at path: {}", name, path.display());

                match client.create_volume(name.clone(), path).await {
                    Ok(volume_id) => {
                        println!("✅ Volume '{}' created successfully!", name);
                        println!("📁 Volume ID: {}", volume_id);
                        println!("📂 Mount path: {}", path_str);
                        println!("🔄 File system monitoring is now active for this volume");
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to create volume '{}': {}", name, e);
                    }
                }
            }
            Some(("list", list_m)) => {
                let verbose = list_m.get_flag("verbose");
                
                match client.list_volumes().await {
                    Ok(volumes) => {
                        if volumes.is_empty() {
                            println!("📂 No volumes found");
                        } else {
                            println!("📁 Active Volumes ({}):", volumes.len());
                            println!();

                            for volume in volumes {
                                if verbose {
                                    println!("{}", volume.format_summary());
                                    println!();
                                } else {
                                    println!("  • {} [{}] -> {}", 
                                        volume.name(), 
                                        volume.id(), 
                                        volume.get_mount_path_display()
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to list volumes: {}", e);
                    }
                }
            }
            Some(("remove", remove_m)) => {
                let name = remove_m.get_one::<String>("name").unwrap();
                let force = remove_m.get_flag("force");

                match client.get_volume_by_name(name.clone()).await {
                    Ok(Some(volume)) => {
                        let volume_id = volume.id();

                        if !force {
                            print!("⚠️  Are you sure you want to remove volume '{}'? [y/N]: ", name);
                            std::io::stdout().flush().unwrap();
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap();
                            if !input.trim().to_lowercase().starts_with('y') {
                                println!("Volume removal cancelled");
                                return;
                            }
                        }

                        match client.remove_volume(volume_id).await {
                            Ok(_) => {
                                println!("✅ Volume '{}' removed successfully", name);
                            }
                            Err(e) => {
                                eprintln!("❌ Failed to remove volume '{}': {}", name, e);
                            }
                        }
                    }
                    Ok(None) => {
                        eprintln!("❌ Volume '{}' not found", name);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to get volume '{}': {}", name, e);
                    }
                }
            }
            Some(("cd", cd_m)) => {
                let name = cd_m.get_one::<String>("name").unwrap();

                match client.get_volume_by_name(name.clone()).await {
                    Ok(Some(volume)) => {
                        let mount_path = volume.get_mount_path_display();
                        println!("📂 Volume '{}' mount path: {}", name, mount_path);

                        #[cfg(windows)]
                        {
                            println!("💡 To change directory on Windows, run:");
                            println!("   cd \"{}\"", mount_path);
                        }

                        #[cfg(not(windows))]
                        {
                            println!("💡 To change directory, run:");
                            println!("   cd \"{}\"", mount_path);
                        }
                    }
                    Ok(None) => {
                        eprintln!("❌ Volume '{}' not found", name);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to get volume '{}': {}", name, e);
                    }
                }
            }
            _ => {
                eprintln!("❌ Unknown volume subcommand");
            }
        }
    }
}