//! IMA/EVM measurement log manager

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ring::digest;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::tpm::TpmManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImaConfig {
    pub log_path: PathBuf,
    pub evm_log_path: PathBuf,
    pub verify_signatures: bool,
    pub allowed_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImaEntry {
    pub pcr: u32,
    pub digest: String,
    pub template: String,
    pub template_data: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmEntry {
    pub pcr: u32,
    pub digest: String,
    pub template: String,
    pub template_data: String,
    pub xattr_name: String,
    pub xattr_value: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImaLog {
    pub entries: Vec<ImaEntry>,
    pub last_position: u64,
    pub last_updated: DateTime<Utc>,
}

pub struct ImaManager {
    config: ImaConfig,
    log: Arc<RwLock<ImaLog>>,
    evm_log: Arc<RwLock<Vec<EvmEntry>>>,
    known_good: Arc<RwLock<HashMap<String, String>>>,
    tpm_manager: Option<Arc<RwLock<TpmManager>>>,
}

impl ImaManager {
    pub async fn new(config: &ImaConfig) -> Result<Self> {
        let manager = Self {
            config: config.clone(),
            log: Arc::new(RwLock::new(ImaLog {
                entries: Vec::new(),
                last_position: 0,
                last_updated: Utc::now(),
            })),
            evm_log: Arc::new(RwLock::new(Vec::new())),
            known_good: Arc::new(RwLock::new(HashMap::new())),
            tpm_manager: None,
        };

        // Load initial log
        manager.reload_log().await?;

        // Load known good database if exists
        manager.load_known_good().await?;

        Ok(manager)
    }

    pub fn set_tpm_manager(&mut self, tpm: Arc<RwLock<TpmManager>>) {
        self.tpm_manager = Some(tpm);
    }

    pub async fn reload_log(&self) -> Result<()> {
        let file = fs::File::open(&self.config.log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut entries = Vec::new();
        let mut position = 0;

        while let Some(line) = lines.next_line().await? {
            position += line.len() as u64 + 1; // +1 for newline
            if let Some(entry) = self.parse_ima_line(&line) {
                entries.push(entry);
            }
        }

        {
            let mut log = self.log.write().await;
            log.entries = entries;
            log.last_position = position;
            log.last_updated = Utc::now();
        }

        info!("Loaded {} IMA entries", log.read().await.entries.len());
        Ok(())
    }

    fn parse_ima_line(&self, line: &str) -> Option<ImaEntry> {
        // IMA log format: pcr digest template-hash template-data
        // Example: 10 0123456789abcdef... ima-ng sha256:0123456789abcdef... /path/to/file
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        let pcr = parts[0].parse().ok()?;
        let digest = parts[1].to_string();
        let template = parts[2].to_string();
        let template_data = parts[3..].join(" ");

        Some(ImaEntry {
            pcr,
            digest,
            template,
            template_data,
            timestamp: Utc::now(), // Would parse from log if available
        })
    }

    pub async fn get_new_entries(&self) -> Result<Vec<ImaEntry>> {
        let file = fs::File::open(&self.config.log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let last_pos = {
            let log = self.log.read().await;
            log.last_position
        };

        let mut new_entries = Vec::new();
        let mut current_pos = 0;

        while let Some(line) = lines.next_line().await? {
            current_pos += line.len() as u64 + 1;

            if current_pos > last_pos {
                if let Some(entry) = self.parse_ima_line(&line) {
                    new_entries.push(entry);
                }
            }
        }

        if !new_entries.is_empty() {
            let mut log = self.log.write().await;
            log.entries.extend(new_entries.clone());
            log.last_position = current_pos;
            log.last_updated = Utc::now();
        }

        Ok(new_entries)
    }

    pub async fn verify_measurement(&self, entry: &ImaEntry) -> Result<bool> {
        if !self.config.verify_signatures {
            return Ok(true);
        }

        // Check against known good database
        {
            let known = self.known_good.read().await;
            if let Some(expected) = known.get(&entry.template_data) {
                return Ok(*expected == entry.digest);
            }
        }

        // If not in known good, would verify signature using allowed keys
        // For now, return true
        Ok(true)
    }

    pub async fn get_log(&self) -> ImaLog {
        self.log.read().await.clone()
    }

    pub async fn get_entries_since(&self, since: DateTime<Utc>) -> Vec<ImaEntry> {
        let log = self.log.read().await;
        log.entries.iter()
            .filter(|e| e.timestamp >= since)
            .cloned()
            .collect()
    }

    pub async fn load_known_good(&self) -> Result<()> {
        let path = Path::new("/etc/sentinel/ima-known-good.json");
        if path.exists() {
            let data = fs::read_to_string(path).await?;
            let map: HashMap<String, String> = serde_json::from_str(&data)?;
            *self.known_good.write().await = map;
            info!("Loaded {} known good measurements", map.len());
        }
        Ok(())
    }

    pub async fn add_known_good(&self, path: &str, digest: &str) -> Result<()> {
        self.known_good.write().await.insert(path.to_string(), digest.to_string());

        // Save to disk
        let known = self.known_good.read().await;
        let data = serde_json::to_string_pretty(&*known)?;
        fs::write("/etc/sentinel/ima-known-good.json", data).await?;

        Ok(())
    }

    pub async fn get_pcr_values(&self, pcrs: &[u32]) -> HashMap<u32, String> {
        let log = self.log.read().await;
        let mut result = HashMap::new();

        for pcr in pcrs {
            // Find last entry for this PCR
            if let Some(entry) = log.entries.iter().rev().find(|e| e.pcr == *pcr) {
                result.insert(*pcr, entry.digest.clone());
            }
        }

        result
    }

    pub async fn load_evm_log(&self) -> Result<()> {
        if !self.config.evm_log_path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&self.config.evm_log_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut entries = Vec::new();

        while let Some(line) = lines.next_line().await? {
            if let Some(entry) = self.parse_evm_line(&line) {
                entries.push(entry);
            }
        }

        *self.evm_log.write().await = entries;
        info!("Loaded {} EVM entries", entries.len());
        Ok(())
    }

    fn parse_evm_line(&self, line: &str) -> Option<EvmEntry> {
        // EVM log format similar to IMA
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            return None;
        }

        Some(EvmEntry {
            pcr: parts[0].parse().ok()?,
            digest: parts[1].to_string(),
            template: parts[2].to_string(),
            template_data: parts[3].to_string(),
            xattr_name: parts[4].to_string(),
            xattr_value: parts[5].to_string(),
            timestamp: Utc::now(),
        })
    }

    pub async fn get_evm_entries(&self) -> Vec<EvmEntry> {
        self.evm_log.read().await.clone()
    }
}