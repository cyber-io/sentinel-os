//! API routes for attestation service

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

use crate::tpm::TpmManager;
use crate::ima::ImaManager;
use crate::pcr::{PcrManager, PcrPolicy, PcrRule, PcrAction};
use crate::quote::{QuoteService, AttestationQuote, VerificationResult, AttestationReport};

#[derive(Clone)]
pub struct AppState {
    pub tpm: Arc<tokio::sync::RwLock<TpmManager>>,
    pub ima: Arc<ImaManager>,
    pub pcr: Arc<PcrManager>,
    pub quote: Arc<QuoteService>,
    pub config: crate::Config,
    pub start_time: SystemTime,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/quote", post(generate_quote))
        .route("/quote/verify", post(verify_quote))
        .route("/report", get(get_report))
        .route("/pcrs", get(get_pcrs))
        .route("/pcrs/:pcr", get(get_pcr))
        .route("/pcrs/:pcr/extend", post(extend_pcr))
        .route("/policy", get(get_policy))
        .route("/policy", put(update_policy))
        .route("/policy/rules", post(add_rule))
        .route("/events", get(get_events))
        .route("/ima/log", get(get_ima_log))
        .route("/ima/entries", get(get_ima_entries))
        .route("/ima/verify/:path", get(verify_measurement))
        .route("/evm/log", get(get_evm_log))
        .route("/metrics", get(metrics))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let uptime = SystemTime::now().duration_since(state.start_time).unwrap_or_default();

    Json(serde_json::json!({
        "status": "healthy",
        "uptime_seconds": uptime.as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
        "tpm_available": state.tpm.read().await.get_context().is_ok(),
    }))
}

#[derive(Deserialize)]
struct QuoteRequest {
    nonce: Option<String>, // hex encoded
}

async fn generate_quote(
    State(state): State<AppState>,
    Json(req): Json<QuoteRequest>,
) -> Result<Json<AttestationQuote>, StatusCode> {
    let nonce = req.nonce.map(|n| hex::decode(n).unwrap_or_default());

    state.quote.generate_quote(nonce.as_deref()).await
        .map(Json)
        .map_err(|e| {
            error!("Quote generation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct VerifyQuoteRequest {
    quote: AttestationQuote,
}

async fn verify_quote(
    State(state): State<AppState>,
    Json(req): Json<VerifyQuoteRequest>,
) -> Result<Json<VerificationResult>, StatusCode> {
    state.quote.verify_quote(&req.quote).await
        .map(Json)
        .map_err(|e| {
            error!("Quote verification failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_report(State(state): State<AppState>) -> Result<Json<AttestationReport>, StatusCode> {
    state.quote.get_attestation_report().await
        .map(Json)
        .map_err(|e| {
            error!("Report generation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_pcrs(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pcrs = state.pcr.get_current_pcrs().await;
    Json(serde_json::json!({ "pcrs": pcrs }))
}

async fn get_pcr(State(state): State<AppState>, Path(pcr): Path<u32>) -> Result<Json<serde_json::Value>, StatusCode> {
    let pcrs = state.pcr.get_current_pcrs().await;
    pcrs.get(&pcr)
        .map(|d| Json(serde_json::json!({ "pcr": pcr, "digest": d })))
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct ExtendPcrRequest {
    data: String, // hex encoded
}

async fn extend_pcr(
    State(state): State<AppState>,
    Path(pcr): Path<u32>,
    Json(req): Json<ExtendPcrRequest>,
) -> Result<StatusCode, StatusCode> {
    let data = hex::decode(req.data).map_err(|_| StatusCode::BAD_REQUEST)?;

    state.pcr.extend_pcr(pcr, &data).await
        .map(|_| StatusCode::OK)
        .map_err(|e| {
            error!("PCR extend failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_policy(State(state): State<AppState>) -> Json<PcrPolicy> {
    Json(state.pcr.get_policy().await)
}

async fn update_policy(
    State(state): State<AppState>,
    Json(policy): Json<PcrPolicy>,
) -> Result<StatusCode, StatusCode> {
    state.pcr.update_policy(policy).await
        .map(|_| StatusCode::OK)
        .map_err(|e| {
            error!("Policy update failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct AddRuleRequest {
    rule: PcrRule,
}

async fn add_rule(
    State(state): State<AppState>,
    Json(req): Json<AddRuleRequest>,
) -> Result<StatusCode, StatusCode> {
    state.pcr.add_rule(req.rule).await
        .map(|_| StatusCode::OK)
        .map_err(|e| {
            error!("Rule addition failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct EventsQuery {
    since: Option<DateTime<Utc>>,
}

async fn get_events(
    State(state): State<AppState>,
    Query(params): Query<EventsQuery>,
) -> Json<Vec<crate::pcr::PcrEvent>> {
    Json(state.pcr.get_events(params.since).await)
}

async fn get_ima_log(State(state): State<AppState>) -> Json<crate::ima::ImaLog> {
    Json(state.ima.get_log().await)
}

#[derive(Deserialize)]
struct ImaEntriesQuery {
    since: Option<DateTime<Utc>>,
}

async fn get_ima_entries(
    State(state): State<AppState>,
    Query(params): Query<ImaEntriesQuery>,
) -> Json<Vec<crate::ima::ImaEntry>> {
    Json(state.ima.get_entries_since(params.since.unwrap_or_else(|| Utc::now() - chrono::Duration::hours(1))).await)
}

async fn verify_measurement(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Find entry in log
    let log = state.ima.get_log().await;
    let entry = log.entries.iter().find(|e| e.template_data == path);

    if let Some(entry) = entry {
        let verified = state.ima.verify_measurement(entry).await.unwrap_or(false);
        Ok(Json(serde_json::json!({
            "path": path,
            "verified": verified,
            "digest": entry.digest,
            "pcr": entry.pcr,
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn get_evm_log(State(state): State<AppState>) -> Json<Vec<crate::ima::EvmEntry>> {
    Json(state.ima.get_evm_entries().await)
}

async fn metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pcrs = state.pcr.get_current_pcrs().await;
    let ima_log = state.ima.get_log().await;
    let events = state.pcr.get_events(None).await;

    Json(serde_json::json!({
        "pcrs_monitored": pcrs.len(),
        "ima_entries": ima_log.entries.len(),
        "pcr_events": events.len(),
        "policy_rules": state.pcr.get_policy().await.rules.len(),
    }))
}