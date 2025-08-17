use crate::core::types::volume::Volume;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

#[derive(Debug)]
pub struct FileWatcher {
    watched_volumes: Arc<RwLock<HashMap<String, PathBuf>>>,
    watchers: Arc<RwLock<HashMap<String, RecommendedWatcher>>>,
    event_sender: Option<mpsc::UnboundedSender<(String, Event)>>,
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watched_volumes: Arc::new(RwLock::new(HashMap::new())),
            watchers: Arc::new(RwLock::new(HashMap::new())),
            event_sender: None,
        }
    }

    pub async fn start_watching(&mut self, volumes: Vec<Volume>) {
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, Event)>();
        self.event_sender = Some(tx);

        for volume in volumes {
            if let Some(mount_point) = &volume.mount_point {
                if volume.is_mounted {
                    if let Err(e) = self.watch_directory(&volume.name, mount_point).await {
                        error!("Failed to start watching volume '{}': {}", volume.name, e);
                    }
                }
            }
        }

        tokio::spawn(async move {
            while let Some((volume_name, event)) = rx.recv().await {
                Self::handle_file_event(&volume_name, event).await;
            }
        });
    }

    pub async fn add_volume(&self, volume: &Volume) {
        if let Some(mount_point) = &volume.mount_point {
            if volume.is_mounted {
                if let Err(e) = self.watch_directory(&volume.name, mount_point).await {
                    error!("Failed to add volume '{}' to watcher: {}", volume.name, e);
                } else {
                    info!("Added volume '{}' to file watcher at path '{}'", volume.name, mount_point.display());
                }
            }
        }
    }

    pub async fn remove_volume(&self, volume_name: &str) {
        let mut watchers = self.watchers.write().await;
        let mut watched = self.watched_volumes.write().await;
        
        if watchers.remove(volume_name).is_some() {
            watched.remove(volume_name);
            info!("Removed volume '{}' from file watcher", volume_name);
        }
    }

    async fn watch_directory(&self, volume_name: &str, mount_point: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !mount_point.exists() {
            return Err(format!("Mount point '{}' does not exist", mount_point.display()).into());
        }

        let volume_name_clone = volume_name.to_string();
        let event_sender = self.event_sender.as_ref()
            .ok_or("Event sender not initialized")?
            .clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        if let Err(e) = event_sender.send((volume_name_clone.clone(), event)) {
                            error!("Failed to send file event: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("File watcher error: {}", e);
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(mount_point, RecursiveMode::Recursive)?;

        let mut watchers = self.watchers.write().await;
        let mut watched = self.watched_volumes.write().await;
        
        watchers.insert(volume_name.to_string(), watcher);
        watched.insert(volume_name.to_string(), mount_point.clone());
        
        info!("Started watching volume '{}' at path '{}'", volume_name, mount_point.display());
        Ok(())
    }

    async fn handle_file_event(volume_name: &str, event: Event) {
        use notify::EventKind;
        
        match event.kind {
            EventKind::Create(_) => {
                for path in &event.paths {
                    println!("📁 File created in volume '{}': {}", volume_name, path.display());
                }
                info!("File creation detected in volume '{}'", volume_name);
            }
            EventKind::Modify(_) => {
                for path in &event.paths {
                    println!("📁 File modified in volume '{}': {}", volume_name, path.display());
                }
                info!("File modification detected in volume '{}'", volume_name);
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    println!("📁 File removed in volume '{}': {}", volume_name, path.display());
                }
                info!("File removal detected in volume '{}'", volume_name);
            }
            EventKind::Any => {
                for path in &event.paths {
                    println!("📁 File system event in volume '{}': {}", volume_name, path.display());
                }
                info!("File system event detected in volume '{}'", volume_name);
            }
            _ => {
                // Handle other event types if needed
                for path in &event.paths {
                    println!("📁 File system event ({:?}) in volume '{}': {}", event.kind, volume_name, path.display());
                }
            }
        }
    }
}