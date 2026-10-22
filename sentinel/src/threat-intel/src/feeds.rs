//! Threat feed manager

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use maxminddb::Reader;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSource {
    pub name: String,
    pub url: String,
    pub format: String, // stix, misp, json, csv
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedsConfig {
    pub enabled: bool,
    pub update_interval_secs: u64,
    pub sources: Vec<FeedSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatFeed {
    pub source: String,
    pub indicators: Vec<ThreatIndicator>,
    pub last_updated: DateTime<Utc>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIndicator {
    pub id: String,
    pub indicator_type: String, // ip, domain, hash, url, email
    pub value: String,
    pub threat_types: Vec<String>,
    pub confidence: u8, // 0-100
    pub severity: String, // low, medium, high, critical
    pub description: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub tags: Vec<String>,
    pub mitre_techniques: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoIPInfo {
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub asn: Option<u32>,
    pub asn_name: Option<String>,
    pub is_anonymous: bool,
    pub is_tor: bool,
}

pub struct FeedManager {
    config: FeedsConfig,
    client: Client,
    feeds: Arc<RwLock<HashMap<String, ThreatFeed>>>,
    ip_cache: Arc<RwLock<HashMap<String, GeoIPInfo>>>,
    geoip_reader: Option<Reader<Vec<u8>>>,
}

impl FeedManager {
    pub async fn new(config: &FeedsConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        // Try to load GeoIP database
        let geoip_reader = if let Ok(data) = tokio::fs::read("/usr/share/GeoIP/GeoLite2-City.mmdb").await {
            Some(Reader::from_source(data)?)
        } else if let Ok(data) = tokio::fs::read("/var/lib/sentinel/geoip/GeoLite2-City.mmdb").await {
            Some(Reader::from_source(data)?)
        } else {
            None
        };

        Ok(Self {
            config: config.clone(),
            client,
            feeds: Arc::new(RwLock::new(HashMap::new())),
            ip_cache: Arc::new(RwLock::new(HashMap::new())),
            geoip_reader,
        })
    }

    pub async fn start_updates(&self) {
        if !self.config.enabled {
            info!("Threat feeds disabled");
            return;
        }

        let mut interval = interval(Duration::from_secs(self.config.update_interval_secs));

        loop {
            interval.tick().await;
            if let Err(e) = self.update_all_feeds().await {
                warn!("Feed update failed: {}", e);
            }
        }
    }

    pub async fn update_all_feeds(&self) -> Result<()> {
        for source in &self.config.sources {
            if let Err(e) = self.update_feed(source).await {
                warn!("Failed to update feed {}: {}", source.name, e);
            }
        }
        Ok(())
    }

    async fn update_feed(&self, source: &FeedSource) -> Result<()> {
        info!("Updating threat feed: {}", source.name);

        let mut request = self.client.get(&source.url);

        if let Some(key) = &source.api_key {
            request = request.bearer_auth(key);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Feed request failed: {}", response.status()));
        }

        let content = response.text().await?;
        let indicators = self.parse_feed(&source.format, &content, &source.name).await?;

        let feed = ThreatFeed {
            source: source.name.clone(),
            indicators,
            last_updated: Utc::now(),
            version: Utc::now().timestamp().to_string(),
        };

        {
            let mut feeds = self.feeds.write().await;
            feeds.insert(source.name.clone(), feed);
        }

        info!("Updated feed {} with {} indicators", source.name,
            self.feeds.read().await.get(&source.name).map(|f| f.indicators.len()).unwrap_or(0));

        Ok(())
    }

    async fn parse_feed(&self, format: &str, content: &str, source: &str) -> Result<Vec<ThreatIndicator>> {
        match format {
            "json" => self.parse_json(content, source),
            "stix" => self.parse_stix(content, source),
            "misp" => self.parse_misp(content, source),
            "csv" => self.parse_csv(content, source),
            _ => Err(anyhow::anyhow!("Unsupported feed format: {}", format)),
        }
    }

    fn parse_json(&self, content: &str, source: &str) -> Result<Vec<ThreatIndicator>> {
        let val: serde_json::Value = serde_json::from_str(content)?;
        let mut indicators = Vec::new();

        // Handle different JSON structures
        if let Some(arr) = val.as_array() {
            for item in arr {
                if let Some(ind) = self.extract_indicator(item, source) {
                    indicators.push(ind);
                }
            }
        } else if let Some(obj) = val.as_object() {
            // Try common structures
            if let Some(pulses) = obj.get("pulses").and_then(|v| v.as_array()) {
                for pulse in pulses {
                    if let Some(arr) = pulse.get("indicators").and_then(|v| v.as_array()) {
                        for ind in arr {
                            if let Some(i) = self.extract_indicator(ind, source) {
                                indicators.push(i);
                            }
                        }
                    }
                }
            }
        }

        Ok(indicators)
    }

    fn extract_indicator(&self, val: &serde_json::Value, source: &str) -> Option<ThreatIndicator> {
        let indicator_type = val.get("type")?.as_str()?;
        let value = val.get("value")?.as_str()?;

        Some(ThreatIndicator {
            id: val.get("id").and_then(|v| v.as_str()).unwrap_or(&Uuid::new_v4().to_string()).to_string(),
            indicator_type: indicator_type.to_string(),
            value: value.to_string(),
            threat_types: val.get("threat_types").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            confidence: val.get("confidence").and_then(|v| v.as_u64()).unwrap_or(50) as u8,
            severity: val.get("severity").and_then(|v| v.as_str()).unwrap_or("medium").to_string(),
            description: val.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()),
            first_seen: val.get("first_seen").and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            last_seen: val.get("last_seen").and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            tags: val.get("tags").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            mitre_techniques: val.get("mitre_techniques").and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default(),
            source: source.to_string(),
        })
    }

    fn parse_stix(&self, _content: &str, _source: &str) -> Result<Vec<ThreatIndicator>> {
        // STIX parsing would use stix crate
        Ok(vec![])
    }

    fn parse_misp(&self, _content: &str, _source: &str) -> Result<Vec<ThreatIndicator>> {
        // MISP parsing
        Ok(vec![])
    }

    fn parse_csv(&self, content: &str, source: &str) -> Result<Vec<ThreatIndicator>> {
        let mut indicators = Vec::new();

        for line in content.lines().skip(1) { // Skip header
            let fields: Vec<&str> = line.split(',').collect();
            if fields.len() >= 3 {
                indicators.push(ThreatIndicator {
                    id: Uuid::new_v4().to_string(),
                    indicator_type: fields[0].to_string(),
                    value: fields[1].to_string(),
                    threat_types: vec![fields.get(2).unwrap_or(&"unknown").to_string()],
                    confidence: 75,
                    severity: "medium".to_string(),
                    description: None,
                    first_seen: Utc::now(),
                    last_seen: Utc::now(),
                    tags: vec![],
                    mitre_techniques: vec![],
                    source: source.to_string(),
                });
            }
        }

        Ok(indicators)
    }

    pub async fn lookup_ip(&self, ip: &str) -> Result<Option<ThreatIndicator>> {
        // Check cache first
        {
            let feeds = self.feeds.read().await;
            for feed in feeds.values() {
                for ind in &feed.indicators {
                    if ind.indicator_type == "ip" && ind.value == ip {
                        return Ok(Some(ind.clone()));
                    }
                }
            }
        }
        Ok(None)
    }

    pub async fn lookup_domain(&self, domain: &str) -> Result<Option<ThreatIndicator>> {
        let feeds = self.feeds.read().await;
        for feed in feeds.values() {
            for ind in &feed.indicators {
                if ind.indicator_type == "domain" && ind.value == domain {
                    return Ok(Some(ind.clone()));
                }
            }
        }
        Ok(None)
    }

    pub async fn lookup_hash(&self, hash: &str) -> Result<Option<ThreatIndicator>> {
        let feeds = self.feeds.read().await;
        for feed in feeds.values() {
            for ind in &feed.indicators {
                if ind.indicator_type == "hash" && ind.value == hash {
                    return Ok(Some(ind.clone()));
                }
            }
        }
        Ok(None)
    }

    pub async fn get_geoip(&self, ip: &str) -> Result<Option<GeoIPInfo>> {
        // Check cache
        {
            let cache = self.ip_cache.read().await;
            if let Some(info) = cache.get(ip) {
                return Ok(Some(info.clone()));
            }
        }

        // Query GeoIP
        if let Some(reader) = &self.geoip_reader {
            // In real implementation, would parse IP and lookup
            // For now, return None
        }

        Ok(None)
    }

    pub async fn get_all_indicators(&self) -> Vec<ThreatIndicator> {
        let feeds = self.feeds.read().await;
        feeds.values().flat_map(|f| f.indicators.clone()).collect()
    }

    pub async fn get_feed_status(&self) -> Vec<serde_json::Value> {
        let feeds = self.feeds.read().await;
        feeds.iter().map(|(name, feed)| {
            serde_json::json!({
                "source": name,
                "indicators": feed.indicators.len(),
                "last_updated": feed.last_updated,
                "version": feed.version,
            })
        }).collect()
    }
}

use uuid::Uuid;