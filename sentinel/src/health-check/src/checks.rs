//! Health checks implementation

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sysinfo::{CpuExt, DiskExt, NetworkExt, ProcessExt, System, SystemExt};
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::ThresholdsConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub name: String,
    pub status: HealthStatus,
    pub message: String,
    pub value: Option<f64>,
    pub threshold_warning: Option<f64>,
    pub threshold_critical: Option<f64>,
    pub unit: String,
    pub timestamp: DateTime<Utc>,
    pub details: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub overall: HealthStatus,
    pub checks: Vec<HealthCheckResult>,
    pub timestamp: DateTime<Utc>,
}

pub struct HealthChecker {
    config: ChecksConfig,
    system: System,
    thresholds: ThresholdsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecksConfig {
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
    pub thresholds: ThresholdsConfig,
}

impl HealthChecker {
    pub async fn new(config: &ChecksConfig) -> Result<Self> {
        let mut system = System::new_all();
        system.refresh_all();

        Ok(Self {
            config: config.clone(),
            system,
            thresholds: config.thresholds.clone(),
        })
    }

    pub async fn run_all_checks(&mut self) -> Result<Vec<HealthCheckResult>> {
        self.system.refresh_all();

        let mut results = Vec::new();

        // Run enabled checks
        for check_name in &self.config.enabled {
            if self.config.disabled.contains(check_name) {
                continue;
            }

            let result = match check_name.as_str() {
                "cpu" => self.check_cpu().await,
                "memory" => self.check_memory().await,
                "disk" => self.check_disk().await,
                "network" => self.check_network().await,
                "gpu" => self.check_gpu().await,
                "temperature" => self.check_temperature().await,
                "smart" => self.check_smart().await,
                "services" => self.check_services().await,
                "security" => self.check_security().await,
                "containers" => self.check_containers().await,
                "tpm" => self.check_tpm().await,
                "ima" => self.check_ima().await,
                _ => Ok(HealthCheckResult {
                    name: check_name.clone(),
                    status: HealthStatus::Unknown,
                    message: format!("Unknown check: {}", check_name),
                    value: None,
                    threshold_warning: None,
                    threshold_critical: None,
                    unit: "".to_string(),
                    timestamp: Utc::now(),
                    details: HashMap::new(),
                }),
            };

            results.push(result?);
        }

        Ok(results)
    }

    pub async fn run_check(&mut self, name: &str) -> Result<HealthCheckResult> {
        self.system.refresh_all();

        match name {
            "cpu" => self.check_cpu().await,
            "memory" => self.check_memory().await,
            "disk" => self.check_disk().await,
            "network" => self.check_network().await,
            "gpu" => self.check_gpu().await,
            "temperature" => self.check_temperature().await,
            "smart" => self.check_smart().await,
            "services" => self.check_services().await,
            "security" => self.check_security().await,
            "containers" => self.check_containers().await,
            "tpm" => self.check_tpm().await,
            "ima" => self.check_ima().await,
            _ => Err(anyhow::anyhow!("Unknown check: {}", name)),
        }
    }

    async fn check_cpu(&mut self) -> Result<HealthCheckResult> {
        self.system.refresh_cpu();
        let cpu_usage = self.system.global_cpu_info().cpu_usage() as f64;

        let (status, message) = if cpu_usage >= self.thresholds.cpu_critical as f64 {
            (HealthStatus::Critical, format!("CPU usage critical: {:.1}%", cpu_usage))
        } else if cpu_usage >= self.thresholds.cpu_warning as f64 {
            (HealthStatus::Warning, format!("CPU usage high: {:.1}%", cpu_usage))
        } else {
            (HealthStatus::Healthy, format!("CPU usage normal: {:.1}%", cpu_usage))
        };

        let mut details = HashMap::new();
        for (i, cpu) in self.system.cpus().iter().enumerate() {
            details.insert(format!("cpu{}", i), serde_json::json!(cpu.cpu_usage()));
        }

        Ok(HealthCheckResult {
            name: "cpu".to_string(),
            status,
            message,
            value: Some(cpu_usage),
            threshold_warning: Some(self.thresholds.cpu_warning as f64),
            threshold_critical: Some(self.thresholds.cpu_critical as f64),
            unit: "percent".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_memory(&mut self) -> Result<HealthCheckResult> {
        self.system.refresh_memory();
        let total = self.system.total_memory() as f64;
        let used = self.system.used_memory() as f64;
        let usage_pct = (used / total * 100.0) as f64;

        let (status, message) = if usage_pct >= self.thresholds.memory_critical as f64 {
            (HealthStatus::Critical, format!("Memory usage critical: {:.1}%", usage_pct))
        } else if usage_pct >= self.thresholds.memory_warning as f64 {
            (HealthStatus::Warning, format!("Memory usage high: {:.1}%", usage_pct))
        } else {
            (HealthStatus::Healthy, format!("Memory usage normal: {:.1}%", usage_pct))
        };

        let mut details = HashMap::new();
        details.insert("total_bytes".to_string(), serde_json::json!(total));
        details.insert("used_bytes".to_string(), serde_json::json!(used));
        details.insert("available_bytes".to_string(), serde_json::json!(self.system.available_memory() as f64));
        details.insert("swap_total_bytes".to_string(), serde_json::json!(self.system.total_swap() as f64));
        details.insert("swap_used_bytes".to_string(), serde_json::json!(self.system.used_swap() as f64));

        Ok(HealthCheckResult {
            name: "memory".to_string(),
            status,
            message,
            value: Some(usage_pct),
            threshold_warning: Some(self.thresholds.memory_warning as f64),
            threshold_critical: Some(self.thresholds.memory_critical as f64),
            unit: "percent".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_disk(&mut self) -> Result<HealthCheckResult> {
        self.system.refresh_disks();
        let mut worst_status = HealthStatus::Healthy;
        let mut messages = Vec::new();
        let mut details = HashMap::new();

        for disk in self.system.disks() {
            let total = disk.total_space() as f64;
            let available = disk.available_space() as f64;
            let used = total - available;
            let usage_pct = if total > 0.0 { (used / total * 100.0) as f64 } else { 0.0 };

            let mount = disk.mount_point().to_string_lossy().to_string();

            let (status, message) = if usage_pct >= self.thresholds.disk_critical as f64 {
                (HealthStatus::Critical, format!("Disk {} critical: {:.1}%", mount, usage_pct))
            } else if usage_pct >= self.thresholds.disk_warning as f64 {
                (HealthStatus::Warning, format!("Disk {} high: {:.1}%", mount, usage_pct))
            } else {
                (HealthStatus::Healthy, format!("Disk {} normal: {:.1}%", mount, usage_pct))
            };

            if status == HealthStatus::Critical {
                worst_status = HealthStatus::Critical;
            } else if status == HealthStatus::Warning && worst_status == HealthStatus::Healthy {
                worst_status = HealthStatus::Warning;
            }

            messages.push(message);

            details.insert(mount.clone(), serde_json::json!({
                "total_bytes": total,
                "used_bytes": used,
                "available_bytes": available,
                "usage_percent": usage_pct,
                "filesystem": disk.file_system().to_string_lossy(),
            }));
        }

        Ok(HealthCheckResult {
            name: "disk".to_string(),
            status: worst_status,
            message: messages.join("; "),
            value: None,
            threshold_warning: Some(self.thresholds.disk_warning as f64),
            threshold_critical: Some(self.thresholds.disk_critical as f64),
            unit: "percent".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_network(&mut self) -> Result<HealthCheckResult> {
        self.system.refresh_network();
        let mut details = HashMap::new();
        let mut total_rx = 0u64;
        let mut total_tx = 0u64;

        for (name, data) in self.system.networks() {
            total_rx += data.received();
            total_tx += data.transmitted();

            details.insert(name.clone(), serde_json::json!({
                "rx_bytes": data.received(),
                "tx_bytes": data.transmitted(),
                "rx_packets": data.packets_received(),
                "tx_packets": data.packets_transmitted(),
                "rx_errors": data.errors_received(),
                "tx_errors": data.errors_transmitted(),
            }));
        }

        Ok(HealthCheckResult {
            name: "network".to_string(),
            status: HealthStatus::Healthy,
            message: format!("Network interfaces: {}", self.system.networks().len()),
            value: Some((total_rx + total_tx) as f64),
            threshold_warning: None,
            threshold_critical: None,
            unit: "bytes".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_gpu(&mut self) -> Result<HealthCheckResult> {
        // Try to query GPU via nvidia-smi
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu", "--format=csv,noheader,nounits"])
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut details = HashMap::new();
                let mut worst_status = HealthStatus::Healthy;
                let mut gpu_count = 0;

                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 6 {
                        let index = parts[0];
                        let name = parts[1];
                        let util = parts[2].parse::<f64>().unwrap_or(0.0);
                        let mem_used = parts[3].parse::<f64>().unwrap_or(0.0);
                        let mem_total = parts[4].parse::<f64>().unwrap_or(1.0);
                        let temp = parts[5].parse::<f64>().unwrap_or(0.0);

                        let mem_pct = (mem_used / mem_total * 100.0) as f64;

                        let (status, _msg) = if mem_pct >= self.thresholds.gpu_memory_critical as f64 {
                            (HealthStatus::Critical, format!("GPU {} memory critical: {:.1}%", index, mem_pct))
                        } else if mem_pct >= self.thresholds.gpu_memory_warning as f64 {
                            (HealthStatus::Warning, format!("GPU {} memory high: {:.1}%", index, mem_pct))
                        } else {
                            (HealthStatus::Healthy, format!("GPU {} normal", index))
                        };

                        if status == HealthStatus::Critical {
                            worst_status = HealthStatus::Critical;
                        } else if status == HealthStatus::Warning && worst_status == HealthStatus::Healthy {
                            worst_status = HealthStatus::Warning;
                        }

                        details.insert(format!("gpu{}", index), serde_json::json!({
                            "name": name,
                            "utilization_percent": util,
                            "memory_used_mb": mem_used,
                            "memory_total_mb": mem_total,
                            "memory_percent": mem_pct,
                            "temperature_c": temp,
                        }));

                        gpu_count += 1;
                    }
                }

                Ok(HealthCheckResult {
                    name: "gpu".to_string(),
                    status: worst_status,
                    message: format!("GPUs detected: {}", gpu_count),
                    value: Some(gpu_count as f64),
                    threshold_warning: Some(self.thresholds.gpu_memory_warning as f64),
                    threshold_critical: Some(self.thresholds.gpu_memory_critical as f64),
                    unit: "count".to_string(),
                    timestamp: Utc::now(),
                    details,
                })
            }
            _ => {
                // No GPU or nvidia-smi not available
                Ok(HealthCheckResult {
                    name: "gpu".to_string(),
                    status: HealthStatus::Healthy,
                    message: "No GPU detected or nvidia-smi not available".to_string(),
                    value: Some(0.0),
                    threshold_warning: None,
                    threshold_critical: None,
                    unit: "count".to_string(),
                    timestamp: Utc::now(),
                    details: HashMap::new(),
                })
            }
        }
    }

    async fn check_temperature(&mut self) -> Result<HealthCheckResult> {
        self.system.refresh_components();
        let mut details = HashMap::new();
        let mut worst_status = HealthStatus::Healthy;
        let mut messages = Vec::new();

        for component in self.system.components() {
            let temp = component.temperature() as f64;
            let label = component.label().to_string();

            let (status, message) = if temp >= self.thresholds.temperature_critical as f64 {
                (HealthStatus::Critical, format!("{} temperature critical: {:.1}°C", label, temp))
            } else if temp >= self.thresholds.temperature_warning as f64 {
                (HealthStatus::Warning, format!("{} temperature high: {:.1}°C", label, temp))
            } else {
                (HealthStatus::Healthy, format!("{} temperature normal: {:.1}°C", label, temp))
            };

            if status == HealthStatus::Critical {
                worst_status = HealthStatus::Critical;
            } else if status == HealthStatus::Warning && worst_status == HealthStatus::Healthy {
                worst_status = HealthStatus::Warning;
            }

            messages.push(message);
            details.insert(label, serde_json::json!({
                "temperature_c": temp,
                "max_temperature_c": component.max(),
                "critical_temperature_c": component.critical(),
            }));
        }

        if details.is_empty() {
            // Try reading from thermal zones
            if let Ok(entries) = std::fs::read_dir("/sys/class/thermal") {
                for entry in entries.flatten() {
                    if let Ok(temp_str) = std::fs::read_to_string(entry.path().join("temp")) {
                        if let Ok(temp_milli) = temp_str.trim().parse::<i64>() {
                            let temp = temp_milli as f64 / 1000.0;
                            let name = entry.file_name().to_string_lossy().to_string();

                            let (status, message) = if temp >= self.thresholds.temperature_critical as f64 {
                                (HealthStatus::Critical, format!("{} critical: {:.1}°C", name, temp))
                            } else if temp >= self.thresholds.temperature_warning as f64 {
                                (HealthStatus::Warning, format!("{} high: {:.1}°C", name, temp))
                            } else {
                                (HealthStatus::Healthy, format!("{} normal: {:.1}°C", name, temp))
                            };

                            if status == HealthStatus::Critical {
                                worst_status = HealthStatus::Critical;
                            } else if status == HealthStatus::Warning && worst_status == HealthStatus::Healthy {
                                worst_status = HealthStatus::Warning;
                            }

                            messages.push(message);
                            details.insert(name, serde_json::json!({ "temperature_c": temp }));
                        }
                    }
                }
            }
        }

        Ok(HealthCheckResult {
            name: "temperature".to_string(),
            status: worst_status,
            message: messages.join("; "),
            value: None,
            threshold_warning: Some(self.thresholds.temperature_warning as f64),
            threshold_critical: Some(self.thresholds.temperature_critical as f64),
            unit: "celsius".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_smart(&mut self) -> Result<HealthCheckResult> {
        let mut details = HashMap::new();
        let mut worst_status = HealthStatus::Healthy;
        let mut messages = Vec::new();

        // Try smartctl
        let output = Command::new("smartctl")
            .args(["--scan"])
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Some(device) = line.split_whitespace().next() {
                        let smart_out = Command::new("smartctl")
                            .args(["-H", device])
                            .output()
                            .await;

                        if let Ok(smart_result) = smart_out {
                            let smart_stdout = String::from_utf8_lossy(&smart_result.stdout);
                            let passed = smart_stdout.contains("PASSED");

                            let status = if passed {
                                HealthStatus::Healthy
                            } else {
                                worst_status = HealthStatus::Critical;
                                HealthStatus::Critical
                            };

                            messages.push(format!("{}: {}", device, if passed { "PASSED" } else { "FAILED" }));
                            details.insert(device.to_string(), serde_json::json!({
                                "passed": passed,
                                "output": smart_stdout,
                            }));
                        }
                    }
                }
            }
        }

        if details.is_empty() {
            return Ok(HealthCheckResult {
                name: "smart".to_string(),
                status: HealthStatus::Unknown,
                message: "No SMART-capable devices found or smartctl not available".to_string(),
                value: None,
                threshold_warning: None,
                threshold_critical: None,
                unit: "".to_string(),
                timestamp: Utc::now(),
                details: HashMap::new(),
            });
        }

        Ok(HealthCheckResult {
            name: "smart".to_string(),
            status: worst_status,
            message: messages.join("; "),
            value: None,
            threshold_warning: None,
            threshold_critical: None,
            unit: "".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_services(&mut self) -> Result<HealthCheckResult> {
        let critical_services = vec![
            "sentinel-capability-broker",
            "sentinel-compartment-manager",
            "sentinel-ai-orchestrator",
            "sentinel-model-registry",
            "sentinel-threat-intel",
            "sentinel-attestation",
            "sentinel-health-check",
        ];

        let mut details = HashMap::new();
        let mut worst_status = HealthStatus::Healthy;
        let mut messages = Vec::new();

        for service in critical_services {
            let output = Command::new("systemctl")
                .args(["is-active", service])
                .output()
                .await;

            let is_active = output.map(|o| o.status.success()).unwrap_or(false);

            let status = if is_active {
                HealthStatus::Healthy
            } else {
                if worst_status != HealthStatus::Critical {
                    worst_status = HealthStatus::Warning;
                }
                HealthStatus::Warning
            };

            messages.push(format!("{}: {}", service, if is_active { "active" } else { "inactive" }));
            details.insert(service.to_string(), serde_json::json!({ "active": is_active }));
        }

        Ok(HealthCheckResult {
            name: "services".to_string(),
            status: worst_status,
            message: messages.join("; "),
            value: None,
            threshold_warning: None,
            threshold_critical: None,
            unit: "".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_security(&mut self) -> Result<HealthCheckResult> {
        let mut details = HashMap::new();
        let mut worst_status = HealthStatus::Healthy;
        let mut messages = Vec::new();

        // Check secure boot
        let secure_boot = Path::new("/sys/firmware/efi/efivars/SecureBoot-").exists();
        details.insert("secure_boot".to_string(), serde_json::json!({ "enabled": secure_boot }));
        if !secure_boot {
            worst_status = HealthStatus::Warning;
            messages.push("Secure Boot disabled".to_string());
        } else {
            messages.push("Secure Boot enabled".to_string());
        }

        // Check TPM
        let tpm_exists = Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists();
        details.insert("tpm".to_string(), serde_json::json!({ "available": tpm_exists }));
        if !tpm_exists {
            worst_status = HealthStatus::Warning;
            messages.push("TPM not available".to_string());
        } else {
            messages.push("TPM available".to_string());
        }

        // Check IMA/EVM
        let ima_exists = Path::new("/sys/kernel/security/ima/ascii_runtime_measurements").exists();
        details.insert("ima".to_string(), serde_json::json!({ "enabled": ima_exists }));
        if !ima_exists {
            worst_status = HealthStatus::Warning;
            messages.push("IMA not enabled".to_string());
        } else {
            messages.push("IMA enabled".to_string());
        }

        // Check lockdown mode
        let lockdown = std::fs::read_to_string("/sys/kernel/security/lockdown")
            .map(|s| s.trim().to_string())
            .unwrap_or("unknown".to_string());
        details.insert("lockdown".to_string(), serde_json::json!({ "mode": lockdown }));
        if lockdown != "confidentiality" && lockdown != "integrity" {
            worst_status = HealthStatus::Warning;
            messages.push(format!("Lockdown mode: {}", lockdown));
        } else {
            messages.push(format!("Lockdown mode: {}", lockdown));
        }

        // Check AppArmor
        let apparmor = Path::new("/sys/kernel/security/apparmor/profiles").exists();
        details.insert("apparmor".to_string(), serde_json::json!({ "enabled": apparmor }));
        if !apparmor {
            worst_status = HealthStatus::Warning;
            messages.push("AppArmor not enabled".to_string());
        } else {
            messages.push("AppArmor enabled".to_string());
        }

        Ok(HealthCheckResult {
            name: "security".to_string(),
            status: worst_status,
            message: messages.join("; "),
            value: None,
            threshold_warning: None,
            threshold_critical: None,
            unit: "".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_containers(&mut self) -> Result<HealthCheckResult> {
        let mut details = HashMap::new();
        let mut worst_status = HealthStatus::Healthy;
        let mut messages = Vec::new();

        // Check containerd
        let containerd_active = Command::new("systemctl")
            .args(["is-active", "containerd"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        details.insert("containerd".to_string(), serde_json::json!({ "active": containerd_active }));

        if containerd_active {
            messages.push("containerd active".to_string());

            // Get container count
            let ctr_out = Command::new("ctr")
                .args(["containers", "ls", "-q"])
                .output()
                .await;

            if let Ok(out) = ctr_out {
                let count = String::from_utf8_lossy(&out.stdout).lines().count();
                details.insert("container_count".to_string(), serde_json::json!(count));
                messages.push(format!("Containers: {}", count));
            }
        } else {
            worst_status = HealthStatus::Warning;
            messages.push("containerd inactive".to_string());
        }

        // Check kata
        let kata_available = Command::new("which")
            .args(["kata-runtime"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        details.insert("kata".to_string(), serde_json::json!({ "available": kata_available }));

        // Check crosvm
        let crosvm_available = Command::new("which")
            .args(["crosvm"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        details.insert("crosvm".to_string(), serde_json::json!({ "available": crosvm_available }));

        Ok(HealthCheckResult {
            name: "containers".to_string(),
            status: worst_status,
            message: messages.join("; "),
            value: None,
            threshold_warning: None,
            threshold_critical: None,
            unit: "".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_tpm(&mut self) -> Result<HealthCheckResult> {
        let mut details = HashMap::new();

        // Check TPM availability
        let tpm_available = Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists();
        details.insert("available".to_string(), serde_json::json!(tpm_available));

        if !tpm_available {
            return Ok(HealthCheckResult {
                name: "tpm".to_string(),
                status: HealthStatus::Critical,
                message: "TPM device not found".to_string(),
                value: Some(0.0),
                threshold_warning: None,
                threshold_critical: None,
                unit: "".to_string(),
                timestamp: Utc::now(),
                details,
            });
        }

        // Try to read TPM info
        let tpm2_getcap = Command::new("tpm2_getcap")
            .args(["properties-fixed"])
            .output()
            .await;

        if let Ok(out) = tpm2_getcap {
            let stdout = String::from_utf8_lossy(&out.stdout);
            details.insert("properties".to_string(), serde_json::json!(stdout));
        }

        Ok(HealthCheckResult {
            name: "tpm".to_string(),
            status: HealthStatus::Healthy,
            message: "TPM available".to_string(),
            value: Some(1.0),
            threshold_warning: None,
            threshold_critical: None,
            unit: "".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    async fn check_ima(&mut self) -> Result<HealthCheckResult> {
        let ima_log = Path::new("/sys/kernel/security/ima/ascii_runtime_measurements");
        let ima_enabled = ima_log.exists();

        let mut details = HashMap::new();
        details.insert("enabled".to_string(), serde_json::json!(ima_enabled));

        if ima_enabled {
            // Count measurements
            if let Ok(content) = std::fs::read_to_string(ima_log) {
                let count = content.lines().count();
                details.insert("measurement_count".to_string(), serde_json::json!(count));
            }
        }

        Ok(HealthCheckResult {
            name: "ima".to_string(),
            status: if ima_enabled { HealthStatus::Healthy } else { HealthStatus::Warning },
            message: if ima_enabled { "IMA enabled" } else { "IMA not enabled" }.to_string(),
            value: if ima_enabled { Some(1.0) } else { Some(0.0) },
            threshold_warning: None,
            threshold_critical: None,
            unit: "".to_string(),
            timestamp: Utc::now(),
            details,
        })
    }

    pub fn get_system_info(&self) -> serde_json::Value {
        serde_json::json!({
            "hostname": System::host_name(),
            "kernel_version": System::kernel_version(),
            "os_version": System::os_version(),
            "distribution_id": System::distribution_id(),
            "total_memory": self.system.total_memory(),
            "total_swap": self.system.total_swap(),
            "cpu_count": self.system.cpus().len(),
            "cpu_brand": self.system.cpus().first().map(|c| c.brand()).unwrap_or("Unknown"),
            "uptime": System::uptime(),
        })
    }
}