//! Threat explainer using LLM

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::detector::{DetectionResult, NetworkFlow, FileFeatures};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainerConfig {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    pub summary: String,
    pub technical_details: String,
    pub impact_assessment: String,
    pub recommended_actions: Vec<String>,
    pub mitre_techniques: Vec<String>,
    pub confidence: f32,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

pub struct ThreatExplainer {
    config: ExplainerConfig,
    client: Client,
    gateway_url: String,
    cache: Arc<RwLock<HashMap<String, Explanation>>>,
}

impl ThreatExplainer {
    pub async fn new(config: &ExplainerConfig, gateway_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Self {
            config: config.clone(),
            client,
            gateway_url,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn explain_detection(
        &self,
        detection: &DetectionResult,
        context: Option<&str>,
    ) -> Result<Explanation> {
        let cache_key = format!("{}:{}:{:.2}", detection.model, detection.label, detection.confidence);

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let prompt = self.build_prompt(detection, context);

        let request = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "system": self.system_prompt(),
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false,
        });

        let response = self.client
            .post(&format!("{}/api/generate", self.gateway_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("LLM request failed: {}", response.status()));
        }

        let data: serde_json::Value = response.json().await?;
        let text = data["response"].as_str().unwrap_or("").to_string();

        let explanation = self.parse_explanation(&text, detection.confidence);

        // Cache result
        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key, explanation.clone());
        }

        Ok(explanation)
    }

    pub async fn explain_network_flow(
        &self,
        flow: &NetworkFlow,
        detections: &[DetectionResult],
    ) -> Result<Explanation> {
        let prompt = self.build_network_prompt(flow, detections);

        let request = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "system": self.network_system_prompt(),
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false,
        });

        let response = self.client
            .post(&format!("{}/api/generate", self.gateway_url))
            .json(&request)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        let text = data["response"].as_str().unwrap_or("").to_string();

        let mut explanation = self.parse_explanation(&text,
            detections.iter().map(|d| d.confidence).fold(0.0, f32::max));
        explanation.summary = format!("Network flow analysis: {} -> {}", flow.src_ip, flow.dst_ip);

        Ok(explanation)
    }

    pub async fn explain_file(
        &self,
        file: &FileFeatures,
        detections: &[DetectionResult],
    ) -> Result<Explanation> {
        let prompt = self.build_file_prompt(file, detections);

        let request = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "system": self.file_system_prompt(),
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false,
        });

        let response = self.client
            .post(&format!("{}/api/generate", self.gateway_url))
            .json(&request)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        let text = data["response"].as_str().unwrap_or("").to_string();

        let mut explanation = self.parse_explanation(&text,
            detections.iter().map(|d| d.confidence).fold(0.0, f32::max));
        explanation.summary = format!("File analysis: {} ({} bytes)", file.hash, file.size);

        Ok(explanation)
    }

    fn system_prompt(&self) -> String {
        r#"You are a senior cybersecurity analyst for Sentinel OS.
Explain threat detections clearly and actionably.
Focus on: what was detected, why it matters, what to do.
Use structured format with clear sections.
Be concise but thorough."#.to_string()
    }

    fn network_system_prompt(&self) -> String {
        r#"You are a network security analyst.
Analyze network flows for malicious activity.
Identify: C2 communication, data exfil, lateral movement, scanning.
Reference MITRE ATT&CK techniques.
Provide immediate actionable recommendations."#.to_string()
    }

    fn file_system_prompt(&self) -> String {
        r#"You are a malware analyst.
Analyze file characteristics for malicious behavior.
Identify: malware family, capabilities, persistence, C2.
Reference MITRE ATT&CK techniques.
Provide containment and remediation steps."#.to_string()
    }

    fn build_prompt(&self, detection: &DetectionResult, context: Option<&str>) -> String {
        format!(r#"Threat Detection Alert:
Model: {}
Label: {}
Confidence: {:.2}%
Context: {}

Provide structured explanation:
1. SUMMARY: One-sentence description
2. TECHNICAL DETAILS: What triggered this detection
3. IMPACT ASSESSMENT: Potential business/operational impact
4. RECOMMENDED ACTIONS: Specific steps to take (numbered)
5. MITRE TECHNIQUES: Relevant ATT&CK technique IDs

Format as JSON."#,
            detection.model,
            detection.label,
            detection.confidence * 100.0,
            context.unwrap_or("No additional context")
        )
    }

    fn build_network_prompt(&self, flow: &NetworkFlow, detections: &[DetectionResult]) -> String {
        format!(r#"Network Flow Analysis:
Source: {}:{} -> Destination: {}:{}
Protocol: {} | Bytes: {} | Packets: {} | Duration: {}ms
Flags: {}

Detections:
{}

Provide structured network threat analysis:
1. SUMMARY: What this flow represents
2. TECHNICAL DETAILS: Flow characteristics and detection triggers
3. IMPACT ASSESSMENT: Risk level and potential impact
4. RECOMMENDED ACTIONS: Immediate and follow-up actions
5. MITRE TECHNIQUES: ATT&CK technique IDs

Format as JSON."#,
            flow.src_ip, flow.src_port,
            flow.dst_ip, flow.dst_port,
            flow.protocol, flow.bytes, flow.packets, flow.duration_ms,
            flow.flags,
            detections.iter().map(|d| format!("- {}: {:.2}%", d.label, d.confidence * 100.0)).collect::<Vec<_>>().join("\n")
        )
    }

    fn build_file_prompt(&self, file: &FileFeatures, detections: &[DetectionResult]) -> String {
        format!(r#"File Analysis:
Hash: {}
Size: {} bytes
Entropy: {:.2}
Imports: {}
Sections: {}
Strings (sample): {}

Detections:
{}

Provide structured malware analysis:
1. SUMMARY: Likely malware family/category
2. TECHNICAL DETAILS: Static analysis findings
3. IMPACT ASSESSMENT: Capabilities and risk
4. RECOMMENDED ACTIONS: Containment, eradication, recovery
5. MITRE TECHNIQUES: ATT&CK technique IDs

Format as JSON."#,
            file.hash,
            file.size,
            file.entropy,
            file.imports.len(),
            file.sections.len(),
            file.strings.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
            detections.iter().map(|d| format!("- {}: {:.2}%", d.label, d.confidence * 100.0)).collect::<Vec<_>>().join("\n")
        )
    }

    fn parse_explanation(&self, text: &str, default_confidence: f32) -> Explanation {
        // Try to parse JSON from response
        if let Some(start) = text.find('{') {
            if let Some(end) = text.rfind('}') {
                let json_str = &text[start..=end];
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    return Explanation {
                        summary: val["summary"].as_str().unwrap_or("Analysis complete").to_string(),
                        technical_details: val["technical_details"].as_str().unwrap_or("").to_string(),
                        impact_assessment: val["impact_assessment"].as_str().unwrap_or("").to_string(),
                        recommended_actions: val["recommended_actions"].as_array()
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default(),
                        mitre_techniques: val["mitre_techniques"].as_array()
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default(),
                        confidence: val["confidence"].as_f64().unwrap_or(default_confidence as f64) as f32,
                        generated_at: chrono::Utc::now(),
                    };
                }
            }
        }

        // Fallback: parse as plain text
        Explanation {
            summary: "Analysis complete".to_string(),
            technical_details: text.to_string(),
            impact_assessment: "Assessment pending".to_string(),
            recommended_actions: vec!["Review detection manually".to_string()],
            mitre_techniques: vec![],
            confidence: default_confidence,
            generated_at: chrono::Utc::now(),
        }
    }
}