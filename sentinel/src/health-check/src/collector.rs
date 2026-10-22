//! Metrics collector

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::checks::{HealthCheckResult, HealthStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorConfig {
    pub interval_secs: u64,
    pub retention_hours: u64,
    pub export_prometheus: bool,
    pub prometheus_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeries {
    pub name: String,
    pub unit: String,
    pub points: VecDeque<MetricPoint>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub timestamp: DateTime<Utc>,
    pub overall: HealthStatus,
    pub checks: Vec<HealthCheckResult>,
}

pub struct MetricsCollector {
    config: CollectorConfig,
    metrics: Arc<DashMap<String, MetricSeries>>,
    history: Arc<RwLock<VecDeque<HealthSnapshot>>>,
    prometheus_registry: Option<prometheus::Registry>,
}

impl MetricsCollector {
    pub async fn new(config: &CollectorConfig) -> Result<Self> {
        let prometheus_registry = if config.export_prometheus {
            Some(prometheus::Registry::new())
        } else {
            None
        };

        Ok(Self {
            config: config.clone(),
            metrics: Arc::new(DashMap::new()),
            history: Arc::new(RwLock::new(VecDeque::new())),
            prometheus_registry,
        })
    }

    pub async fn record_results(&self, results: &[HealthCheckResult]) -> Result<()> {
        let timestamp = Utc::now();

        // Determine overall status
        let overall = results.iter()
            .map(|r| &r.status)
            .max_by_key(|s| match s {
                HealthStatus::Critical => 3,
                HealthStatus::Warning => 2,
                HealthStatus::Unknown => 1,
                HealthStatus::Healthy => 0,
            })
            .cloned()
            .unwrap_or(HealthStatus::Unknown);

        // Record snapshot
        let snapshot = HealthSnapshot {
            timestamp,
            overall: overall.clone(),
            checks: results.to_vec(),
        };

        {
            let mut history = self.history.write().await;
            history.push_back(snapshot);

            // Trim to retention
            let cutoff = timestamp - chrono::Duration::hours(self.config.retention_hours as i64);
            while let Some(front) = history.front() {
                if front.timestamp < cutoff {
                    history.pop_front();
                } else {
                    break;
                }
            }
        }

        // Record individual metrics
        for result in results {
            if let Some(value) = result.value {
                self.record_metric(
                    &format!("health_{}", result.name),
                    value,
                    &result.unit,
                    &[
                        ("status", format!("{:?}", result.status).to_lowercase()),
                        ("check", result.name.clone()),
                    ],
                    timestamp,
                ).await?;
            }

            // Record threshold metrics
            if let Some(warn) = result.threshold_warning {
                self.record_metric(
                    &format!("health_{}_threshold_warning", result.name),
                    warn,
                    &result.unit,
                    &[("check", result.name.clone())],
                    timestamp,
                ).await?;
            }

            if let Some(crit) = result.threshold_critical {
                self.record_metric(
                    &format!("health_{}_threshold_critical", result.name),
                    crit,
                    &result.unit,
                    &[("check", result.name.clone())],
                    timestamp,
                ).await?;
            }
        }

        // Update Prometheus metrics
        if let Some(registry) = &self.prometheus_registry {
            self.update_prometheus(results, timestamp).await?;
        }

        Ok(())
    }

    async fn record_metric(
        &self,
        name: &str,
        value: f64,
        unit: &str,
        labels: &[(&str, String)],
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        let mut label_map = HashMap::new();
        for (k, v) in labels {
            label_map.insert(k.to_string(), v.clone());
        }

        let point = MetricPoint {
            timestamp,
            value,
            labels: label_map.clone(),
        };

        self.metrics.entry(name.to_string())
            .and_modify(|series| {
                series.points.push_back(point.clone());

                // Keep last 1000 points
                while series.points.len() > 1000 {
                    series.points.pop_front();
                }
            })
            .or_insert_with(|| MetricSeries {
                name: name.to_string(),
                unit: unit.to_string(),
                points: {
                    let mut dq = VecDeque::new();
                    dq.push_back(point);
                    dq
                },
                labels: label_map,
            });

        Ok(())
    }

    async fn update_prometheus(&self, results: &[HealthCheckResult], timestamp: DateTime<Utc>) -> Result<()> {
        if let Some(registry) = &self.prometheus_registry {
            // In real implementation, would update Prometheus gauges
            debug!("Would update Prometheus metrics at {}", timestamp);
        }
        Ok(())
    }

    pub async fn get_metric(&self, name: &str) -> Option<MetricSeries> {
        self.metrics.get(name).map(|s| s.clone())
    }

    pub async fn get_all_metrics(&self) -> Vec<MetricSeries> {
        self.metrics.iter().map(|s| s.clone()).collect()
    }

    pub async fn get_metric_points(
        &self,
        name: &str,
        since: Option<DateTime<Utc>>,
    ) -> Vec<MetricPoint> {
        if let Some(series) = self.metrics.get(name) {
            if let Some(since) = since {
                series.points.iter()
                    .filter(|p| p.timestamp >= since)
                    .cloned()
                    .collect()
            } else {
                series.points.iter().cloned().collect()
            }
        } else {
            Vec::new()
        }
    }

    pub async fn get_history(&self, since: Option<DateTime<Utc>>, limit: usize) -> Vec<HealthSnapshot> {
        let history = self.history.read().await;
        let filtered: Vec<_> = if let Some(since) = since {
            history.iter().filter(|s| s.timestamp >= since).cloned().collect()
        } else {
            history.iter().cloned().collect()
        };

        filtered.into_iter().rev().take(limit).collect()
    }

    pub async fn get_latest_snapshot(&self) -> Option<HealthSnapshot> {
        let history = self.history.read().await;
        history.back().cloned()
    }

    pub async fn get_overall_status(&self) -> Option<HealthStatus> {
        let history = self.history.read().await;
        history.back().map(|s| s.overall.clone())
    }

    pub async fn cleanup_old_metrics(&self, retention: Duration) -> Result<()> {
        let cutoff = SystemTime::now() - retention;

        // Clean metrics
        for mut series in self.metrics.iter_mut() {
            let cutoff_dt: DateTime<Utc> = cutoff.into();
            while let Some(front) = series.points.front() {
                if front.timestamp < cutoff_dt {
                    series.points.pop_front();
                } else {
                    break;
                }
            }
        }

        // Clean history
        {
            let mut history = self.history.write().await;
            let cutoff_dt: DateTime<Utc> = cutoff.into();
            while let Some(front) = history.front() {
                if front.timestamp < cutoff_dt {
                    history.pop_front();
                } else {
                    break;
                }
            }
        }

        Ok(())
    }

    pub async fn export_prometheus(&self) -> Result<String> {
        if let Some(registry) = &self.prometheus_registry {
            let encoder = prometheus::TextEncoder::new();
            let metric_families = registry.gather();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer)?;
            Ok(String::from_utf8(buffer)?)
        } else {
            Ok("# Prometheus export not enabled\n".to_string())
        }
    }

    pub async fn get_summary(&self) -> serde_json::Value {
        let mut check_status = HashMap::new();

        for entry in self.metrics.iter() {
            let name = entry.key().strip_prefix("health_").unwrap_or(entry.key()).to_string();
            if name.ends_with("_threshold_warning") || name.ends_with("_threshold_critical") {
                continue;
            }

            if let Some(point) = entry.points.back() {
                check_status.insert(name, serde_json::json!({
                    "value": point.value,
                    "unit": entry.unit,
                    "timestamp": point.timestamp,
                    "labels": point.labels,
                }));
            }
        }

        let latest = self.get_latest_snapshot().await;

        serde_json::json!({
            "overall_status": latest.as_ref().map(|s| format!("{:?}", s.overall).to_lowercase()),
            "last_check": latest.as_ref().map(|s| s.timestamp),
            "checks": check_status,
            "metric_count": self.metrics.len(),
            "history_size": self.history.read().await.len(),
        })
    }
}