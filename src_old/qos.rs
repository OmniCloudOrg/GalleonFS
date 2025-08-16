use crate::*;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct QoSManager {
    policies: Arc<RwLock<HashMap<String, QoSPolicy>>>,
    active_limits: Arc<RwLock<HashMap<Uuid, ActiveQoSLimits>>>,
}

#[derive(Debug, Clone)]
pub struct ActiveQoSLimits {
    pub volume_id: Uuid,
    pub current_iops: u64,
    pub current_throughput_mbps: u64,
    pub burst_start_time: Option<SystemTime>,
    pub policy_name: String,
}

impl QoSManager {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
            active_limits: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_policy(&self, policy: QoSPolicy) -> Result<()> {
        let mut policies = self.policies.write().await;
        policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    pub async fn remove_policy(&self, policy_name: &str) -> Result<()> {
        let mut policies = self.policies.write().await;
        policies.remove(policy_name);
        
        // Remove active limits for this policy
        let mut active_limits = self.active_limits.write().await;
        active_limits.retain(|_id, limits| limits.policy_name != policy_name);
        
        Ok(())
    }

    pub async fn get_policy(&self, policy_name: &str) -> Result<Option<QoSPolicy>> {
        let policies = self.policies.read().await;
        Ok(policies.get(policy_name).cloned())
    }

    pub async fn list_policies(&self) -> Result<Vec<QoSPolicy>> {
        let policies = self.policies.read().await;
        Ok(policies.values().cloned().collect())
    }

    pub async fn apply_policy_to_volume(&self, volume_id: Uuid, volume_labels: &HashMap<String, String>) -> Result<()> {
        let policies = self.policies.read().await;
        
        // Find matching policy based on selectors
        for policy in policies.values() {
            if self.matches_selector(&policy.selector, volume_labels) {
                let active_limits = ActiveQoSLimits {
                    volume_id,
                    current_iops: 0,
                    current_throughput_mbps: 0,
                    burst_start_time: None,
                    policy_name: policy.name.clone(),
                };
                
                let mut active_limits_map = self.active_limits.write().await;
                active_limits_map.insert(volume_id, active_limits);
                break;
            }
        }
        
        Ok(())
    }

    pub async fn check_io_limits(&self, volume_id: Uuid, requested_iops: u64, requested_throughput_mbps: u64) -> Result<bool> {
        let policies = self.policies.read().await;
        let mut active_limits = self.active_limits.write().await;
        
        if let Some(limits) = active_limits.get_mut(&volume_id) {
            if let Some(policy) = policies.get(&limits.policy_name) {
                // Check IOPS limits
                if let Some(_iops_limit) = policy.limits.iops {
                    let effective_limit = self.calculate_effective_iops_limit(policy, limits)?;
                    if limits.current_iops + requested_iops > effective_limit {
                        return Ok(false);
                    }
                }
                
                // Check throughput limits
                if let Some(_throughput_limit) = policy.limits.throughput_mbps {
                    let effective_limit = self.calculate_effective_throughput_limit(policy, limits)?;
                    if limits.current_throughput_mbps + requested_throughput_mbps > effective_limit {
                        return Ok(false);
                    }
                }
                
                // Update current usage
                limits.current_iops += requested_iops;
                limits.current_throughput_mbps += requested_throughput_mbps;
            }
        }
        
        Ok(true)
    }

    pub async fn record_io_completion(&self, volume_id: Uuid, completed_iops: u64, completed_throughput_mbps: u64) -> Result<()> {
        let mut active_limits = self.active_limits.write().await;
        
        if let Some(limits) = active_limits.get_mut(&volume_id) {
            limits.current_iops = limits.current_iops.saturating_sub(completed_iops);
            limits.current_throughput_mbps = limits.current_throughput_mbps.saturating_sub(completed_throughput_mbps);
        }
        
        Ok(())
    }

    pub async fn get_volume_qos_status(&self, volume_id: Uuid) -> Result<Option<QoSStatus>> {
        let policies = self.policies.read().await;
        let active_limits = self.active_limits.read().await;
        
        if let Some(limits) = active_limits.get(&volume_id) {
            if let Some(policy) = policies.get(&limits.policy_name) {
                let status = QoSStatus {
                    volume_id,
                    policy_name: policy.name.clone(),
                    current_iops: limits.current_iops,
                    current_throughput_mbps: limits.current_throughput_mbps,
                    iops_limit: self.calculate_effective_iops_limit(policy, limits)?,
                    throughput_limit: self.calculate_effective_throughput_limit(policy, limits)?,
                    is_bursting: limits.burst_start_time.is_some(),
                };
                return Ok(Some(status));
            }
        }
        
        Ok(None)
    }

    fn matches_selector(&self, selector: &LabelSelector, labels: &HashMap<String, String>) -> bool {
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

    fn calculate_effective_iops_limit(&self, policy: &QoSPolicy, limits: &ActiveQoSLimits) -> Result<u64> {
        let base_limit = policy.limits.iops.unwrap_or(u64::MAX);
        
        if let Some(burstable) = &policy.burstable {
            if burstable.enabled {
                if let Some(burst_start) = limits.burst_start_time {
                    let burst_duration = SystemTime::now().duration_since(burst_start)
                        .unwrap_or_default();
                    
                    if burst_duration.as_secs() < burstable.duration_seconds {
                        return Ok((base_limit as f64 * burstable.iops_multiplier) as u64);
                    }
                }
            }
        }
        
        Ok(base_limit)
    }

    fn calculate_effective_throughput_limit(&self, policy: &QoSPolicy, limits: &ActiveQoSLimits) -> Result<u64> {
        let base_limit = policy.limits.throughput_mbps.unwrap_or(u64::MAX);
        
        if let Some(burstable) = &policy.burstable {
            if burstable.enabled {
                if let Some(burst_start) = limits.burst_start_time {
                    let burst_duration = SystemTime::now().duration_since(burst_start)
                        .unwrap_or_default();
                    
                    if burst_duration.as_secs() < burstable.duration_seconds {
                        return Ok((base_limit as f64 * burstable.throughput_multiplier) as u64);
                    }
                }
            }
        }
        
        Ok(base_limit)
    }

    pub async fn enable_burst(&self, volume_id: Uuid) -> Result<()> {
        let mut active_limits = self.active_limits.write().await;
        
        if let Some(limits) = active_limits.get_mut(&volume_id) {
            limits.burst_start_time = Some(SystemTime::now());
        }
        
        Ok(())
    }

    pub async fn disable_burst(&self, volume_id: Uuid) -> Result<()> {
        let mut active_limits = self.active_limits.write().await;
        
        if let Some(limits) = active_limits.get_mut(&volume_id) {
            limits.burst_start_time = None;
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct QoSStatus {
    pub volume_id: Uuid,
    pub policy_name: String,
    pub current_iops: u64,
    pub current_throughput_mbps: u64,
    pub iops_limit: u64,
    pub throughput_limit: u64,
    pub is_bursting: bool,
}