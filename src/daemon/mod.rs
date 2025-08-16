use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::vfs::{VfsManager, VfsEvent};

pub mod volume;
pub mod watcher;
pub mod ipc;
pub mod server;
pub mod client;

pub use volume::Volume;
pub use watcher::VolumeWatcher;
pub use server::{DaemonServer, DEFAULT_DAEMON_PORT};
pub use client::DaemonClient;

#[derive(Clone)]
pub struct Daemon {
    volumes: Arc<Mutex<HashMap<Uuid, Volume>>>,
    volume_watchers: Arc<Mutex<HashMap<Uuid, VolumeWatcher>>>,
    event_tx: Arc<Mutex<Option<mpsc::UnboundedSender<(Uuid, Event)>>>>,
    vfs_manager: Arc<Mutex<Option<Arc<VfsManager>>>>,
    node_id: Uuid,
}

impl Daemon {
    pub fn new() -> Self {
        let node_id = Uuid::new_v4();
        info!("🚀 Creating new daemon with node ID: {}", node_id);
        
        Self {
            volumes: Arc::new(Mutex::new(HashMap::new())),
            volume_watchers: Arc::new(Mutex::new(HashMap::new())),
            event_tx: Arc::new(Mutex::new(None)),
            vfs_manager: Arc::new(Mutex::new(None)),
            node_id,
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.start_with_storage_path(std::path::PathBuf::from("./galleonfs_storage")).await
    }

    pub async fn start_with_storage_path(&self, storage_path: std::path::PathBuf) -> Result<()> {
        info!("🚀 Starting GalleonFS daemon with storage at: {:?}", storage_path);

        // Initialize VFS manager
        let vfs_manager = Arc::new(VfsManager::new(storage_path, self.node_id).await?);
        {
            let mut vfs_guard = self.vfs_manager.lock().await;
            *vfs_guard = Some(vfs_manager.clone());
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<(Uuid, Event)>();
        *self.event_tx.lock().await = Some(tx);

        let daemon_clone = self.clone();
        tokio::spawn(async move {
            while let Some((volume_id, event)) = rx.recv().await {
                daemon_clone.handle_file_event(volume_id, event).await;
            }
        });

        info!("✅ GalleonFS daemon started successfully with VFS integration");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Stopping GalleonFS daemon...");

        let mut watchers = self.volume_watchers.lock().await;
        for (volume_id, watcher) in watchers.drain() {
            info!("Stopping watcher for volume: {}", volume_id);
            drop(watcher);
        }

        *self.event_tx.lock().await = None;

        info!("GalleonFS daemon stopped successfully");
        Ok(())
    }

    pub async fn create_volume(&self, name: String, mount_path: PathBuf) -> Result<Uuid> {
        let volume = Volume::new(name.clone(), mount_path.clone())?;
        let volume_id = volume.id();

        info!("📁 Creating volume '{}' at path: {:?}", name, mount_path);

        // Create VFS volume if VFS manager is available
        let vfs_manager_guard = self.vfs_manager.lock().await;
        if let Some(ref vfs_manager) = *vfs_manager_guard {
            // Calculate volume size (default to 1GB if not specified)
            let volume_size = 1024 * 1024 * 1024; // 1GB default
            
            // Create VFS volume with intelligent sharding
            let vfs_volume = vfs_manager.create_volume(
                name.clone(),
                volume_size,
                3, // Default replication factor
                "encrypted-storage".to_string(), // Default storage class
            ).await?;

            info!("🗄️ VFS volume created: {} shards, {} blocks", 
                  vfs_volume.shard_count, vfs_volume.block_count);
        }
        drop(vfs_manager_guard);

        let mut volumes = self.volumes.lock().await;
        volumes.insert(volume_id, volume);

        if let Some(ref tx) = *self.event_tx.lock().await {
            let watcher = VolumeWatcher::new(volume_id, mount_path, tx.clone()).await?;
            let mut volume_watchers = self.volume_watchers.lock().await;
            volume_watchers.insert(volume_id, watcher);
        }

        info!("✅ Volume created successfully with ID: {}", volume_id);
        Ok(volume_id)
    }

    pub async fn add_volume(&self, mount_path: PathBuf) -> Result<Uuid> {
        let volume_name = format!("volume_{}", Uuid::new_v4().to_string()[..8].to_string());
        self.create_volume(volume_name, mount_path).await
    }

    pub async fn remove_volume(&self, volume_id: Uuid) -> Result<()> {
        info!("Removing volume: {}", volume_id);

        let mut volumes = self.volumes.lock().await;
        if let Some(volume) = volumes.remove(&volume_id) {
            info!("Removed volume: {}", volume.name());
        } else {
            warn!("Volume not found: {}", volume_id);
            return Err(anyhow::anyhow!("Volume not found: {}", volume_id));
        }

        let mut volume_watchers = self.volume_watchers.lock().await;
        if let Some(watcher) = volume_watchers.remove(&volume_id) {
            drop(watcher);
            info!("Stopped watcher for volume: {}", volume_id);
        }

        Ok(())
    }

    pub async fn list_volumes(&self) -> Vec<Volume> {
        let volumes = self.volumes.lock().await;
        volumes.values().cloned().collect()
    }

    pub async fn get_volume(&self, volume_id: Uuid) -> Option<Volume> {
        let volumes = self.volumes.lock().await;
        volumes.get(&volume_id).cloned()
    }

    pub async fn get_volume_by_name(&self, name: &str) -> Option<Volume> {
        let volumes = self.volumes.lock().await;
        volumes.values().find(|v| v.name() == name).cloned()
    }

    async fn handle_file_event(&self, volume_id: Uuid, event: Event) {
        let volume_name = {
            let volumes = self.volumes.lock().await;
            volumes.get(&volume_id).map(|v| v.name().to_string())
        };

        if let Some(volume_name) = volume_name {
            self.print_detailed_event_info(&volume_name, volume_id, &event).await;
        } else {
            error!("Received event for unknown volume: {}", volume_id);
        }
    }

    async fn print_detailed_event_info(&self, volume_name: &str, volume_id: Uuid, event: &Event) {
        println!("═══════════════════════════════════════════════════════════════");
        println!("📁 VOLUME EVENT DETECTED");
        println!("═══════════════════════════════════════════════════════════════");
        println!("Volume Name: {}", volume_name);
        println!("Volume ID: {}", volume_id);
        println!("Event Kind: {:?}", event.kind);
        println!("Event Paths: {:?}", event.paths);
        
        // Print event attributes if they exist
        println!("Event Attributes: {:?}", event.attrs);

        match &event.kind {
            notify::EventKind::Access(access_kind) => {
                println!("📖 File Access Event: {:?}", access_kind);
                match access_kind {
                    notify::event::AccessKind::Read => println!("   → File was read"),
                    notify::event::AccessKind::Open(_) => println!("   → File was opened"),
                    notify::event::AccessKind::Close(_) => println!("   → File was closed"),
                    notify::event::AccessKind::Any => println!("   → File access (generic)"),
                    _ => println!("   → Other access type"),
                }
            },
            notify::EventKind::Create(create_kind) => {
                println!("✨ File Creation Event: {:?}", create_kind);
                match create_kind {
                    notify::event::CreateKind::File => println!("   → New file created"),
                    notify::event::CreateKind::Folder => println!("   → New directory created"),
                    notify::event::CreateKind::Any => println!("   → Something was created"),
                    _ => println!("   → Other creation type"),
                }
            },
            notify::EventKind::Modify(modify_kind) => {
                println!("✏️  File Modification Event: {:?}", modify_kind);
                match modify_kind {
                    notify::event::ModifyKind::Data(_) => println!("   → File content was modified"),
                    notify::event::ModifyKind::Metadata(_) => println!("   → File metadata was modified"),
                    notify::event::ModifyKind::Name(_) => println!("   → File was renamed/moved"),
                    notify::event::ModifyKind::Any => println!("   → File was modified (generic)"),
                    _ => println!("   → Other modification type"),
                }
            },
            notify::EventKind::Remove(remove_kind) => {
                println!("🗑️  File Removal Event: {:?}", remove_kind);
                match remove_kind {
                    notify::event::RemoveKind::File => println!("   → File was deleted"),
                    notify::event::RemoveKind::Folder => println!("   → Directory was deleted"),
                    notify::event::RemoveKind::Any => println!("   → Something was removed"),
                    _ => println!("   → Other removal type"),
                }
            },
            notify::EventKind::Any => {
                println!("🔄 Generic File System Event");
            },
            _ => {
                println!("❓ Unknown Event Type: {:?}", event.kind);
            }
        }

        for path in &event.paths {
            if let Ok(metadata) = std::fs::metadata(path) {
                println!("───────────────────────────────────────────────────────────────");
                println!("📄 File Details for: {:?}", path);
                println!("   Size: {} bytes", metadata.len());
                println!("   Is Directory: {}", metadata.is_dir());
                println!("   Is File: {}", metadata.is_file());
                println!("   Read Only: {}", metadata.permissions().readonly());
                
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let datetime = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0);
                        if let Some(dt) = datetime {
                            println!("   Last Modified: {}", dt.format("%Y-%m-%d %H:%M:%S UTC"));
                        }
                    }
                }
            } else {
                println!("───────────────────────────────────────────────────────────────");
                println!("📄 File Details for: {:?}", path);
                println!("   ⚠️  Could not read file metadata (file may have been deleted)");
            }
        }

        println!("───────────────────────────────────────────────────────────────");
        println!("🕒 Event Timestamp: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
        println!("═══════════════════════════════════════════════════════════════\n");
    }
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}