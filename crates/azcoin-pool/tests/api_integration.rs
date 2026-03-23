//! Integration tests for API endpoints. Proves the vertical slice works.

use api_server::{api_router, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pool_core::PoolServices;
use std::sync::Arc;
use tower::util::ServiceExt;

fn make_app() -> axum::Router {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let api_state = ApiState { pool_services };
    api_router(api_state)
}

#[tokio::test]
async fn health_returns_ok() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"OK");
}

#[tokio::test]
async fn ready_returns_ok() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn pool_stats_returns_json() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/pool/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["pool_name"], "test-pool");
    assert_eq!(json["worker_count"], 0);
}

#[tokio::test]
async fn pool_jobs_current_returns_placeholder_job() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/pool/jobs/current")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("job_id").is_some());
    assert_eq!(json["job_id"], "0");
    assert_eq!(json["clean_jobs"], true);
}

#[tokio::test]
async fn pool_shares_recent_returns_array() {
    let app = make_app();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/pool/shares/recent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(json.is_empty(), "no shares before any submit");
}

#[tokio::test]
async fn pool_workers_returns_empty_array() {
    let app = make_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/pool/workers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(json.is_empty());
}

