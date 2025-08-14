use crate::*;
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct MonitoringManager {
    volume_metrics: Arc<RwLock<HashMap<Uuid, Vec<VolumeMetrics>>>>,
    system_metrics: Arc<RwLock<Vec<SystemMetrics>>>,
    alerts: Arc<RwLock<Vec<Alert>>>,
    alert_rules: Arc<RwLock<HashMap<String, AlertRule>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: SystemTime,
    pub total_volumes: u64,
    pub total_capacity_bytes: u64,
    pub used_capacity_bytes: u64,
    pub total_snapshots: u64,
    pub active_migrations: u64,
    pub failed_operations: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub message: String,
    pub resource_id: Option<Uuid>,
    pub timestamp: SystemTime,
    pub resolved: bool,
    pub resolved_at: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    pub metric: String,
    pub condition: AlertCondition,
    pub threshold: f64,
    pub duration_seconds: u64,
    pub severity: AlertSeverity,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlertCondition {
    GreaterThan,
    LessThan,
    Equal,
}

impl MonitoringManager {
    pub fn new() -> Self {
        Self {
            volume_metrics: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(RwLock::new(Vec::new())),
            alerts: Arc::new(RwLock::new(Vec::new())),
            alert_rules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn record_volume_metrics(&self, metrics: VolumeMetrics) -> Result<()> {
        let mut volume_metrics = self.volume_metrics.write().await;
        
        let history = volume_metrics.entry(metrics.volume_id).or_insert_with(Vec::new);
        history.push(metrics.clone());
        
        // Keep only last 1000 metrics per volume to prevent unbounded growth
        if history.len() > 1000 {
            history.remove(0);
        }
        
        // Check alert rules
        self.check_alert_rules_for_volume(&metrics).await?;
        
        Ok(())
    }

    pub async fn record_system_metrics(&self, metrics: SystemMetrics) -> Result<()> {
        let mut system_metrics = self.system_metrics.write().await;
        system_metrics.push(metrics.clone());
        
        // Keep only last 1000 system metrics
        if system_metrics.len() > 1000 {
            system_metrics.remove(0);
        }
        
        // Check system-level alert rules
        self.check_system_alert_rules(&metrics).await?;
        
        Ok(())
    }

    pub async fn get_volume_metrics(&self, volume_id: Uuid, duration: Option<Duration>) -> Result<Vec<VolumeMetrics>> {
        let volume_metrics = self.volume_metrics.read().await;
        
        if let Some(metrics) = volume_metrics.get(&volume_id) {
            if let Some(duration) = duration {
                let cutoff_time = SystemTime::now() - duration;
                Ok(metrics.iter()
                    .filter(|m| m.timestamp >= cutoff_time)
                    .cloned()
                    .collect())
            } else {
                Ok(metrics.clone())
            }
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_system_metrics(&self, duration: Option<Duration>) -> Result<Vec<SystemMetrics>> {
        let system_metrics = self.system_metrics.read().await;
        
        if let Some(duration) = duration {
            let cutoff_time = SystemTime::now() - duration;
            Ok(system_metrics.iter()
                .filter(|m| m.timestamp >= cutoff_time)
                .cloned()
                .collect())
        } else {
            Ok(system_metrics.clone())
        }
    }

    pub async fn get_volume_statistics(&self, volume_id: Uuid, duration: Duration) -> Result<VolumeStatistics> {
        let metrics = self.get_volume_metrics(volume_id, Some(duration)).await?;
        
        if metrics.is_empty() {
            return Ok(VolumeStatistics::default());
        }
        
        let total_iops: f64 = metrics.iter().map(|m| m.iops).sum();
        let total_throughput: f64 = metrics.iter().map(|m| m.throughput_mbps).sum();
        let total_latency: f64 = metrics.iter().map(|m| m.latency_ms).sum();
        let total_errors: u64 = metrics.iter().map(|m| m.error_count).sum();
        
        let count = metrics.len() as f64;
        
        Ok(VolumeStatistics {
            volume_id,
            period_start: metrics.first().unwrap().timestamp,
            period_end: metrics.last().unwrap().timestamp,
            avg_iops: total_iops / count,
            max_iops: metrics.iter().map(|m| m.iops).fold(0.0, f64::max),
            avg_throughput_mbps: total_throughput / count,
            max_throughput_mbps: metrics.iter().map(|m| m.throughput_mbps).fold(0.0, f64::max),
            avg_latency_ms: total_latency / count,
            max_latency_ms: metrics.iter().map(|m| m.latency_ms).fold(0.0, f64::max),
            total_errors,
        })
    }

    pub async fn add_alert_rule(&self, rule: AlertRule) -> Result<()> {
        let mut alert_rules = self.alert_rules.write().await;
        alert_rules.insert(rule.name.clone(), rule);
        Ok(())
    }

    pub async fn remove_alert_rule(&self, rule_name: &str) -> Result<()> {
        let mut alert_rules = self.alert_rules.write().await;
        alert_rules.remove(rule_name);
        Ok(())
    }

    pub async fn get_alerts(&self, filter: Option<AlertFilter>) -> Result<Vec<Alert>> {
        let alerts = self.alerts.read().await;
        
        if let Some(filter) = filter {
            Ok(alerts.iter()
                .filter(|alert| self.matches_alert_filter(alert, &filter))
                .cloned()
                .collect())
        } else {
            Ok(alerts.clone())
        }
    }

    pub async fn resolve_alert(&self, alert_id: Uuid) -> Result<()> {
        let mut alerts = self.alerts.write().await;
        
        for alert in alerts.iter_mut() {
            if alert.id == alert_id {
                alert.resolved = true;
                alert.resolved_at = Some(SystemTime::now());
                break;
            }
        }
        
        Ok(())
    }

    async fn check_alert_rules_for_volume(&self, metrics: &VolumeMetrics) -> Result<()> {
        let alert_rules = self.alert_rules.read().await;
        
        for rule in alert_rules.values() {
            if !rule.enabled {
                continue;
            }
            
            let should_alert = match rule.metric.as_str() {
                "iops" => self.check_condition(metrics.iops, &rule.condition, rule.threshold),
                "throughput_mbps" => self.check_condition(metrics.throughput_mbps, &rule.condition, rule.threshold),
                "latency_ms" => self.check_condition(metrics.latency_ms, &rule.condition, rule.threshold),
                "error_count" => self.check_condition(metrics.error_count as f64, &rule.condition, rule.threshold),
                _ => false,
            };
            
            if should_alert {
                self.create_alert(rule, Some(metrics.volume_id)).await?;
            }
        }
        
        Ok(())
    }

    async fn check_system_alert_rules(&self, metrics: &SystemMetrics) -> Result<()> {
        let alert_rules = self.alert_rules.read().await;
        
        for rule in alert_rules.values() {
            if !rule.enabled {
                continue;
            }
            
            let should_alert = match rule.metric.as_str() {
                "capacity_usage_percent" => {
                    let usage_percent = (metrics.used_capacity_bytes as f64 / metrics.total_capacity_bytes as f64) * 100.0;
                    self.check_condition(usage_percent, &rule.condition, rule.threshold)
                }
                "failed_operations" => self.check_condition(metrics.failed_operations as f64, &rule.condition, rule.threshold),
                _ => false,
            };
            
            if should_alert {
                self.create_alert(rule, None).await?;
            }
        }
        
        Ok(())
    }

    fn check_condition(&self, value: f64, condition: &AlertCondition, threshold: f64) -> bool {
        match condition {
            AlertCondition::GreaterThan => value > threshold,
            AlertCondition::LessThan => value < threshold,
            AlertCondition::Equal => (value - threshold).abs() < f64::EPSILON,
        }
    }

    async fn create_alert(&self, rule: &AlertRule, resource_id: Option<Uuid>) -> Result<()> {
        let alert = Alert {
            id: Uuid::new_v4(),
            rule_name: rule.name.clone(),
            severity: rule.severity.clone(),
            message: format!("Alert triggered for rule: {}", rule.name),
            resource_id,
            timestamp: SystemTime::now(),
            resolved: false,
            resolved_at: None,
        };
        
        let mut alerts = self.alerts.write().await;
        alerts.push(alert);
        
        Ok(())
    }

    fn matches_alert_filter(&self, alert: &Alert, filter: &AlertFilter) -> bool {
        if let Some(severity) = &filter.severity {
            if &alert.severity != severity {
                return false;
            }
        }
        
        if let Some(resolved) = filter.resolved {
            if alert.resolved != resolved {
                return false;
            }
        }
        
        if let Some(start_time) = filter.start_time {
            if alert.timestamp < start_time {
                return false;
            }
        }
        
        if let Some(end_time) = filter.end_time {
            if alert.timestamp > end_time {
                return false;
            }
        }
        
        true
    }

    pub async fn get_health_status(&self) -> Result<HealthStatus> {
        let alerts = self.alerts.read().await;
        let critical_alerts = alerts.iter().filter(|a| !a.resolved && a.severity == AlertSeverity::Critical).count();
        let warning_alerts = alerts.iter().filter(|a| !a.resolved && a.severity == AlertSeverity::Warning).count();
        
        let overall_status = if critical_alerts > 0 {
            HealthState::Critical
        } else if warning_alerts > 0 {
            HealthState::Warning
        } else {
            HealthState::Healthy
        };
        
        Ok(HealthStatus {
            overall_status,
            critical_alerts: critical_alerts as u64,
            warning_alerts: warning_alerts as u64,
            last_check: SystemTime::now(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VolumeStatistics {
    pub volume_id: Uuid,
    pub period_start: SystemTime,
    pub period_end: SystemTime,
    pub avg_iops: f64,
    pub max_iops: f64,
    pub avg_throughput_mbps: f64,
    pub max_throughput_mbps: f64,
    pub avg_latency_ms: f64,
    pub max_latency_ms: f64,
    pub total_errors: u64,
}

impl Default for VolumeStatistics {
    fn default() -> Self {
        Self {
            volume_id: Uuid::nil(),
            period_start: SystemTime::now(),
            period_end: SystemTime::now(),
            avg_iops: 0.0,
            max_iops: 0.0,
            avg_throughput_mbps: 0.0,
            max_throughput_mbps: 0.0,
            avg_latency_ms: 0.0,
            max_latency_ms: 0.0,
            total_errors: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlertFilter {
    pub severity: Option<AlertSeverity>,
    pub resolved: Option<bool>,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HealthStatus {
    pub overall_status: HealthState,
    pub critical_alerts: u64,
    pub warning_alerts: u64,
    pub last_check: SystemTime,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HealthState {
    Healthy,
    Warning,
    Critical,
}