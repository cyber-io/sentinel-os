//! Storage backend for model registry

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct StorageConfig {
    pub backend: String,
    pub path: PathBuf,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
}

#[async_trait]
pub trait Storage: Send + Sync {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
}

pub struct StorageBackend {
    inner: Arc<dyn Storage>,
}

impl StorageBackend {
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let inner: Arc<dyn Storage> = match config.backend.as_str() {
            "fs" => Arc::new(FileSystemStorage::new(config.path.clone()).await?),
            "s3" => {
                if let (Some(bucket), Some(region)) = (&config.s3_bucket, &config.s3_region) {
                    Arc::new(S3Storage::new(bucket.clone(), region.clone()).await?)
                } else {
                    return Err(anyhow::anyhow!("S3 backend requires bucket and region"));
                }
            }
            "memory" => Arc::new(MemoryStorage::new()),
            _ => return Err(anyhow::anyhow!("Unknown storage backend: {}", config.backend)),
        };

        Ok(Self { inner })
    }

    pub async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        self.inner.write(path, data).await
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.inner.read(path).await
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        self.inner.delete(path).await
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        self.inner.exists(path).await
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        self.inner.list(prefix).await
    }

    pub async fn write_json<T: Serialize>(&self, path: &str, data: &T) -> Result<()> {
        let json = serde_json::to_vec_pretty(data)?;
        self.write(path, &json).await
    }

    pub async fn read_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let data = self.read(path).await?;
        Ok(serde_json::from_slice(&data)?)
    }

    pub async fn write_model(&self, manifest: &crate::registry::ModelManifest) -> Result<()> {
        let path = format!("models/{}/{}/manifest.json", manifest.name, manifest.tag);
        self.write_json(&path, manifest).await
    }

    pub async fn delete_model(&self, name: &str, tag: &str) -> Result<()> {
        let path = format!("models/{}/{}/", name, tag);
        self.delete(&path).await
    }
}

struct FileSystemStorage {
    root: PathBuf,
}

impl FileSystemStorage {
    async fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    fn full_path(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

#[async_trait]
impl Storage for FileSystemStorage {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let full = self.full_path(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(full, data).await?;
        Ok(())
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.full_path(path);
        Ok(fs::read(full).await?)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full = self.full_path(path);
        if full.exists() {
            fs::remove_dir_all(full).await?;
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.full_path(path).exists())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let full = self.full_path(prefix);

        if full.exists() {
            let mut entries = fs::read_dir(full).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if let Ok(rel) = path.strip_prefix(&self.root) {
                    results.push(rel.to_string_lossy().to_string());
                }
            }
        }

        Ok(results)
    }
}

struct S3Storage {
    bucket: String,
    region: String,
    client: aws_sdk_s3::Client,
}

impl S3Storage {
    async fn new(bucket: String, region: String) -> Result<Self> {
        let config = aws_config::load_from_env().await;
        let client = aws_sdk_s3::Client::new(&config);
        Ok(Self { bucket, region, client })
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(path)
            .body(Bytes::copy_from_slice(data).into())
            .send()
            .await?;
        Ok(())
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?;

        let data = resp.body.collect().await?.into_bytes();
        Ok(data.to_vec())
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let result = self.client
            .head_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;
        Ok(result.is_ok())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut results = Vec::new();
        let mut token = None;

        loop {
            let mut req = self.client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(prefix);

            if let Some(t) = token {
                req = req.continuation_token(t);
            }

            let resp = req.send().await?;

            if let Some(contents) = resp.contents {
                for obj in contents {
                    if let Some(key) = obj.key {
                        results.push(key);
                    }
                }
            }

            token = resp.next_continuation_token;
            if token.is_none() || !resp.is_truncated.unwrap_or(false) {
                break;
            }
        }

        Ok(results)
    }
}

struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl MemoryStorage {
    fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        self.data.write().await.insert(path.to_string(), data.to_vec());
        Ok(())
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.data.read().await.get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Not found: {}", path))
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.data.write().await.remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.data.read().await.contains_key(path))
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        Ok(self.data.read().await.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}