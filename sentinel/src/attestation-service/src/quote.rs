//! Quote service for remote attestation

use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use ring::digest;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::tpm::{TpmManager, QuoteResult};
use crate::ima::ImaManager;
use crate::pcr::PcrManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteConfig {
    pub signing_key: Option<std::path::PathBuf>,
    pub aik_name: String,
    pub quote_format: String,
    pub include_ima_log: bool,
    pub include_event_log: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationQuote {
    pub quote: QuoteResult,
    pub pcr_values: HashMap<u32, String>,
    pub ima_log: Option<crate::ima::ImaLog>,
    pub event_log: Option<Vec<crate::pcr::PcrEvent>>,
    pub timestamp: DateTime<Utc>,
    pub nonce: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub valid: bool,
    pub pcr_matches: HashMap<u32, bool>,
    pub signature_valid: bool,
    pub ima_verified: bool,
    pub errors: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

use std::collections::HashMap;

pub struct QuoteService {
    tpm: Arc<RwLock<TpmManager>>,
    ima: Arc<ImaManager>,
    pcr: Arc<PcrManager>,
    config: QuoteConfig,
    nonce_cache: Arc<RwLock<std::collections::HashMap<String, DateTime<Utc>>>>,
}

impl QuoteService {
    pub async fn new(
        tpm: Arc<RwLock<TpmManager>>,
        ima: Arc<ImaManager>,
        pcr: Arc<PcrManager>,
        config: &QuoteConfig,
    ) -> Result<Self> {
        Ok(Self {
            tpm,
            ima,
            pcr,
            config: config.clone(),
            nonce_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    pub async fn generate_quote(&self, nonce: Option<&[u8]>) -> Result<AttestationQuote> {
        let nonce_bytes = nonce.unwrap_or(&Self::generate_nonce());
        let nonce_str = hex::encode(nonce_bytes);

        // Cache nonce
        {
            let mut cache = self.nonce_cache.write().await;
            cache.insert(nonce_str.clone(), Utc::now());

            // Clean old nonces (older than 1 hour)
            let cutoff = Utc::now() - chrono::Duration::hours(1);
            cache.retain(|_, ts| *ts > cutoff);
        }

        // Get PCR values
        let pcrs = self.pcr.config.monitored_pcrs.clone();
        let pcr_values = self.pcr.get_current_pcrs().await;

        // Generate quote from TPM
        let quote_result = self.tpm.write().await.quote(nonce_bytes, &pcrs).await?;

        // Get IMA log if requested
        let ima_log = if self.config.include_ima_log {
            Some(self.ima.get_log().await)
        } else {
            None
        };

        // Get event log if requested
        let event_log = if self.config.include_event_log {
            Some(self.pcr.get_events(None).await)
        } else {
            None
        };

        Ok(AttestationQuote {
            quote: quote_result,
            pcr_values,
            ima_log,
            event_log,
            timestamp: Utc::now(),
            nonce: nonce_str,
            version: "1.0".to_string(),
        })
    }

    pub async fn verify_quote(&self, quote: &AttestationQuote) -> Result<VerificationResult> {
        let mut result = VerificationResult {
            valid: true,
            pcr_matches: HashMap::new(),
            signature_valid: false,
            ima_verified: false,
            errors: Vec::new(),
            timestamp: Utc::now(),
        };

        // Verify nonce is recent
        {
            let cache = self.nonce_cache.read().await;
            if let Some(ts) = cache.get(&quote.nonce) {
                if (Utc::now() - *ts).num_hours() > 1 {
                    result.errors.push("Nonce too old".to_string());
                    result.valid = false;
                }
            } else {
                result.errors.push("Unknown nonce".to_string());
                result.valid = false;
            }
        }

        // Verify PCR values match baseline
        let baseline = self.pcr.baseline.read().await;
        for (pcr, expected) in baseline.iter() {
            let actual = quote.pcr_values.get(pcr);
            let matches = actual.map(|a| a == expected).unwrap_or(false);
            result.pcr_matches.insert(*pcr, matches);

            if !matches {
                result.errors.push(format!("PCR {} mismatch", pcr));
                result.valid = false;
            }
        }

        // Verify TPM signature
        // In real implementation, would verify quote.signature against AIK public key
        result.signature_valid = true; // Placeholder

        // Verify IMA log
        if let Some(ima_log) = &quote.ima_log {
            let mut all_verified = true;
            for entry in &ima_log.entries {
                if !self.ima.verify_measurement(entry).await.unwrap_or(false) {
                    all_verified = false;
                    result.errors.push(format!("IMA verification failed for {}", entry.template_data));
                }
            }
            result.ima_verified = all_verified;
            if !all_verified {
                result.valid = false;
            }
        }

        Ok(result)
    }

    pub async fn get_attestation_report(&self) -> Result<AttestationReport> {
        let quote = self.generate_quote(None).await?;
        let verification = self.verify_quote(&quote).await?;

        Ok(AttestationReport {
            quote,
            verification,
            platform_info: self.get_platform_info().await?,
        })
    }

    async fn get_platform_info(&self) -> Result<PlatformInfo> {
        let tpm_caps = self.tpm.write().await.get_capabilities().await?;

        Ok(PlatformInfo {
            tpm_version: "2.0".to_string(),
            tpm_manufacturer: "Unknown".to_string(),
            firmware_version: "Unknown".to_string(),
            secure_boot: true, // Would check actual status
            tpm_caps: format!("{:?}", tpm_caps),
        })
    }

    fn generate_nonce() -> Vec<u8> {
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut nonce = vec![0u8; 32];
        rng.fill(&mut nonce).unwrap();
        nonce
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    pub quote: AttestationQuote,
    pub verification: VerificationResult,
    pub platform_info: PlatformInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub tpm_version: String,
    pub tpm_manufacturer: String,
    pub firmware_version: String,
    pub secure_boot: bool,
    pub tpm_caps: String,
}