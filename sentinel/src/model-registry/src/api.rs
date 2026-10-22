//! API routes for model registry

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use axum::{
    extract::{Path, Query, State, Multipart},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put, delete},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tracing::{debug, info, error};
use uuid::Uuid;

use crate::registry::{ModelRegistry, ModelManifest, ModelIndex};

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<RwLock<ModelRegistry>>,
    pub config: crate::Config,
    pub start_time: SystemTime,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/models", get(list_models))
        .route("/models", post(push_model))
        .route("/models/:name/:tag", get(get_model))
        .route("/models/:name/:tag", delete(delete_model))
        .route("/models/:name/:tag/pull", post(pull_model))
        .route("/models/:name/:tag/verify", post(verify_model))
        .route("/models/:name/:tag/sign", post(sign_model))
        .route("/models/:name/:tag/download", get(download_model))
        .route("/index", get(get_index))
        .route("/metrics", get(metrics))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let uptime = SystemTime::now().duration_since(state.start_time).unwrap_or_default();

    Json(serde_json::json!({
        "status": "healthy",
        "uptime_seconds": uptime.as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_models(
    State(state): State<AppState>,
    Query(params): Query<ListQuery>,
) -> Result<Json<Vec<ModelManifest>>, StatusCode> {
    let registry = state.registry.read().await;
    let mut models = registry.list().await;

    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100);

    models = models.into_iter().skip(offset).take(limit).collect();

    Ok(Json(models))
}

#[derive(Deserialize)]
struct PushQuery {
    name: String,
    tag: String,
    sign: Option<bool>,
}

async fn push_model(
    State(state): State<AppState>,
    Query(params): Query<PushQuery>,
    mut multipart: Multipart,
) -> Result<Json<ModelManifest>, StatusCode> {
    // In real implementation, would handle multipart upload
    // For now, return placeholder
    let registry = state.registry.read().await;

    let manifest = ModelManifest {
        name: params.name,
        tag: params.tag,
        digest: "sha256:placeholder".to_string(),
        size_bytes: 0,
        architecture: "amd64".to_string(),
        os: "linux".to_string(),
        created_at: Utc::now(),
        labels: std::collections::HashMap::new(),
        annotations: std::collections::HashMap::new(),
        layers: vec![],
        config: crate::registry::ConfigInfo {
            digest: "sha256:config".to_string(),
            size_bytes: 0,
            media_type: "application/vnd.sentinel.model.config".to_string(),
        },
        signatures: vec![],
        sbom: None,
    };

    Ok(Json(manifest))
}

async fn get_model(
    State(state): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<Json<ModelManifest>, StatusCode> {
    let registry = state.registry.read().await;

    registry.get(&name, &tag).await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_model(
    State(state): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let mut registry = state.registry.write().await;

    registry.delete(&name, &tag).await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct PullQuery {
    verify: Option<bool>,
}

async fn pull_model(
    State(state): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
    Query(params): Query<PullQuery>,
) -> Result<Json<ModelManifest>, StatusCode> {
    let registry = state.registry.read().await;

    registry.pull(&name, &tag, params.verify.unwrap_or(true)).await
        .map(Json)
        .map_err(|e| {
            error!("Pull failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn verify_model(
    State(state): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let registry = state.registry.read().await;

    let result = registry.verify(&name, &tag).await
        .map_err(|e| {
            error!("Verify failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "verified": result,
        "model": format!("{}:{}", name, tag),
    })))
}

async fn sign_model(
    State(state): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<Json<ModelManifest>, StatusCode> {
    let mut registry = state.registry.write().await;

    registry.sign(&name, &tag).await
        .map(Json)
        .map_err(|e| {
            error!("Sign failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn download_model(
    State(state): State<AppState>,
    Path((name, tag)): Path<(String, String)>,
) -> Result<impl IntoResponse, StatusCode> {
    let registry = state.registry.read().await;

    let manifest = registry.get(&name, &tag).await
        .ok_or(StatusCode::NOT_FOUND)?;

    // In real implementation, would stream the model layers
    // For now, return manifest as JSON
    Ok(Json(manifest))
}

async fn get_index(State(state): State<AppState>) -> Result<Json<ModelIndex>, StatusCode> {
    let registry = state.registry.read().await;
    let index = registry.index.read().await.clone();
    Ok(Json(index))
}

async fn metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let registry = state.registry.read().await;
    let index = registry.index.read().await;

    Json(serde_json::json!({
        "models_total": index.models.values().map(|v| v.len()).sum::<usize>(),
        "unique_models": index.models.len(),
        "last_updated": index.updated_at,
    }))
}