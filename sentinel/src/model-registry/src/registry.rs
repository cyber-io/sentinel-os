//! Model registry core logic

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use oras::client::{Client, ClientConfig};
use oras::registry::Reference;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::storage::StorageBackend;
use crate::verification::VerificationService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub name: String,
    pub tag: String,
    pub digest: String,
    pub size_bytes: u64,
    pub architecture: String,
    pub os: String,
    pub created_at: DateTime<Utc>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub layers: Vec<LayerInfo>,
    pub config: ConfigInfo,
    pub signatures: Vec<SignatureInfo>,
    pub sbom: Option<SbomInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInfo {
    pub digest: String,
    pub size_bytes: u64,
    pub media_type: String,
    pub annotations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub digest: String,
    pub size_bytes: u64,
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub digest: String,
    pub algorithm: String,
    pub public_key: String,
    pub signed_at: DateTime<Utc>,
    pub signer: String,
    pub certificate: Option<String>,
    pub rekor_entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomInfo {
    pub format: String,
    pub digest: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelIndex {
    pub models: HashMap<String, Vec<ModelManifest>>, // name -> list of tags
    pub updated_at: DateTime<Utc>,
}

pub struct ModelRegistry {
    storage: Arc<StorageBackend>,
    verification: Arc<VerificationService>,
    oras_client: Option<Client>,
    oras_config: OrasConfig,
    index: Arc<RwLock<ModelIndex>>,
    local_cache: Arc<DashMap<String, ModelManifest>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrasConfig {
    pub registry: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub insecure: bool,
}

impl ModelRegistry {
    pub async fn new(
        storage: StorageBackend,
        verification: VerificationService,
        oras_config: OrasConfig,
    ) -> Result<Self> {
        let storage = Arc::new(storage);
        let verification = Arc::new(verification);

        // Initialize ORAS client
        let oras_client = if !oras_config.registry.is_empty() {
            let mut config = ClientConfig::default();
            config.insecure = oras_config.insecure;

            if let (Some(user), Some(pass)) = (&oras_config.username, &oras_config.password) {
                config.auth = Some(oras::client::Auth::Basic(user.clone(), pass.clone()));
            }

            Some(Client::new(config))
        } else {
            None
        };

        let registry = Self {
            storage,
            verification,
            oras_client,
            oras_config,
            index: Arc::new(RwLock::new(ModelIndex {
                models: HashMap::new(),
                updated_at: Utc::now(),
            })),
            local_cache: Arc::new(DashMap::new()),
        };

        // Load existing index
        registry.load_index().await?;

        Ok(registry)
    }

    async fn load_index(&self) -> Result<()> {
        if let Ok(data) = self.storage.read_json("index.json").await {
            let index: ModelIndex = serde_json::from_slice(&data)?;
            *self.index.write().await = index;
            info!("Loaded model index with {} models", index.models.len());
        }
        Ok(())
    }

    async fn save_index(&self) -> Result<()> {
        let index = self.index.read().await.clone();
        let data = serde_json::to_vec_pretty(&index)?;
        self.storage.write_json("index.json", &data).await?;
        Ok(())
    }

    pub async fn push(&self, model_path: &PathBuf, name: &str, tag: &str, sign: bool) -> Result<ModelManifest> {
        // Read model files
        let layers = self.prepare_layers(model_path).await?;
        let config = self.prepare_config(model_path).await?;

        // Create manifest
        let mut manifest = ModelManifest {
            name: name.to_string(),
            tag: tag.to_string(),
            digest: String::new(), // Will be computed
            size_bytes: layers.iter().map(|l| l.size_bytes).sum(),
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            created_at: Utc::now(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            layers,
            config,
            signatures: Vec::new(),
            sbom: None,
        };

        // Compute digest
        manifest.digest = self.compute_digest(&manifest).await?;

        // Sign if requested
        if sign {
            let signature = self.verification.sign(&manifest).await?;
            manifest.signatures.push(signature);
        }

        // Store locally
        self.storage.write_model(&manifest).await?;

        // Update index
        {
            let mut index = self.index.write().await;
            index.models.entry(name.to_string()).or_default().push(manifest.clone());
            index.updated_at = Utc::now();
        }
        self.save_index().await?;

        // Push to ORAS registry if configured
        if let Some(client) = &self.oras_client {
            self.push_to_registry(client, &manifest).await?;
        }

        self.local_cache.insert(format!("{}:{}", name, tag), manifest.clone());

        info!("Pushed model {}:{}", name, tag);
        Ok(manifest)
    }

    pub async fn pull(&self, name: &str, tag: &str, verify: bool) -> Result<ModelManifest> {
        let key = format!("{}:{}", name, tag);

        // Check local cache
        if let Some(manifest) = self.local_cache.get(&key) {
            return Ok(manifest.clone());
        }

        // Check index
        {
            let index = self.index.read().await;
            if let Some(models) = index.models.get(name) {
                if let Some(manifest) = models.iter().find(|m| m.tag == tag) {
                    self.local_cache.insert(key.clone(), manifest.clone());
                    return Ok(manifest.clone());
                }
            }
        }

        // Pull from ORAS registry
        if let Some(client) = &self.oras_client {
            let manifest = self.pull_from_registry(client, name, tag).await?;

            if verify {
                self.verification.verify(&manifest).await?;
            }

            // Store locally
            self.storage.write_model(&manifest).await?;
            self.local_cache.insert(key, manifest.clone());

            // Update index
            {
                let mut index = self.index.write().await;
                index.models.entry(name.to_string()).or_default().push(manifest.clone());
                index.updated_at = Utc::now();
            }
            self.save_index().await?;

            return Ok(manifest);
        }

        Err(anyhow::anyhow!("Model not found locally and no registry configured"))
    }

    pub async fn list(&self) -> Vec<ModelManifest> {
        let index = self.index.read().await;
        index.models.values().flatten().cloned().collect()
    }

    pub async fn get(&self, name: &str, tag: &str) -> Option<ModelManifest> {
        let key = format!("{}:{}", name, tag);

        if let Some(manifest) = self.local_cache.get(&key) {
            return Some(manifest.clone());
        }

        let index = self.index.read().await;
        index.models.get(name)
            .and_then(|models| models.iter().find(|m| m.tag == tag).cloned())
    }

    pub async fn delete(&self, name: &str, tag: &str) -> Result<()> {
        let key = format!("{}:{}", name, tag);

        // Remove from local cache
        self.local_cache.remove(&key);

        // Remove from storage
        self.storage.delete_model(name, tag).await?;

        // Update index
        {
            let mut index = self.index.write().await;
            if let Some(models) = index.models.get_mut(name) {
                models.retain(|m| m.tag != tag);
                if models.is_empty() {
                    index.models.remove(name);
                }
            }
            index.updated_at = Utc::now();
        }
        self.save_index().await?;

        // Delete from ORAS registry if configured
        if let Some(client) = &self.oras_client {
            self.delete_from_registry(client, name, tag).await?;
        }

        info!("Deleted model {}:{}", name, tag);
        Ok(())
    }

    pub async fn verify(&self, name: &str, tag: &str) -> Result<bool> {
        if let Some(manifest) = self.get(name, tag).await {
            self.verification.verify(&manifest).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn sign(&self, name: &str, tag: &str) -> Result<ModelManifest> {
        let mut manifest = self.get(name, tag).await
            .ok_or_else(|| anyhow::anyhow!("Model not found"))?;

        let signature = self.verification.sign(&manifest).await?;
        manifest.signatures.push(signature);

        // Update stored manifest
        self.storage.write_model(&manifest).await?;
        self.local_cache.insert(format!("{}:{}", name, tag), manifest.clone());

        // Update index
        {
            let mut index = self.index.write().await;
            if let Some(models) = index.models.get_mut(name) {
                if let Some(m) = models.iter_mut().find(|m| m.tag == tag) {
                    *m = manifest.clone();
                }
            }
        }
        self.save_index().await?;

        // Push updated manifest to registry
        if let Some(client) = &self.oras_client {
            self.push_to_registry(client, &manifest).await?;
        }

        Ok(manifest)
    }

    async fn prepare_layers(&self, model_path: &PathBuf) -> Result<Vec<LayerInfo>> {
        // In real implementation, would read model files and create layers
        // For now, return a placeholder
        Ok(vec![LayerInfo {
            digest: "sha256:placeholder".to_string(),
            size_bytes: 1024 * 1024 * 1024, // 1GB
            media_type: "application/vnd.sentinel.model.layer".to_string(),
            annotations: HashMap::new(),
        }])
    }

    async fn prepare_config(&self, _model_path: &PathBuf) -> Result<ConfigInfo> {
        Ok(ConfigInfo {
            digest: "sha256:config".to_string(),
            size_bytes: 1024,
            media_type: "application/vnd.sentinel.model.config".to_string(),
        })
    }

    async fn compute_digest(&self, manifest: &ModelManifest) -> Result<String> {
        // Compute SHA256 of manifest JSON
        let data = serde_json::to_vec(manifest)?;
        let digest = ring::digest::digest(&ring::digest::SHA256, &data);
        Ok(format!("sha256:{}", hex::encode(digest)))
    }

    async fn push_to_registry(&self, client: &Client, manifest: &ModelManifest) -> Result<()> {
        let reference = Reference::try_from(format!("{}/{}:{}",
            self.oras_config.registry, manifest.name, manifest.tag))?;

        // In real implementation, would push layers and manifest
        debug!("Would push to registry: {}", reference);
        Ok(())
    }

    async fn pull_from_registry(&self, client: &Client, name: &str, tag: &str) -> Result<ModelManifest> {
        let reference = Reference::try_from(format!("{}/{}:{}",
            self.oras_config.registry, name, tag))?;

        // In real implementation, would pull from registry
        debug!("Would pull from registry: {}", reference);

        // Return placeholder
        Ok(ModelManifest {
            name: name.to_string(),
            tag: tag.to_string(),
            digest: "sha256:placeholder".to_string(),
            size_bytes: 1024 * 1024 * 1024,
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            created_at: Utc::now(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            layers: vec![],
            config: ConfigInfo {
                digest: "sha256:config".to_string(),
                size_bytes: 1024,
                media_type: "application/vnd.sentinel.model.config".to_string(),
            },
            signatures: vec![],
            sbom: None,
        })
    }

    async fn delete_from_registry(&self, client: &Client, name: &str, tag: &str) -> Result<()> {
        let reference = Reference::try_from(format!("{}/{}:{}",
            self.oras_config.registry, name, tag))?;

        debug!("Would delete from registry: {}", reference);
        Ok(())
    }
}