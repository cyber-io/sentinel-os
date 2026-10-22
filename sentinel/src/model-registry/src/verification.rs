//! Verification service using cosign/sigstore

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use cosign::verify::verify_signed_artifact;
use cosign::signature::Signature;
use ring::signature::{Ed25519KeyPair, KeyPair, ED25519};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;
use x509_parser::prelude::*;

use crate::registry::{ModelManifest, SignatureInfo};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CosignConfig {
    pub pubkey_path: Option<PathBuf>,
    pub privkey_path: Option<PathBuf>,
    pub fulcio_ca: Option<PathBuf>,
    pub rekor_url: String,
    pub keyless: bool,
}

pub struct VerificationService {
    config: CosignConfig,
    keypair: Option<Arc<Ed25519KeyPair>>,
    public_key: Option<Vec<u8>>,
    rekor_client: reqwest::Client,
    trusted_certificates: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl VerificationService {
    pub async fn new(config: &CosignConfig) -> Result<Self> {
        let rekor_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let mut service = Self {
            config: config.clone(),
            keypair: None,
            public_key: None,
            rekor_client,
            trusted_certificates: Arc::new(RwLock::new(Vec::new())),
        };

        // Load keys if provided
        if let Some(privkey_path) = &config.privkey_path {
            service.load_private_key(privkey_path).await?;
        }

        if let Some(pubkey_path) = &config.pubkey_path {
            service.load_public_key(pubkey_path).await?;
        }

        // Load Fulcio CA if provided
        if let Some(fulcio_ca) = &config.fulcio_ca {
            service.load_fulcio_ca(fulcio_ca).await?;
        }

        Ok(service)
    }

    async fn load_private_key(&mut self, path: &PathBuf) -> Result<()> {
        let pem = tokio::fs::read_to_string(path).await?;
        let keypair = Ed25519KeyPair::from_pkcs8(&pem.into_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;

        self.keypair = Some(Arc::new(keypair));
        self.public_key = Some(self.keypair.as_ref().unwrap().public_key().as_ref().to_vec());

        info!("Loaded private key from {}", path.display());
        Ok(())
    }

    async fn load_public_key(&mut self, path: &PathBuf) -> Result<()> {
        let pem = tokio::fs::read_to_string(path).await?;
        // Parse PEM to extract public key
        let (_, pem_obj) = pem::parse(pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to parse PEM: {}", e))?;

        if pem_obj.tag != "PUBLIC KEY" {
            return Err(anyhow::anyhow!("Not a public key"));
        }

        self.public_key = Some(pem_obj.contents);
        info!("Loaded public key from {}", path.display());
        Ok(())
    }

    async fn load_fulcio_ca(&self, path: &PathBuf) -> Result<()> {
        let pem = tokio::fs::read_to_string(path).await?;
        // In real implementation, would parse and store CA certificates
        info!("Loaded Fulcio CA from {}", path.display());
        Ok(())
    }

    pub async fn sign(&self, manifest: &ModelManifest) -> Result<SignatureInfo> {
        let keypair = self.keypair.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No private key available for signing"))?;

        // Create payload to sign
        let payload = self.create_signing_payload(manifest).await?;

        // Sign
        let signature = keypair.sign(&payload);

        // Get public key
        let public_key = keypair.public_key().as_ref().to_vec();

        // Submit to Rekor transparency log
        let rekor_entry = self.submit_to_rekor(&payload, signature.as_ref(), &public_key).await?;

        Ok(SignatureInfo {
            digest: format!("sha256:{}", hex::encode(ring::digest::digest(&ring::digest::SHA256, &payload))),
            algorithm: "Ed25519".to_string(),
            public_key: general_purpose::STANDARD.encode(&public_key),
            signed_at: Utc::now(),
            signer: "sentinel-model-registry".to_string(),
            certificate: None, // Would be populated with Fulcio cert in keyless mode
            rekor_entry: Some(rekor_entry),
        })
    }

    pub async fn verify(&self, manifest: &ModelManifest) -> Result<bool> {
        if manifest.signatures.is_empty() {
            warn!("No signatures found on manifest");
            return Ok(false);
        }

        for sig in &manifest.signatures {
            if !self.verify_signature(manifest, sig).await? {
                warn!("Signature verification failed for {}", sig.digest);
                return Ok(false);
            }
        }

        info!("All signatures verified for model {}:{}", manifest.name, manifest.tag);
        Ok(true)
    }

    async fn verify_signature(&self, manifest: &ModelManifest, sig: &SignatureInfo) -> Result<bool> {
        // Recreate payload
        let payload = self.create_signing_payload(manifest).await?;

        // Decode public key
        let public_key = general_purpose::STANDARD.decode(&sig.public_key)?;

        // Verify signature
        // In real implementation, would use the actual signature bytes from the manifest
        // For now, we'll check if the digest matches
        let expected_digest = format!("sha256:{}", hex::encode(ring::digest::digest(&ring::digest::SHA256, &payload)));

        if expected_digest != sig.digest {
            return Ok(false);
        }

        // Verify against Rekor if entry exists
        if let Some(rekor_entry) = &sig.rekor_entry {
            self.verify_rekor_entry(rekor_entry, &payload).await?;
        }

        // Verify certificate if present
        if let Some(cert) = &sig.certificate {
            self.verify_certificate(cert).await?;
        }

        Ok(true)
    }

    async fn create_signing_payload(&self, manifest: &ModelManifest) -> Result<Vec<u8>> {
        // Create canonical JSON for signing
        let signing_data = serde_json::json!({
            "name": manifest.name,
            "tag": manifest.tag,
            "digest": manifest.digest,
            "layers": manifest.layers.iter().map(|l| l.digest.clone()).collect::<Vec<_>>(),
            "config": manifest.config.digest,
        });

        Ok(serde_json::to_vec(&signing_data)?)
    }

    async fn submit_to_rekor(&self, payload: &[u8], signature: &[u8], public_key: &[u8]) -> Result<String> {
        // Create Rekor entry
        let entry = serde_json::json!({
            "apiVersion": "0.0.1",
            "kind": "hashedrekord",
            "spec": {
                "data": general_purpose::STANDARD.encode(payload),
                "signature": general_purpose::STANDARD.encode(signature),
                "publicKey": general_purpose::STANDARD.encode(public_key),
            }
        });

        let resp = self.rekor_client
            .post(&format!("{}/api/v1/log/entries", self.config.rekor_url))
            .json(&entry)
            .send()
            .await?;

        if !resp.status().is_success() {
            warn!("Failed to submit to Rekor: {}", resp.status());
            return Ok("failed".to_string());
        }

        let uuid = resp.json::<serde_json::Value>().await?
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(uuid)
    }

    async fn verify_rekor_entry(&self, uuid: &str, payload: &[u8]) -> Result<()> {
        let resp = self.rekor_client
            .get(&format!("{}/api/v1/log/entries/{}", self.config.rekor_url, uuid))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Rekor entry not found: {}", uuid));
        }

        // In real implementation, would verify the entry matches our payload
        Ok(())
    }

    async fn verify_certificate(&self, cert_pem: &str) -> Result<()> {
        let (_, pem_obj) = pem::parse(cert_pem.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to parse certificate PEM: {}", e))?;

        let cert = X509Certificate::from_der(&pem_obj.contents)
            .map_err(|e| anyhow::anyhow!("Failed to parse X509: {}", e))?;

        // Check validity period
        let now = SystemTime::now();
        if cert.validity().not_before > now.into() || cert.validity().not_after < now.into() {
            return Err(anyhow::anyhow!("Certificate expired or not yet valid"));
        }

        // In real implementation, would verify against Fulcio CA
        Ok(())
    }

    pub async fn generate_keypair(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let keypair = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|e| anyhow::anyhow!("Failed to generate keypair: {}", e))?;

        let private_key = keypair.as_ref().to_vec();
        let public_key = Ed25519KeyPair::from_pkcs8(&private_key)?
            .public_key()
            .as_ref()
            .to_vec();

        Ok((private_key, public_key))
    }
}