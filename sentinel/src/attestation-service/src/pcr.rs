//! PCR policy manager

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::tpm::TpmManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcrConfig {
    pub policy_file: PathBuf,
    pub monitored_pcrs: Vec<u32>,
    pub alert_on_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcrPolicy {
    pub version: String,
    pub rules: Vec<PcrRule>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcrRule {
    pub pcr: u32,
    pub expected_digest: Option<String>,
    pub allow_list: Vec<String>, // Paths that are allowed to extend this PCR
    pub deny_list: Vec<String>,
    pub action: PcrAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PcrAction {
    Allow,
    Alert,
    Block,
    Quarantine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcrEvent {
    pub pcr: u32,
    pub old_digest: String,
    pub new_digest: String,
    pub event_data: String,
    pub timestamp: DateTime<Utc>,
    pub action_taken: PcrAction,
}

pub struct PcrManager {
    config: PcrConfig,
    policy: Arc<RwLock<PcrPolicy>>,
    tpm_manager: Option<Arc<RwLock<TpmManager>>>,
    event_log: Arc<RwLock<Vec<PcrEvent>>>,
    baseline: Arc<RwLock<HashMap<u32, String>>>,
}

impl PcrManager {
    pub async fn new(config: &PcrConfig) -> Result<Self> {
        let policy = if config.policy_file.exists() {
            let data = fs::read_to_string(&config.policy_file).await?;
            toml::from_str(&data)?
        } else {
            PcrPolicy::default()
        };

        Ok(Self {
            config: config.clone(),
            policy: Arc::new(RwLock::new(policy)),
            tpm_manager: None,
            event_log: Arc::new(RwLock::new(Vec::new())),
            baseline: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn set_tpm_manager(&mut self, tpm: Arc<RwLock<TpmManager>>) {
        self.tpm_manager = Some(tpm);
    }

    pub async fn establish_baseline(&self) -> Result<()> {
        if let Some(tpm) = &self.tpm_manager {
            let pcrs = self.config.monitored_pcrs.clone();
            let digests = tpm.write().await.read_pcrs(&pcrs).await?;

            let mut baseline = self.baseline.write().await;
            for (i, pcr) in pcrs.iter().enumerate() {
                if let Some(digest) = digests.get(i) {
                    baseline.insert(*pcr, hex::encode(digest.as_ref()));
                }
            }

            info!("Established PCR baseline for {} PCRs", pcrs.len());
        }
        Ok(())
    }

    pub async fn check_pcrs(&self) -> Result<Vec<PcrEvent>> {
        let mut events = Vec::new();

        if let Some(tpm) = &self.tpm_manager {
            let pcrs = self.config.monitored_pcrs.clone();
            let digests = tpm.write().await.read_pcrs(&pcrs).await?;

            let baseline = self.baseline.read().await;
            let policy = self.policy.read().await;

            for (i, pcr) in pcrs.iter().enumerate() {
                if let Some(digest) = digests.get(i) {
                    let new_digest = hex::encode(digest.as_ref());

                    if let Some(old_digest) = baseline.get(pcr) {
                        if *old_digest != new_digest {
                            // PCR changed!
                            let rule = policy.rules.iter().find(|r| r.pcr == *pcr);
                            let action = rule.map(|r| r.action.clone()).unwrap_or(PcrAction::Alert);

                            let event = PcrEvent {
                                pcr: *pcr,
                                old_digest: old_digest.clone(),
                                new_digest: new_digest.clone(),
                                event_data: format!("PCR {} changed", pcr),
                                timestamp: Utc::now(),
                                action_taken: action.clone(),
                            };

                            events.push(event.clone());

                            // Update baseline
                            self.baseline.write().await.insert(*pcr, new_digest);

                            // Take action
                            match action {
                                PcrAction::Alert => {
                                    warn!("PCR {} changed unexpectedly!", pcr);
                                }
                                PcrAction::Block => {
                                    // Would trigger compartment isolation
                                    warn!("PCR {} changed - BLOCK action triggered", pcr);
                                }
                                PcrAction::Quarantine => {
                                    // Would trigger compartment quarantine
                                    warn!("PCR {} changed - QUARANTINE action triggered", pcr);
                                }
                                PcrAction::Allow => {}
                            }
                        }
                    }
                }
            }
        }

        if !events.is_empty() {
            self.event_log.write().await.extend(events.clone());
        }

        Ok(events)
    }

    pub async fn start_monitoring(&self) {
        let mut interval = interval(Duration::from_secs(10));

        loop {
            interval.tick().await;
            if let Err(e) = self.check_pcrs().await {
                warn!("PCR check failed: {}", e);
            }
        }
    }

    pub async fn get_policy(&self) -> PcrPolicy {
        self.policy.read().await.clone()
    }

    pub async fn update_policy(&self, policy: PcrPolicy) -> Result<()> {
        *self.policy.write().await = policy.clone();

        let data = toml::to_string_pretty(&policy)?;
        fs::write(&self.config.policy_file, data).await?;

        info!("Updated PCR policy");
        Ok(())
    }

    pub async fn add_rule(&self, rule: PcrRule) -> Result<()> {
        let mut policy = self.policy.write().await;
        policy.rules.retain(|r| r.pcr != rule.pcr);
        policy.rules.push(rule);
        policy.updated_at = Utc::now();
        drop(policy);

        self.update_policy(self.policy.read().await.clone()).await
    }

    pub async fn get_events(&self, since: Option<DateTime<Utc>>) -> Vec<PcrEvent> {
        let events = self.event_log.read().await;
        if let Some(since) = since {
            events.iter().filter(|e| e.timestamp >= since).cloned().collect()
        } else {
            events.clone()
        }
    }

    pub async fn get_current_pcrs(&self) -> HashMap<u32, String> {
        let mut result = HashMap::new();

        if let Some(tpm) = &self.tpm_manager {
            let pcrs = self.config.monitored_pcrs.clone();
            if let Ok(digests) = tpm.write().await.read_pcrs(&pcrs).await {
                for (i, pcr) in pcrs.iter().enumerate() {
                    if let Some(digest) = digests.get(i) {
                        result.insert(*pcr, hex::encode(digest.as_ref()));
                    }
                }
            }
        }

        result
    }

    pub async fn extend_pcr(&self, pcr: u32, data: &[u8]) -> Result<()> {
        if let Some(tpm) = &self.tpm_manager {
            tpm.write().await.extend_pcr(pcr, data).await?;

            // Update baseline
            let digest = hex::encode(ring::digest::digest(&ring::digest::SHA256, data).as_ref());
            self.baseline.write().await.insert(pcr, digest);
        }

        Ok(())
    }
}

impl Default for PcrPolicy {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            rules: vec![
                PcrRule {
                    pcr: 0,
                    expected_digest: None,
                    allow_list: vec!["/boot/vmlinuz-*".to_string(), "/boot/initramfs-*".to_string()],
                    deny_list: vec![],
                    action: PcrAction::Alert,
                },
                PcrRule {
                    pcr: 1,
                    expected_digest: None,
                    allow_list: vec!["/boot/grub/*".to_string()],
                    deny_list: vec![],
                    action: PcrAction::Alert,
                },
                PcrRule {
                    pcr: 2,
                    expected_digest: None,
                    allow_list: vec![],
                    deny_list: vec![],
                    action: PcrAction::Alert,
                },
            ],
            updated_at: Utc::now(),
        }
    }
}

use ring::digest;