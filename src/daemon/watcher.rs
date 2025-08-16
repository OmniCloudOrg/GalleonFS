use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use uuid::Uuid;

pub struct VolumeWatcher {
    volume_id: Uuid,
    mount_path: PathBuf,
    _watcher: RecommendedWatcher,
}

impl VolumeWatcher {
    pub async fn new(
        volume_id: Uuid,
        mount_path: PathBuf,
        event_tx: mpsc::UnboundedSender<(Uuid, Event)>,
    ) -> Result<Self> {
        info!("Creating file system watcher for volume {} at path: {:?}", volume_id, mount_path);

        let mount_path_clone = mount_path.clone();
        let volume_id_clone = volume_id;

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    if let Err(e) = event_tx.send((volume_id_clone, event)) {
                        error!("Failed to send file system event: {}", e);
                    }
                }
                Err(e) => {
                    error!("File system watch error for volume {}: {}", volume_id_clone, e);
                }
            }
        })?;

        watcher.watch(&mount_path, RecursiveMode::Recursive)?;

        info!("File system watcher started for volume {} at path: {:?}", volume_id, mount_path);

        Ok(Self {
            volume_id,
            mount_path,
            _watcher: watcher,
        })
    }

    pub fn volume_id(&self) -> Uuid {
        self.volume_id
    }

    pub fn mount_path(&self) -> &PathBuf {
        &self.mount_path
    }
}

impl Drop for VolumeWatcher {
    fn drop(&mut self) {
        info!("Dropping file system watcher for volume {} at path: {:?}", self.volume_id, self.mount_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_volume_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let volume_id = Uuid::new_v4();

        let watcher = VolumeWatcher::new(volume_id, temp_dir.path().to_path_buf(), tx).await;
        assert!(watcher.is_ok());

        let watcher = watcher.unwrap();
        assert_eq!(watcher.volume_id(), volume_id);
        assert_eq!(watcher.mount_path(), temp_dir.path());

        // Test that the watcher actually detects file changes
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "Hello, World!").unwrap();

        // Give the watcher a moment to detect the change
        sleep(Duration::from_millis(100)).await;

        // We should receive at least one event
        let timeout = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
        assert!(timeout.is_ok(), "Should have received a file system event");

        if let Ok(Some((received_volume_id, _event))) = timeout {
            assert_eq!(received_volume_id, volume_id);
        }
    }

    #[tokio::test]
    async fn test_volume_watcher_multiple_events() {
        let temp_dir = TempDir::new().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let volume_id = Uuid::new_v4();

        let _watcher = VolumeWatcher::new(volume_id, temp_dir.path().to_path_buf(), tx).await.unwrap();

        // Create multiple files to generate multiple events
        for i in 0..3 {
            let test_file = temp_dir.path().join(format!("test_{}.txt", i));
            fs::write(&test_file, format!("Content {}", i)).unwrap();
            sleep(Duration::from_millis(50)).await;
        }

        // Collect events for a short period
        let mut event_count = 0;
        let timeout_duration = Duration::from_millis(1000);
        let start_time = tokio::time::Instant::now();

        while start_time.elapsed() < timeout_duration {
            match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Some((received_volume_id, _event))) => {
                    assert_eq!(received_volume_id, volume_id);
                    event_count += 1;
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // We should have received at least one event (possibly more due to OS-specific behavior)
        assert!(event_count > 0, "Should have received at least one file system event");
    }
}