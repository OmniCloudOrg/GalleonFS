use crate::*;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct MigrationManager {
    storage_engine: Arc<dyn StorageEngine>,
    migrations: Arc<RwLock<HashMap<Uuid, Migration>>>,
}

impl MigrationManager {
    pub fn new(storage_engine: Arc<dyn StorageEngine>) -> Self {
        Self {
            storage_engine,
            migrations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_migration(&self, source_volume_id: Uuid, target_spec: MigrationTarget, strategy: MigrationStrategy) -> Result<Migration> {
        let migration = Migration {
            id: Uuid::new_v4(),
            name: format!("migration-{}", Uuid::new_v4()),
            source_volume_id,
            target_spec,
            strategy,
            state: MigrationState::Pending,
            progress_percent: 0,
            started_at: SystemTime::now(),
            estimated_completion: None,
        };

        let mut migrations = self.migrations.write().await;
        migrations.insert(migration.id, migration.clone());

        Ok(migration)
    }

    pub async fn start_migration(&self, migration_id: Uuid) -> Result<()> {
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            migration.state = MigrationState::Copying;
        }

        // Spawn background task to handle migration
        let storage_engine = self.storage_engine.clone();
        let migrations_ref = self.migrations.clone();
        
        tokio::spawn(async move {
            if let Err(e) = Self::execute_migration(storage_engine, migrations_ref.clone(), migration_id).await {
                tracing::error!("Migration failed: {}", e);
                // Update migration state to failed
                let mut migrations = migrations_ref.write().await;
                if let Some(migration) = migrations.get_mut(&migration_id) {
                    migration.state = MigrationState::Failed;
                }
            }
        });

        Ok(())
    }

    async fn execute_migration(
        storage_engine: Arc<dyn StorageEngine>,
        migrations: Arc<RwLock<HashMap<Uuid, Migration>>>,
        migration_id: Uuid
    ) -> Result<()> {
        // Get migration details
        let migration = {
            let migrations_guard = migrations.read().await;
            migrations_guard.get(&migration_id).cloned()
                .ok_or_else(|| anyhow::anyhow!("Migration not found"))?
        };

        let source_volume = storage_engine.get_volume(migration.source_volume_id).await?
            .ok_or_else(|| anyhow::anyhow!("Source volume not found"))?;

        // Create target volume
        let mut target_volume = Volume::new(
            format!("{}-migrated", source_volume.name),
            source_volume.volume_type,
            source_volume.size_bytes,
            migration.target_spec.storage_class.clone(),
        );

        storage_engine.create_volume(&mut target_volume).await?;

        // Update migration state
        {
            let mut migrations_guard = migrations.write().await;
            if let Some(mig) = migrations_guard.get_mut(&migration_id) {
                mig.state = MigrationState::Copying;
                mig.progress_percent = 10;
            }
        }

        match migration.strategy {
            MigrationStrategy::Offline => {
                Self::execute_offline_migration(&storage_engine, &migrations, migration_id, &source_volume, &target_volume).await?;
            }
            MigrationStrategy::Online => {
                Self::execute_online_migration(&storage_engine, &migrations, migration_id, &source_volume, &target_volume).await?;
            }
        }

        // Update migration state to completed
        {
            let mut migrations_guard = migrations.write().await;
            if let Some(mig) = migrations_guard.get_mut(&migration_id) {
                mig.state = MigrationState::Completed;
                mig.progress_percent = 100;
            }
        }

        Ok(())
    }

    async fn execute_offline_migration(
        storage_engine: &Arc<dyn StorageEngine>,
        migrations: &Arc<RwLock<HashMap<Uuid, Migration>>>,
        migration_id: Uuid,
        source_volume: &Volume,
        target_volume: &Volume
    ) -> Result<()> {
        // For offline migration, we can safely copy all data
        let block_size = 4096u64;
        let num_blocks = (source_volume.size_bytes + block_size - 1) / block_size;

        for block_id in 0..num_blocks {
            // Read from source
            if let Ok(data) = storage_engine.read_block(source_volume.id, block_id).await {
                // Write to target
                storage_engine.write_block(target_volume.id, block_id, &data).await?;
            }

            // Update progress
            let progress = ((block_id + 1) * 100 / num_blocks) as u8;
            {
                let mut migrations_guard = migrations.write().await;
                if let Some(mig) = migrations_guard.get_mut(&migration_id) {
                    mig.progress_percent = progress;
                }
            }
        }

        storage_engine.flush_volume(target_volume.id).await?;
        Ok(())
    }

    async fn execute_online_migration(
        storage_engine: &Arc<dyn StorageEngine>,
        migrations: &Arc<RwLock<HashMap<Uuid, Migration>>>,
        migration_id: Uuid,
        source_volume: &Volume,
        target_volume: &Volume
    ) -> Result<()> {
        // For online migration, we need to handle ongoing writes
        // This is a simplified implementation
        
        // Step 1: Initial full copy
        let block_size = 4096u64;
        let num_blocks = (source_volume.size_bytes + block_size - 1) / block_size;

        for block_id in 0..num_blocks {
            if let Ok(data) = storage_engine.read_block(source_volume.id, block_id).await {
                storage_engine.write_block(target_volume.id, block_id, &data).await?;
            }

            let progress = ((block_id + 1) * 80 / num_blocks) as u8; // 80% for initial copy
            {
                let mut migrations_guard = migrations.write().await;
                if let Some(mig) = migrations_guard.get_mut(&migration_id) {
                    mig.progress_percent = progress;
                    if progress >= 80 {
                        mig.state = MigrationState::Syncing;
                    }
                }
            }
        }

        // Step 2: Incremental sync (simplified - in reality this would track dirty blocks)
        {
            let mut migrations_guard = migrations.write().await;
            if let Some(mig) = migrations_guard.get_mut(&migration_id) {
                mig.state = MigrationState::ReadyForCutover;
                mig.progress_percent = 90;
            }
        }

        // Step 3: Final cutover (brief pause for final sync)
        {
            let mut migrations_guard = migrations.write().await;
            if let Some(mig) = migrations_guard.get_mut(&migration_id) {
                mig.state = MigrationState::CuttingOver;
                mig.progress_percent = 95;
            }
        }

        storage_engine.flush_volume(target_volume.id).await?;
        Ok(())
    }

    pub async fn get_migration(&self, migration_id: Uuid) -> Result<Option<Migration>> {
        let migrations = self.migrations.read().await;
        Ok(migrations.get(&migration_id).cloned())
    }

    pub async fn list_migrations(&self) -> Result<Vec<Migration>> {
        let migrations = self.migrations.read().await;
        Ok(migrations.values().cloned().collect())
    }

    pub async fn cancel_migration(&self, migration_id: Uuid) -> Result<()> {
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            if matches!(migration.state, MigrationState::Pending | MigrationState::Copying | MigrationState::Syncing) {
                migration.state = MigrationState::Failed;
            }
        }
        Ok(())
    }

    pub async fn approve_cutover(&self, migration_id: Uuid) -> Result<()> {
        let mut migrations = self.migrations.write().await;
        if let Some(migration) = migrations.get_mut(&migration_id) {
            if migration.state == MigrationState::ReadyForCutover {
                migration.state = MigrationState::CuttingOver;
                // In a real implementation, this would trigger the final cutover process
            }
        }
        Ok(())
    }
}