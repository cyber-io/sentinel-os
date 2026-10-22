//! API routes for health check service

use std::sync::Arc;
use std::time::SystemTime;

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

use crate::checks::{HealthChecker, HealthCheckResult, HealthStatus, SystemHealth};
use crate::collector::{MetricsCollector, HealthSnapshot};

#[derive(Clone)]
pub struct AppState {
    pub checker: Arc<tokio::sync::RwLock<HealthChecker>>,
    pub collector: Arc<MetricsCollector>,
    pub config: crate::Config,
    pub start_time: SystemTime,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/check", get(get_all_checks))
        .route("/check/:name", get(get_check))
        .route("/check/:name/run", post(run_check))
        .route("/snapshots", get(get_snapshots))
        .route("/snapshots/latest", get(get_latest_snapshot))
        .route("/metrics", get(get_metrics))
        .route("/metrics/:name", get(get_metric))
        .route("/metrics/:name/points", get(get_metric_points))
        .route("/summary", get(get_summary))
        .route("/prometheus", get(prometheus_metrics))
        .route("/system", get(get_system_info))
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

async fn get_all_checks(State(state): State<AppState>) -> Result<Json<Vec<HealthCheckResult>>, StatusCode> {
    let mut checker = state.checker.write().await;
    checker.run_all_checks().await
        .map(Json)
        .map_err(|e| {
            error!("Health checks failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_check(State(state): State<AppState>, Path(name): Path<String>) -> Result<Json<HealthCheckResult>, StatusCode> {
    let mut checker = state.checker.write().await;
    checker.run_check(&name).await
        .map(Json)
        .map_err(|e| {
            error!("Health check {} failed: {}", name, e);
            StatusCode::NOT_FOUND
        })
}

async fn run_check(State(state): State<AppState>, Path(name): Path<String>) -> Result<Json<HealthCheckResult>, StatusCode> {
    let mut checker = state.checker.write().await;
    checker.run_check(&name).await
        .map(Json)
        .map_err(|e| {
            error!("Health check {} failed: {}", name, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Deserialize)]
struct SnapshotsQuery {
    since: Option<DateTime<Utc>>,
    limit: Option<usize>,
}

async fn get_snapshots(
    State(state): State<AppState>,
    Query(params): Query<SnapshotsQuery>,
) -> Json<Vec<HealthSnapshot>> {
    Json(state.collector.get_history(params.since, params.limit.unwrap_or(100)).await)
}

async fn get_latest_snapshot(State(state): State<AppState>) -> Result<Json<HealthSnapshot>, StatusCode> {
    state.collector.get_latest_snapshot().await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_metrics(State(state): State<AppState>) -> Json<Vec<crate::collector::MetricSeries>> {
    Json(state.collector.get_all_metrics().await)
}

async fn get_metric(State(state): State<AppState>, Path(name): Path<String>) -> Result<Json<crate::collector::MetricSeries>, StatusCode> {
    state.collector.get_metric(&name).await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct MetricPointsQuery {
    since: Option<DateTime<Utc>>,
}

async fn get_metric_points(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<MetricPointsQuery>,
) -> Json<Vec<crate::collector::MetricPoint>> {
    Json(state.collector.get_metric_points(&name, params.since).await)
}

async fn get_summary(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.collector.get_summary().await)
}

async fn prometheus_metrics(State(state): State<AppState>) -> Result<String, StatusCode> {
    state.collector.export_prometheus().await
        .map_err(|e| {
            error!("Prometheus export failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_system_info(State(state): State<AppState>) -> Json<serde_json::Value> {
    let checker = state.checker.read().await;
    Json(checker.get_system_info())
}