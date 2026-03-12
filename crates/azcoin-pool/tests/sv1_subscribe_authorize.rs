//! Integration test: SV1 subscribe/authorize/notify/submit, worker visible in API.

use api_server::{api_router, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use pool_core::{JobSource, PoolServices, ShareProcessor};
use protocol_sv1::{run_stratum_listener_accept, SessionEventHandler};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::util::ServiceExt;

struct TestSv1Handler {
    worker_registry: Arc<pool_core::InMemoryWorkerRegistry>,
    job_source: Arc<dyn JobSource>,
    share_processor: Arc<dyn ShareProcessor>,
}

#[async_trait::async_trait]
impl SessionEventHandler for TestSv1Handler {
    fn on_connect(&self, _: std::net::SocketAddr) {}
    fn on_disconnect(&self, _: std::net::SocketAddr) {}

    async fn on_authorize(&self, username: &str) -> Result<Option<pool_core::Job>, String> {
        self.worker_registry
            .register(pool_core::WorkerIdentity::new(username))
            .await;
        Ok(self.job_source.current_job().await)
    }

    async fn on_submit(&self, share: pool_core::ShareSubmission) -> pool_core::ShareResult {
        self.share_processor.process_share(share).await
    }
}

/// Read one newline-delimited line from the stream.
async fn read_sv1_line(stream: &mut tokio::net::TcpStream) -> String {
    let mut buf = [0u8; 1];
    let mut resp = Vec::new();
    loop {
        stream.read_exact(&mut buf).await.unwrap();
        if buf[0] == b'\n' {
            break;
        }
        resp.push(buf[0]);
    }
    String::from_utf8_lossy(&resp).trim().to_string()
}

async fn send_sv1_request(stream: &mut tokio::net::TcpStream, req: &serde_json::Value) -> String {
    let line = serde_json::to_string(req).unwrap() + "\n";
    stream.write_all(line.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    read_sv1_line(stream).await
}

#[tokio::test]
async fn sv1_subscribe_authorize_worker_in_api() {
    let pool_services = Arc::new(PoolServices::with_stub_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    // Bind stratum to ephemeral port
    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });

    // Small delay for listener to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Connect and send subscribe
    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({
        "id": 1,
        "method": "mining.subscribe",
        "params": []
    });
    let subscribe_resp = send_sv1_request(&mut stream, &subscribe_req).await;
    let subscribe_json: serde_json::Value = serde_json::from_str(&subscribe_resp).unwrap();
    assert!(
        subscribe_json.get("result").is_some(),
        "subscribe should return result"
    );
    assert!(subscribe_json.get("error").is_none() || subscribe_json["error"].is_null());

    // Send authorize - conventional order: response, set_difficulty, notify
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let line1 = send_sv1_request(&mut stream, &authorize_req).await;
    let line2 = read_sv1_line(&mut stream).await;
    let line3 = read_sv1_line(&mut stream).await;

    // First line: authorize response
    let authorize_json: serde_json::Value = serde_json::from_str(&line1).unwrap();
    assert_eq!(
        authorize_json["result"], true,
        "authorize should return true"
    );
    assert!(authorize_json.get("error").is_none() || authorize_json["error"].is_null());

    // Second line: mining.set_difficulty
    let set_diff_json: serde_json::Value = serde_json::from_str(&line2).unwrap();
    assert_eq!(set_diff_json["method"], "mining.set_difficulty");

    // Third line: mining.notify
    let notify_json: serde_json::Value = serde_json::from_str(&line3).unwrap();
    assert_eq!(notify_json["method"], "mining.notify");
    let params = notify_json["params"].as_array().unwrap();
    assert_eq!(params.len(), 9);
    assert_eq!(params[0], "0", "job_id");
    assert_eq!(params[8], true, "clean_jobs");

    // Send submit - expect rejection
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(
        submit_json.get("error").is_some(),
        "submit should be rejected"
    );
    assert_eq!(
        submit_json["error"]["message"],
        "share validation not implemented"
    );

    drop(stream);

    // Verify worker in API
    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
        .clone()
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
    let workers: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(workers.len(), 1);
    assert_eq!(workers[0]["id"], "user.worker1");

    // Verify share in recent
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
    let shares: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0]["worker_id"], "user.worker1");
    assert_eq!(shares[0]["job_id"], "0");
    assert_eq!(shares[0]["accepted"], false);
    assert_eq!(
        shares[0]["reject_reason"],
        "share validation not implemented"
    );
}
