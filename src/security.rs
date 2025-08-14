use crate::*;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct SecurityManager {
    access_policies: Arc<RwLock<HashMap<String, VolumeAccessPolicy>>>,
    encryption_configs: Arc<RwLock<HashMap<Uuid, EncryptionConfig>>>,
    audit_log: Arc<RwLock<Vec<AuditLogEntry>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub timestamp: SystemTime,
    pub user: String,
    pub action: String,
    pub resource: String,
    pub result: AuditResult,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failure,
    Denied,
}

pub struct KeyManagementService {
    keys: Arc<RwLock<HashMap<Uuid, EncryptionKey>>>,
}

#[derive(Debug, Clone)]
pub struct EncryptionKey {
    pub id: Uuid,
    pub volume_id: Uuid,
    pub algorithm: String,
    pub key_data: Vec<u8>,
    pub created_at: SystemTime,
    pub last_rotated: SystemTime,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self {
            access_policies: Arc::new(RwLock::new(HashMap::new())),
            encryption_configs: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_access_policy(&self, policy: VolumeAccessPolicy) -> Result<()> {
        let mut policies = self.access_policies.write().await;
        policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    pub async fn remove_access_policy(&self, policy_name: &str) -> Result<()> {
        let mut policies = self.access_policies.write().await;
        policies.remove(policy_name);
        Ok(())
    }

    pub async fn check_access(&self, user: &str, groups: &[String], volume_labels: &HashMap<String, String>, operation: &str) -> Result<bool> {
        let policies = self.access_policies.read().await;
        
        for policy in policies.values() {
            if self.matches_volume_selector(&policy.volume_selector, volume_labels) {
                // Check if user is allowed
                if policy.allowed_users.contains(&user.to_string()) {
                    if policy.operations.contains(&operation.to_string()) {
                        self.log_audit_event(user, operation, "volume", AuditResult::Success, HashMap::new()).await?;
                        return Ok(true);
                    }
                }
                
                // Check if user's groups are allowed
                for group in groups {
                    if policy.allowed_groups.contains(group) {
                        if policy.operations.contains(&operation.to_string()) {
                            self.log_audit_event(user, operation, "volume", AuditResult::Success, HashMap::new()).await?;
                            return Ok(true);
                        }
                    }
                }
            }
        }
        
        self.log_audit_event(user, operation, "volume", AuditResult::Denied, HashMap::new()).await?;
        Ok(false)
    }

    pub async fn set_volume_encryption(&self, volume_id: Uuid, config: EncryptionConfig) -> Result<()> {
        let mut configs = self.encryption_configs.write().await;
        configs.insert(volume_id, config);
        Ok(())
    }

    pub async fn get_volume_encryption(&self, volume_id: Uuid) -> Result<Option<EncryptionConfig>> {
        let configs = self.encryption_configs.read().await;
        Ok(configs.get(&volume_id).cloned())
    }

    pub async fn is_volume_encrypted(&self, volume_id: Uuid) -> Result<bool> {
        let configs = self.encryption_configs.read().await;
        Ok(configs.get(&volume_id).map(|c| c.enabled).unwrap_or(false))
    }

    pub async fn log_audit_event(
        &self,
        user: &str,
        action: &str,
        resource: &str,
        result: AuditResult,
        details: HashMap<String, String>
    ) -> Result<()> {
        let entry = AuditLogEntry {
            id: Uuid::new_v4(),
            timestamp: SystemTime::now(),
            user: user.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            result,
            details,
        };

        let mut audit_log = self.audit_log.write().await;
        audit_log.push(entry);

        // In a real implementation, this would also write to persistent storage
        // and potentially forward to external SIEM systems

        Ok(())
    }

    pub async fn get_audit_logs(&self, filter: Option<AuditLogFilter>) -> Result<Vec<AuditLogEntry>> {
        let audit_log = self.audit_log.read().await;
        
        if let Some(filter) = filter {
            Ok(audit_log.iter()
                .filter(|entry| self.matches_audit_filter(entry, &filter))
                .cloned()
                .collect())
        } else {
            Ok(audit_log.clone())
        }
    }

    fn matches_volume_selector(&self, selector: &LabelSelector, labels: &HashMap<String, String>) -> bool {
        for (key, value) in &selector.match_labels {
            if let Some(label_value) = labels.get(key) {
                if label_value != value {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    fn matches_audit_filter(&self, entry: &AuditLogEntry, filter: &AuditLogFilter) -> bool {
        if let Some(user) = &filter.user {
            if &entry.user != user {
                return false;
            }
        }
        
        if let Some(action) = &filter.action {
            if &entry.action != action {
                return false;
            }
        }
        
        if let Some(result) = &filter.result {
            if !matches!((&entry.result, result), 
                (AuditResult::Success, AuditResult::Success) |
                (AuditResult::Failure, AuditResult::Failure) |
                (AuditResult::Denied, AuditResult::Denied)) {
                return false;
            }
        }
        
        if let Some(start_time) = filter.start_time {
            if entry.timestamp < start_time {
                return false;
            }
        }
        
        if let Some(end_time) = filter.end_time {
            if entry.timestamp > end_time {
                return false;
            }
        }
        
        true
    }
}

#[derive(Debug, Clone)]
pub struct AuditLogFilter {
    pub user: Option<String>,
    pub action: Option<String>,
    pub result: Option<AuditResult>,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
}

impl KeyManagementService {
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn generate_key(&self, volume_id: Uuid, algorithm: &str) -> Result<Uuid> {
        let key = EncryptionKey {
            id: Uuid::new_v4(),
            volume_id,
            algorithm: algorithm.to_string(),
            key_data: self.generate_key_data(algorithm)?,
            created_at: SystemTime::now(),
            last_rotated: SystemTime::now(),
        };

        let key_id = key.id;
        let mut keys = self.keys.write().await;
        keys.insert(key_id, key);

        Ok(key_id)
    }

    pub async fn rotate_key(&self, key_id: Uuid) -> Result<()> {
        let mut keys = self.keys.write().await;
        if let Some(key) = keys.get_mut(&key_id) {
            key.key_data = self.generate_key_data(&key.algorithm)?;
            key.last_rotated = SystemTime::now();
        }
        Ok(())
    }

    pub async fn get_key(&self, key_id: Uuid) -> Result<Option<EncryptionKey>> {
        let keys = self.keys.read().await;
        Ok(keys.get(&key_id).cloned())
    }

    pub async fn delete_key(&self, key_id: Uuid) -> Result<()> {
        let mut keys = self.keys.write().await;
        keys.remove(&key_id);
        Ok(())
    }

    fn generate_key_data(&self, algorithm: &str) -> Result<Vec<u8>> {
        match algorithm {
            "AES-256-GCM" => {
                // Generate 256-bit (32 byte) key
                use rand::RngCore;
                let mut key = vec![0u8; 32];
                rand::thread_rng().fill_bytes(&mut key);
                Ok(key)
            }
            "AES-128-GCM" => {
                // Generate 128-bit (16 byte) key
                use rand::RngCore;
                let mut key = vec![0u8; 16];
                rand::thread_rng().fill_bytes(&mut key);
                Ok(key)
            }
            _ => Err(anyhow::anyhow!("Unsupported encryption algorithm: {}", algorithm))
        }
    }
}