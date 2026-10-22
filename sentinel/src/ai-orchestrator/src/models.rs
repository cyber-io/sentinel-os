//! Models module - model registry and inference abstraction

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub model: String,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub text: String,
    pub tokens_used: usize,
    pub model: String,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub backend: ModelBackend,
    pub capabilities: Vec<ModelCapability>,
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub loaded: bool,
    pub size_gb: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelBackend {
    LlamaCpp,
    Vllm,
    Ollama,
    OpenAI,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Chat,
    Completion,
    Embedding,
    ToolUse,
    Vision,
    CodeGeneration,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRegistryConfig {
    pub gateway_url: String,
    pub default_model: String,
    pub model_cache_dir: String,
    pub auto_load: bool,
    pub health_check_interval_secs: u64,
}

pub struct ModelRegistry {
    client: Client,
    gateway_url: String,
    models: Arc<DashMap<String, ModelInfo>>,
    backends: Arc<RwLock<HashMap<String, Box<dyn InferenceBackend>>>>,
    config: ModelRegistryConfig,
}

#[async_trait::async_trait]
trait InferenceBackend: Send + Sync {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse>;
    async fn health_check(&self) -> Result<bool>;
    async fn load_model(&self, model: &str) -> Result<()>;
    async fn unload_model(&self, model: &str) -> Result<()>;
}

struct LlamaCppBackend {
    endpoint: String,
    client: Client,
}

struct VllmBackend {
    endpoint: String,
    client: Client,
}

struct OllamaBackend {
    endpoint: String,
    client: Client,
}

struct OpenAIBackend {
    endpoint: String,
    client: Client,
    api_key: Option<String>,
}

impl ModelRegistry {
    pub async fn new(gateway_url: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;

        let registry = Self {
            client,
            gateway_url: gateway_url.to_string(),
            models: Arc::new(DashMap::new()),
            backends: Arc::new(RwLock::new(HashMap::new())),
            config: ModelRegistryConfig {
                gateway_url: gateway_url.to_string(),
                default_model: "sentinel-planner".to_string(),
                model_cache_dir: "/var/lib/sentinel/models".to_string(),
                auto_load: true,
                health_check_interval_secs: 60,
            },
        };

        // Initialize backends
        registry.init_backends().await?;

        // Discover models
        registry.discover_models().await?;

        Ok(registry)
    }

    async fn init_backends(&self) -> Result<()> {
        let mut backends = self.backends.write().await;

        // Llama.cpp backend
        backends.insert(
            "llamacpp".to_string(),
            Box::new(LlamaCppBackend {
                endpoint: format!("{}/v1", self.gateway_url),
                client: self.client.clone(),
            })
        );

        // vLLM backend (if different port)
        backends.insert(
            "vllm".to_string(),
            Box::new(VllmBackend {
                endpoint: format!("{}/v1", self.gateway_url.replace("11434", "8000")),
                client: self.client.clone(),
            })
        );

        // Ollama backend
        backends.insert(
            "ollama".to_string(),
            Box::new(OllamaBackend {
                endpoint: self.gateway_url.clone(),
                client: self.client.clone(),
            })
        );

        Ok(())
    }

    async fn discover_models(&self) -> Result<()> {
        // Query gateway for available models
        let resp = self.client.get(&format!("{}/api/tags", self.gateway_url))
            .send()
            .await?;

        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().await?;
            if let Some(models) = data.get("models").and_then(|v| v.as_array()) {
                for model in models {
                    let name = model.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let size = model.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as f32 / 1_073_741_824.0;

                    let info = ModelInfo {
                        name: name.to_string(),
                        backend: ModelBackend::Ollama,
                        capabilities: vec![ModelCapability::Chat, ModelCapability::Completion],
                        context_window: 4096,
                        max_output_tokens: 2048,
                        loaded: true,
                        size_gb: size,
                    };

                    self.models.insert(name.to_string(), info);
                }
            }
        }

        // Also check vLLM models endpoint
        let vllm_url = self.gateway_url.replace("11434", "8000");
        let resp = self.client.get(&format!("{}/v1/models", vllm_url))
            .send()
            .await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                let data: serde_json::Value = resp.json().await?;
                if let Some(models) = data.get("data").and_then(|v| v.as_array()) {
                    for model in models {
                        let id = model.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let info = ModelInfo {
                            name: id.to_string(),
                            backend: ModelBackend::Vllm,
                            capabilities: vec![ModelCapability::Chat, ModelCapability::Completion, ModelCapability::ToolUse],
                            context_window: 8192,
                            max_output_tokens: 4096,
                            loaded: true,
                            size_gb: 0.0,
                        };
                        self.models.insert(id.to_string(), info);
                    }
                }
            }
        }

        info!("Discovered {} models", self.models.len());
        Ok(())
    }

    pub async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        // Determine backend from model name
        let backend_name = self.get_backend_for_model(&request.model).await;
        let backend = self.backends.read().await.get(&backend_name).cloned();

        let backend = match backend {
            Some(b) => b,
            None => return Err(anyhow::anyhow!("No backend found for model: {}", request.model)),
        };

        backend.infer(request).await
    }

    async fn get_backend_for_model(&self, model: &str) -> String {
        if let Some(info) = self.models.get(model) {
            match info.backend {
                ModelBackend::LlamaCpp => "llamacpp".to_string(),
                ModelBackend::Vllm => "vllm".to_string(),
                ModelBackend::Ollama => "ollama".to_string(),
                ModelBackend::OpenAI => "openai".to_string(),
                ModelBackend::Local => "llamacpp".to_string(),
            }
        } else {
            // Default to Ollama-compatible gateway
            "ollama".to_string()
        }
    }

    pub async fn list_models(&self) -> Vec<ModelInfo> {
        self.models.iter().map(|m| m.value().clone()).collect()
    }

    pub async fn get_model(&self, name: &str) -> Option<ModelInfo> {
        self.models.get(name).map(|m| m.value().clone())
    }

    pub async fn pull_model(&self, name: &str) -> Result<()> {
        let resp = self.client.post(&format!("{}/api/pull", self.gateway_url))
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to pull model: {}", resp.status()));
        }

        // Refresh model list
        self.discover_models().await?;
        Ok(())
    }

    pub async fn health_check(&self) -> Result<HashMap<String, bool>> {
        let mut results = HashMap::new();
        let backends = self.backends.read().await;

        for (name, backend) in backends.iter() {
            match backend.health_check().await {
                Ok(healthy) => { results.insert(name.clone(), healthy); }
                Err(e) => {
                    warn!("Health check failed for {}: {}", name, e);
                    results.insert(name.clone(), false);
                }
            }
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl InferenceBackend for LlamaCppBackend {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let mut messages = Vec::new();

        if let Some(sys) = request.system_prompt {
            messages.push(serde_json::json!({ "role": "system", "content": sys }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": request.prompt }));

        let payload = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "stream": request.stream,
        });

        let resp = self.client.post(&format!("{}/chat/completions", self.endpoint))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Inference failed: {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await?;
        let text = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens_used = data["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(InferenceResponse {
            text,
            tokens_used,
            model: request.model,
            finish_reason: data["choices"][0]["finish_reason"].as_str().unwrap_or("stop").to_string(),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let resp = self.client.get(&format!("{}/models", self.endpoint))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    async fn load_model(&self, _model: &str) -> Result<()> {
        // Llama.cpp loads models on demand
        Ok(())
    }

    async fn unload_model(&self, _model: &str) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl InferenceBackend for VllmBackend {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let mut messages = Vec::new();

        if let Some(sys) = request.system_prompt {
            messages.push(serde_json::json!({ "role": "system", "content": sys }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": request.prompt }));

        let payload = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
            "stream": request.stream,
        });

        let resp = self.client.post(&format!("{}/chat/completions", self.endpoint))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("vLLM inference failed: {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await?;
        let text = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens_used = data["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(InferenceResponse {
            text,
            tokens_used,
            model: request.model,
            finish_reason: data["choices"][0]["finish_reason"].as_str().unwrap_or("stop").to_string(),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let resp = self.client.get(&format!("{}/health", self.endpoint))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    async fn load_model(&self, model: &str) -> Result<()> {
        // vLLM typically pre-loads models
        Ok(())
    }

    async fn unload_model(&self, _model: &str) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl InferenceBackend for OllamaBackend {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let payload = serde_json::json!({
            "model": request.model,
            "prompt": request.prompt,
            "system": request.system_prompt,
            "options": {
                "temperature": request.temperature,
                "num_predict": request.max_tokens,
            },
            "stream": request.stream,
        });

        let resp = self.client.post(&format!("{}/api/generate", self.endpoint))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Ollama inference failed: {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await?;
        let text = data["response"].as_str().unwrap_or("").to_string();

        Ok(InferenceResponse {
            text,
            tokens_used: data["eval_count"].as_u64().unwrap_or(0) as usize,
            model: request.model,
            finish_reason: if data["done"].as_bool().unwrap_or(false) { "stop" } else { "length" }.to_string(),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let resp = self.client.get(&format!("{}/api/tags", self.endpoint))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    async fn load_model(&self, model: &str) -> Result<()> {
        // Ollama loads models on demand
        Ok(())
    }

    async fn unload_model(&self, _model: &str) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl InferenceBackend for OpenAIBackend {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let mut messages = Vec::new();

        if let Some(sys) = request.system_prompt {
            messages.push(serde_json::json!({ "role": "system", "content": sys }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": request.prompt }));

        let mut req_builder = self.client.post(&format!("{}/chat/completions", self.endpoint))
            .json(&serde_json::json!({
                "model": request.model,
                "messages": messages,
                "temperature": request.temperature,
                "max_tokens": request.max_tokens,
                "stream": request.stream,
            }));

        if let Some(key) = &self.api_key {
            req_builder = req_builder.bearer_auth(key);
        }

        let resp = req_builder.send().await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("OpenAI inference failed: {}", resp.status()));
        }

        let data: serde_json::Value = resp.json().await?;
        let text = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tokens_used = data["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize;

        Ok(InferenceResponse {
            text,
            tokens_used,
            model: request.model,
            finish_reason: data["choices"][0]["finish_reason"].as_str().unwrap_or("stop").to_string(),
        })
    }

    async fn health_check(&self) -> Result<bool> {
        let mut req_builder = self.client.get(&format!("{}/models", self.endpoint))
            .timeout(Duration::from_secs(5));

        if let Some(key) = &self.api_key {
            req_builder = req_builder.bearer_auth(key);
        }

        let resp = req_builder.send().await?;
        Ok(resp.status().is_success())
    }

    async fn load_model(&self, _model: &str) -> Result<()> {
        Ok(())
    }

    async fn unload_model(&self, _model: &str) -> Result<()> {
        Ok(())
    }
}