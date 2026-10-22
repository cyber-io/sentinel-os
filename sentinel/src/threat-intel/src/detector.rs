//! Threat detector using ONNX models

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use ndarray::{Array, ArrayD, IxDyn};
use onnxruntime::{environment::Environment, GraphOptimizationLevel, LoggingLevel, Session, Tensor};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::feeds::ThreatFeed;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub path: PathBuf,
    pub input_shape: Vec<usize>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorConfig {
    pub models: Vec<ModelConfig>,
    pub threshold: f32,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub model: String,
    pub label: String,
    pub confidence: f32,
    pub features: HashMap<String, f32>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkFlow {
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: String,
    pub bytes: u64,
    pub packets: u64,
    pub duration_ms: u64,
    pub flags: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFeatures {
    pub hash: String,
    pub size: u64,
    pub entropy: f32,
    pub imports: Vec<String>,
    pub sections: Vec<SectionInfo>,
    pub strings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionInfo {
    pub name: String,
    pub virtual_size: u64,
    pub raw_size: u64,
    pub entropy: f32,
    pub characteristics: u32,
}

pub struct ThreatDetector {
    config: DetectorConfig,
    sessions: HashMap<String, Session>,
    environment: Arc<Environment>,
}

impl ThreatDetector {
    pub async fn new(config: &DetectorConfig) -> Result<Self> {
        let environment = Arc::new(
            Environment::builder()
                .with_name("sentinel-threat-detector")
                .with_log_level(LoggingLevel::Warning)
                .build()?
        );

        let mut sessions = HashMap::new();

        for model_config in &config.models {
            let session = Session::builder(&environment)?
                .with_optimization_level(GraphOptimizationLevel::All)?
                .with_intra_threads(4)?
                .commit_from_file(&model_config.path)?;

            sessions.insert(model_config.name.clone(), session);
            info!("Loaded model: {} from {}", model_config.name, model_config.path.display());
        }

        Ok(Self {
            config: config.clone(),
            sessions,
            environment,
        })
    }

    pub async fn detect_network(&self, flow: &NetworkFlow) -> Result<Vec<DetectionResult>> {
        let features = self.extract_network_features(flow);
        self.run_detection("network-ids", &features).await
    }

    pub async fn detect_file(&self, file: &FileFeatures) -> Result<Vec<DetectionResult>> {
        let features = self.extract_file_features(file);
        self.run_detection("malware-classifier", &features).await
    }

    pub async fn detect_log(&self, log: &str) -> Result<Vec<DetectionResult>> {
        // Log-based detection would use a different model
        Ok(vec![])
    }

    async fn run_detection(&self, model_name: &str, features: &[f32]) -> Result<Vec<DetectionResult>> {
        let session = self.sessions.get(model_name)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_name))?;

        let model_config = self.config.models.iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| anyhow::anyhow!("Model config not found"))?;

        // Prepare input tensor
        let input_shape: Vec<i64> = model_config.input_shape.iter().map(|&x| x as i64).collect();
        let input_tensor = Array::from_shape_vec(IxDyn(&input_shape), features.to_vec())?;

        // Run inference
        let input_tensor_ort = Tensor::from_array(input_tensor)?;
        let outputs = session.run(vec![input_tensor_ort])?;

        // Process outputs
        let output = &outputs[0];
        let output_array = output.try_extract_tensor::<f32>()?;
        let probabilities = output_array.as_slice().unwrap();

        // Find top predictions
        let mut results = Vec::new();
        for (i, &prob) in probabilities.iter().enumerate() {
            if prob >= self.config.threshold && i < model_config.labels.len() {
                results.push(DetectionResult {
                    model: model_name.to_string(),
                    label: model_config.labels[i].clone(),
                    confidence: prob,
                    features: HashMap::new(), // Would include feature importance
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        // Sort by confidence
        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        Ok(results)
    }

    fn extract_network_features(&self, flow: &NetworkFlow) -> Vec<f32> {
        // Extract 78 features for network IDS model
        let mut features = Vec::with_capacity(78);

        // Basic flow features
        features.push(flow.src_port as f32 / 65535.0);
        features.push(flow.dst_port as f32 / 65535.0);
        features.push(match flow.protocol.as_str() {
            "tcp" => 0.0,
            "udp" => 1.0,
            "icmp" => 2.0,
            _ => 3.0,
        });
        features.push(flow.bytes as f32);
        features.push(flow.packets as f32);
        features.push(flow.duration_ms as f32);

        // IP-based features (simplified)
        features.push(self.ip_to_float(&flow.src_ip));
        features.push(self.ip_to_float(&flow.dst_ip));

        // TCP flags
        features.push(if flow.flags.contains('S') { 1.0 } else { 0.0 });
        features.push(if flow.flags.contains('A') { 1.0 } else { 0.0 });
        features.push(if flow.flags.contains('F') { 1.0 } else { 0.0 });
        features.push(if flow.flags.contains('R') { 1.0 } else { 0.0 });
        features.push(if flow.flags.contains('P') { 1.0 } else { 0.0 });
        features.push(if flow.flags.contains('U') { 1.0 } else { 0.0 });

        // Derived features
        features.push(if flow.duration_ms > 0 { flow.bytes as f32 / flow.duration_ms as f32 } else { 0.0 });
        features.push(if flow.packets > 0 { flow.bytes as f32 / flow.packets as f32 } else { 0.0 });

        // Pad to 78 features
        while features.len() < 78 {
            features.push(0.0);
        }
        features.truncate(78);

        features
    }

    fn extract_file_features(&self, file: &FileFeatures) -> Vec<f32> {
        // Extract 2381 features for malware classifier (EMBER-style)
        let mut features = Vec::with_capacity(2381);

        // General file info
        features.push(file.size as f32);
        features.push(file.entropy);

        // Import features (hashed)
        for imp in &file.imports {
            features.push(self.hash_string(imp) as f32 / u32::MAX as f32);
        }
        while features.len() < 100 {
            features.push(0.0);
        }

        // Section features
        for sec in &file.sections {
            features.push(sec.virtual_size as f32);
            features.push(sec.raw_size as f32);
            features.push(sec.entropy);
            features.push(sec.characteristics as f32);
        }
        while features.len() < 200 {
            features.push(0.0);
        }

        // String features (hashed)
        for s in &file.strings {
            features.push(self.hash_string(s) as f32 / u32::MAX as f32);
        }
        while features.len() < 2381 {
            features.push(0.0);
        }
        features.truncate(2381);

        features
    }

    fn ip_to_float(&self, ip: &str) -> f32 {
        // Simple IP to float conversion
        ip.split('.')
            .filter_map(|o| o.parse::<u32>().ok())
            .fold(0u32, |acc, octet| (acc << 8) | octet) as f32 / u32::MAX as f32
    }

    fn hash_string(&self, s: &str) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish() as u32
    }
}