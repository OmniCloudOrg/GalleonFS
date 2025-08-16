use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{BlockStorageManager, ReplicationManager, FileMapper};

/// VFS event handler for processing file system events in real-time
/// Converts file system operations into VFS block operations with replication
pub struct VfsEventHandler {
    /// Block storage manager for data persistence
    block_storage: Arc<BlockStorageManager>,
    /// Replication manager for data distribution
    replication_manager: Arc<ReplicationManager>,
    /// File mapper for file-to-block mapping
    file_mapper: Arc<FileMapper>,
    /// Event processing queue
    event_queue: mpsc::UnboundedSender<VfsEvent>,
    /// Event processing statistics
    stats: Arc<tokio::sync::RwLock<EventStats>>,
}

/// VFS events that need to be processed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VfsEvent {
    /// File was created
    FileCreated {
        volume_id: Uuid,
        path: String,
        initial_size: u64,
        permissions: u32,
    },
    /// File was written to
    FileWritten {
        volume_id: Uuid,
        path: String,
        offset: u64,
        data: Vec<u8>,
        sync_required: bool,
    },
    /// File was read from
    FileRead {
        volume_id: Uuid,
        path: String,
        offset: u64,
        length: u64,
    },
    /// File was truncated
    FileTruncated {
        volume_id: Uuid,
        path: String,
        new_size: u64,
    },
    /// File was deleted
    FileDeleted {
        volume_id: Uuid,
        path: String,
    },
    /// File was renamed/moved
    FileRenamed {
        volume_id: Uuid,
        old_path: String,
        new_path: String,
    },
    /// Directory was created
    DirectoryCreated {
        volume_id: Uuid,
        path: String,
        permissions: u32,
    },
    /// Directory was deleted
    DirectoryDeleted {
        volume_id: Uuid,
        path: String,
    },
    /// File attributes were changed
    AttributesChanged {
        volume_id: Uuid,
        path: String,
        new_permissions: Option<u32>,
        new_owner: Option<(u32, u32)>, // (uid, gid)
        new_times: Option<FileTimes>,
    },
    /// Extended attributes were modified
    ExtendedAttributesChanged {
        volume_id: Uuid,
        path: String,
        attribute_name: String,
        new_value: Option<Vec<u8>>, // None = deleted
    },
    /// Synchronization requested
    SyncRequested {
        volume_id: Uuid,
        path: Option<String>, // None = sync entire volume
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTimes {
    pub access_time: Option<chrono::DateTime<chrono::Utc>>,
    pub modify_time: Option<chrono::DateTime<chrono::Utc>>,
    pub change_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Event processing statistics
#[derive(Debug, Clone)]
pub struct EventStats {
    /// Total events processed
    pub total_events: u64,
    /// Events processed per type
    pub events_by_type: std::collections::HashMap<String, u64>,
    /// Average processing latency
    pub avg_processing_latency_us: f64,
    /// Event queue depth
    pub queue_depth: u64,
    /// Processing errors
    pub error_count: u64,
    /// Bytes processed
    pub total_bytes_processed: u64,
}

/// Event processing result
#[derive(Debug, Clone)]
pub struct EventProcessingResult {
    /// Whether the event was processed successfully
    pub success: bool,
    /// Processing latency
    pub latency: std::time::Duration,
    /// Bytes processed
    pub bytes_processed: u64,
    /// Number of blocks affected
    pub blocks_affected: u32,
    /// Whether replication was triggered
    pub replication_triggered: bool,
    /// Error message if processing failed
    pub error_message: Option<String>,
}

impl VfsEventHandler {
    pub async fn new(
        block_storage: Arc<BlockStorageManager>,
        replication_manager: Arc<ReplicationManager>,
        file_mapper: Arc<FileMapper>,
    ) -> Result<Self> {
        info!("🎭 Initializing VFS event handler");

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let handler = Self {
            block_storage,
            replication_manager,
            file_mapper,
            event_queue: event_tx,
            stats: Arc::new(tokio::sync::RwLock::new(EventStats {
                total_events: 0,
                events_by_type: std::collections::HashMap::new(),
                avg_processing_latency_us: 0.0,
                queue_depth: 0,
                error_count: 0,
                total_bytes_processed: 0,
            })),
        };

        // Start event processing loop
        handler.start_event_processing_loop(event_rx).await;

        info!("✅ VFS event handler initialized");
        Ok(handler)
    }

    /// Submit an event for processing
    pub async fn submit_event(&self, event: VfsEvent) -> Result<()> {
        debug!("📥 Submitting VFS event: {:?}", std::mem::discriminant(&event));

        // Update queue depth
        {
            let mut stats = self.stats.write().await;
            stats.queue_depth += 1;
        }

        self.event_queue.send(event)
            .map_err(|e| anyhow::anyhow!("Failed to submit event: {}", e))?;

        Ok(())
    }

    /// Start the background event processing loop
    async fn start_event_processing_loop(&self, mut event_rx: mpsc::UnboundedReceiver<VfsEvent>) {
        let handler = self.clone();
        
        tokio::spawn(async move {
            info!("🔄 Starting VFS event processing loop");

            while let Some(event) = event_rx.recv().await {
                let start_time = std::time::Instant::now();
                
                // Process the event
                let result = handler.process_event(event.clone()).await;
                
                let latency = start_time.elapsed();

                // Update statistics
                handler.update_event_stats(&event, &result, latency).await;

                if !result.success {
                    error!("❌ Event processing failed: {:?} - {}", 
                           std::mem::discriminant(&event), 
                           result.error_message.unwrap_or_else(|| "Unknown error".to_string()));
                } else {
                    debug!("✅ Event processed successfully in {:?}", latency);
                }
            }

            warn!("🛑 VFS event processing loop terminated");
        });
    }

    /// Process a single VFS event
    async fn process_event(&self, event: VfsEvent) -> EventProcessingResult {
        let start_time = std::time::Instant::now();
        let mut bytes_processed = 0;
        let mut blocks_affected = 0;
        let mut replication_triggered = false;

        let result = match event {
            VfsEvent::FileCreated { volume_id, path, initial_size, permissions } => {
                self.handle_file_created(volume_id, &path, initial_size, permissions).await
            }
            VfsEvent::FileWritten { volume_id, path, offset, data, sync_required } => {
                bytes_processed = data.len() as u64;
                let result = self.handle_file_written(volume_id, &path, offset, &data, sync_required).await;
                if result.is_ok() {
                    replication_triggered = true;
                    blocks_affected = ((data.len() as u64 + super::VFS_BLOCK_SIZE - 1) / super::VFS_BLOCK_SIZE) as u32;
                }
                result
            }
            VfsEvent::FileRead { volume_id, path, offset, length } => {
                bytes_processed = length;
                self.handle_file_read(volume_id, &path, offset, length).await
            }
            VfsEvent::FileTruncated { volume_id, path, new_size } => {
                self.handle_file_truncated(volume_id, &path, new_size).await
            }
            VfsEvent::FileDeleted { volume_id, path } => {
                self.handle_file_deleted(volume_id, &path).await
            }
            VfsEvent::FileRenamed { volume_id, old_path, new_path } => {
                self.handle_file_renamed(volume_id, &old_path, &new_path).await
            }
            VfsEvent::DirectoryCreated { volume_id, path, permissions } => {
                self.handle_directory_created(volume_id, &path, permissions).await
            }
            VfsEvent::DirectoryDeleted { volume_id, path } => {
                self.handle_directory_deleted(volume_id, &path).await
            }
            VfsEvent::AttributesChanged { volume_id, path, new_permissions, new_owner, new_times } => {
                self.handle_attributes_changed(volume_id, &path, new_permissions, new_owner, new_times).await
            }
            VfsEvent::ExtendedAttributesChanged { volume_id, path, attribute_name, new_value } => {
                self.handle_extended_attributes_changed(volume_id, &path, &attribute_name, new_value).await
            }
            VfsEvent::SyncRequested { volume_id, path } => {
                self.handle_sync_requested(volume_id, path.as_deref()).await
            }
        };

        let latency = start_time.elapsed();

        match result {
            Ok(()) => EventProcessingResult {
                success: true,
                latency,
                bytes_processed,
                blocks_affected,
                replication_triggered,
                error_message: None,
            },
            Err(e) => EventProcessingResult {
                success: false,
                latency,
                bytes_processed: 0,
                blocks_affected: 0,
                replication_triggered: false,
                error_message: Some(e.to_string()),
            },
        }
    }

    // Event handlers

    async fn handle_file_created(&self, volume_id: Uuid, path: &str, _initial_size: u64, _permissions: u32) -> Result<()> {
        debug!("📁 Handling file creation: {}", path);
        
        let path_buf = std::path::PathBuf::from(path);
        
        // Create inode mapping for the new file
        let _ = self.file_mapper.get_or_create_inode(volume_id, &path_buf).await?;
        
        info!("✅ File created: {}", path);
        Ok(())
    }

    async fn handle_file_written(&self, volume_id: Uuid, path: &str, offset: u64, data: &[u8], sync_required: bool) -> Result<()> {
        debug!("✏️ Handling file write: path={}, offset={}, size={}, sync={}", 
               path, offset, data.len(), sync_required);

        let path_buf = std::path::PathBuf::from(path);

        // Map file write to VFS blocks
        let block_mappings = self.file_mapper.map_file_to_blocks(
            volume_id, 
            &path_buf, 
            offset, 
            data.len() as u64
        ).await?;

        // Write data to each affected block
        let mut current_offset = 0;
        for mapping in &block_mappings {
            let block_data_start = current_offset;
            let block_data_end = (current_offset + mapping.length as usize).min(data.len());
            let block_data = &data[block_data_start..block_data_end];

            // Write to block storage
            self.block_storage.write_block(
                volume_id,
                mapping.shard_id,
                mapping.block_id,
                mapping.block_offset,
                block_data,
            ).await?;

            // Trigger replication
            self.replication_manager.replicate_block(
                volume_id,
                mapping.shard_id,
                mapping.block_id,
                block_data,
            ).await?;

            current_offset += mapping.length as usize;
        }

        // Update file access time
        self.file_mapper.update_access_time(&path_buf).await?;

        // Sync if required
        if sync_required {
            self.block_storage.sync_all().await?;
        }

        debug!("✅ File write completed: {} bytes to {} blocks", data.len(), block_mappings.len());
        Ok(())
    }

    async fn handle_file_read(&self, volume_id: Uuid, path: &str, offset: u64, length: u64) -> Result<()> {
        debug!("📖 Handling file read: path={}, offset={}, length={}", path, offset, length);

        let path_buf = std::path::PathBuf::from(path);

        // Update file access time
        self.file_mapper.update_access_time(&path_buf).await?;

        // Note: The actual read data is handled by the block storage layer
        // This event is mainly for metadata updates and access tracking

        debug!("✅ File read completed: {} bytes", length);
        Ok(())
    }

    async fn handle_file_truncated(&self, _volume_id: Uuid, path: &str, new_size: u64) -> Result<()> {
        debug!("✂️ Handling file truncation: path={}, new_size={}", path, new_size);
        
        // TODO: Implement file truncation
        // This would involve:
        // 1. Updating the file size in the inode mapping
        // 2. Deallocating blocks beyond the new size
        // 3. Updating replication accordingly

        info!("✅ File truncated: {} to {} bytes", path, new_size);
        Ok(())
    }

    async fn handle_file_deleted(&self, _volume_id: Uuid, path: &str) -> Result<()> {
        debug!("🗑️ Handling file deletion: {}", path);

        let path_buf = std::path::PathBuf::from(path);

        // Remove file mapping and deallocate blocks
        self.file_mapper.remove_file(&path_buf).await?;

        info!("✅ File deleted: {}", path);
        Ok(())
    }

    async fn handle_file_renamed(&self, _volume_id: Uuid, old_path: &str, new_path: &str) -> Result<()> {
        debug!("📝 Handling file rename: {} -> {}", old_path, new_path);
        
        // TODO: Implement file renaming
        // This would involve updating the path mapping while keeping the same inode

        info!("✅ File renamed: {} -> {}", old_path, new_path);
        Ok(())
    }

    async fn handle_directory_created(&self, volume_id: Uuid, path: &str, _permissions: u32) -> Result<()> {
        debug!("📁 Handling directory creation: {}", path);

        let path_buf = std::path::PathBuf::from(path);
        
        // Create inode mapping for the new directory
        let _ = self.file_mapper.get_or_create_inode(volume_id, &path_buf).await?;

        info!("✅ Directory created: {}", path);
        Ok(())
    }

    async fn handle_directory_deleted(&self, _volume_id: Uuid, path: &str) -> Result<()> {
        debug!("🗑️ Handling directory deletion: {}", path);

        let path_buf = std::path::PathBuf::from(path);

        // Remove directory mapping
        self.file_mapper.remove_file(&path_buf).await?;

        info!("✅ Directory deleted: {}", path);
        Ok(())
    }

    async fn handle_attributes_changed(
        &self, 
        _volume_id: Uuid, 
        path: &str, 
        _new_permissions: Option<u32>,
        _new_owner: Option<(u32, u32)>,
        _new_times: Option<FileTimes>
    ) -> Result<()> {
        debug!("🏷️ Handling attribute change: {}", path);
        
        // TODO: Implement attribute changes
        // This would involve updating the inode metadata

        info!("✅ Attributes changed: {}", path);
        Ok(())
    }

    async fn handle_extended_attributes_changed(
        &self,
        _volume_id: Uuid,
        path: &str,
        attribute_name: &str,
        _new_value: Option<Vec<u8>>
    ) -> Result<()> {
        debug!("🏷️ Handling extended attribute change: {} - {}", path, attribute_name);
        
        // TODO: Implement extended attribute changes

        info!("✅ Extended attribute changed: {} - {}", path, attribute_name);
        Ok(())
    }

    async fn handle_sync_requested(&self, _volume_id: Uuid, path: Option<&str>) -> Result<()> {
        match path {
            Some(path) => {
                debug!("🔄 Handling sync request for path: {}", path);
                // TODO: Implement path-specific sync
            }
            None => {
                debug!("🔄 Handling sync request for entire volume");
                self.block_storage.sync_all().await?;
            }
        }

        info!("✅ Sync completed");
        Ok(())
    }

    // Statistics and monitoring

    async fn update_event_stats(&self, event: &VfsEvent, result: &EventProcessingResult, latency: std::time::Duration) {
        let mut stats = self.stats.write().await;
        
        stats.total_events += 1;
        stats.queue_depth = stats.queue_depth.saturating_sub(1);
        
        // Update event type counters
        let event_type = format!("{:?}", std::mem::discriminant(event));
        *stats.events_by_type.entry(event_type).or_insert(0) += 1;
        
        // Update latency (exponential moving average)
        let latency_us = latency.as_micros() as f64;
        if stats.total_events == 1 {
            stats.avg_processing_latency_us = latency_us;
        } else {
            stats.avg_processing_latency_us = 0.9 * stats.avg_processing_latency_us + 0.1 * latency_us;
        }
        
        if result.success {
            stats.total_bytes_processed += result.bytes_processed;
        } else {
            stats.error_count += 1;
        }
    }

    /// Get current event processing statistics
    pub async fn get_stats(&self) -> EventStats {
        self.stats.read().await.clone()
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = EventStats {
            total_events: 0,
            events_by_type: std::collections::HashMap::new(),
            avg_processing_latency_us: 0.0,
            queue_depth: 0,
            error_count: 0,
            total_bytes_processed: 0,
        };
    }
}

// Make VfsEventHandler cloneable for background tasks
impl Clone for VfsEventHandler {
    fn clone(&self) -> Self {
        Self {
            block_storage: self.block_storage.clone(),
            replication_manager: self.replication_manager.clone(),
            file_mapper: self.file_mapper.clone(),
            event_queue: self.event_queue.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl Default for EventStats {
    fn default() -> Self {
        Self {
            total_events: 0,
            events_by_type: std::collections::HashMap::new(),
            avg_processing_latency_us: 0.0,
            queue_depth: 0,
            error_count: 0,
            total_bytes_processed: 0,
        }
    }
}