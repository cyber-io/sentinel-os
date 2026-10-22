//! API routes for threat intelligence

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, error};

use crate::detector::{ThreatDetector, DetectionResult, NetworkFlow, FileFeatures};
use crate::explainer::{ThreatExplainer, Explanation};
use crate::feeds::{FeedManager, ThreatIndicator, GeoIPInfo};

#[derive(Clone)]
pub struct AppState {
    pub detector: Arc<tokio::sync::RwLock<ThreatDetector>>,
    pub explainer: Arc<ThreatExplainer>,
    pub feeds: Arc<FeedManager>,
    pub config: crate::Config,
    pub start_time: SystemTime,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/detect/network", post(detect_network))
        .route("/detect/file", post(detect_file))
        .route("/detect/log", post(detect_log))
        .route("/explain/detection", post(explain_detection))
        .route("/explain/network", post(explain_network))
        .route("/explain/file", post(explain_file))
        .route("/feeds", get(list_feeds))
        .route("/feeds/indicators", get(get_indicators))
        .route("/lookup/ip/:ip", get(lookup_ip))
        .route("/lookup/domain/:domain", get(lookup_domain))
        .route("/lookup/hash/:hash", get(lookup_hash))
        .route("/geoip/:ip", get(geoip))
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
struct NetworkFlowRequest {
    src_ip: String,
    dst_ip: String,
    src_port: u16,
    dst_port: u16,
    protocol: String,
    bytes: u64,
    packets: u64,
    duration_ms: u64,
    flags: String,
}

async fn detect_network(
    State(state): State<AppState>,
    Json(req): Json<NetworkFlowRequest>,
) -> Result<Json<Vec<DetectionResult>>, StatusCode> {
    let flow = NetworkFlow {
        src_ip: req.src_ip,
        dst_ip: req.dst_ip,
        src_port: req.src_port,
        dst_port: req.dst_port,
        protocol: req.protocol,
        bytes: req.bytes,
        packets: req.packets,
        duration_ms: req.duration_ms,
        flags: req.flags,
        timestamp: Utc::now(),
    };

    let detector = state.detector.read().await;
    detector.detect_network(&flow).await
        .map(Json)
        .map_err(|e| {
            error!("Network detection failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct FileFeaturesRequest {
    hash: String,
    size: u64,
    entropy: f32,
    imports: Vec<String>,
    sections: Vec<crate::detector::SectionInfo>,
    strings: Vec<String>,
}

async fn detect_file(
    State(state): State<AppState>,
    Json(req): Json<FileFeaturesRequest>,
) -> Result<Json<Vec<DetectionResult>>, StatusCode> {
    let file = FileFeatures {
        hash: req.hash,
        size: req.size,
        entropy: req.entropy,
        imports: req.imports,
        sections: req.sections,
        strings: req.strings,
    };

    let detector = state.detector.read().await;
    detector.detect_file(&file).await
        .map(Json)
        .map_err(|e| {
            error!("File detection failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct LogRequest {
    log: String,
}

async fn detect_log(
    State(state): State<AppState>,
    Json(req): Json<LogRequest>,
) -> Result<Json<Vec<DetectionResult>>, StatusCode> {
    let detector = state.detector.read().await;
    detector.detect_log(&req.log).await
        .map(Json)
        .map_err(|e| {
            error!("Log detection failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct ExplainDetectionRequest {
    detection: DetectionResult,
    context: Option<String>,
}

async fn explain_detection(
    State(state): State<AppState>,
    Json(req): Json<ExplainDetectionRequest>,
) -> Result<Json<Explanation>, StatusCode> {
    state.explainer.explain_detection(&req.detection, req.context.as_deref()).await
        .map(Json)
        .map_err(|e| {
            error!("Explanation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct ExplainNetworkRequest {
    flow: NetworkFlow,
    detections: Vec<DetectionResult>,
}

async fn explain_network(
    State(state): State<AppState>,
    Json(req): Json<ExplainNetworkRequest>,
) -> Result<Json<Explanation>, StatusCode> {
    state.explainer.explain_network_flow(&req.flow, &req.detections).await
        .map(Json)
        .map_err(|e| {
            error!("Network explanation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct ExplainFileRequest {
    file: FileFeatures,
    detections: Vec<DetectionResult>,
}

async fn explain_file(
    State(state): State<AppState>,
    Json(req): Json<ExplainFileRequest>,
) -> Result<Json<Explanation>, StatusCode> {
    state.explainer.explain_file(&req.file, &req.detections).await
        .map(Json)
        .map_err(|e| {
            error!("File explanation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn list_feeds(State(state): State<AppState>) -> Json<Vec<serde_json::Value>> {
    Json(state.feeds.get_feed_status().await)
}

async fn get_indicators(State(state): State<AppState>) -> Json<Vec<ThreatIndicator>> {
    Json(state.feeds.get_all_indicators().await)
}

async fn lookup_ip(State(state): State<AppState>, Path(ip): Path<String>) -> Result<Json<Option<ThreatIndicator>>, StatusCode> {
    state.feeds.lookup_ip(&ip).await
        .map(Json)
        .map_err(|e| {
            error!("IP lookup failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn lookup_domain(State(state): State<AppState>, Path(domain): Path<String>) -> Result<Json<Option<ThreatIndicator>>, StatusCode> {
    state.feeds.lookup_domain(&domain).await
        .map(Json)
        .map_err(|e| {
            error!("Domain lookup failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn lookup_hash(State(state): State<AppState>, Path(hash): Path<String>) -> Result<Json<Option<ThreatIndicator>>, StatusCode> {
    state.feeds.lookup_hash(&hash).await
        .map(Json)
        .map_err(|e| {
            error!("Hash lookup failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn geoip(State(state): State<AppState>, Path(ip): Path<String>) -> Result<Json<Option<GeoIPInfo>>, StatusCode> {
    state.feeds.get_geoip(&ip).await
        .map(Json)
        .map_err(|e| {
            error!("GeoIP lookup failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let feeds = state.feeds.feeds.read().await;
    let total_indicators: usize = feeds.values().map(|f| f.indicators.len()).sum();

    Json(serde_json::json!({
        "feeds": feeds.len(),
        "total_indicators": total_indicators,
        "models_loaded": state.config.detector.models.len(),
    }))
}