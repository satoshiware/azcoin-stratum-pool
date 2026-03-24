//! Integration test: SV1 subscribe/authorize/notify/submit, worker visible in API.

use api_server::{api_router, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use coin_azcoin::AzcoinShareValidator;
use http_body_util::BodyExt;
use pool_core::{
    FixedJobSource, Job, JobSource, PoolServices, ShareProcessor, ShareValidator, VecJobSource,
};
use protocol_sv1::{run_stratum_listener_accept, SessionEventHandler};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::util::ServiceExt;

struct TestSv1Handler {
    worker_registry: Arc<pool_core::InMemoryWorkerRegistry>,
    job_source: Arc<dyn JobSource>,
    job_registry: Arc<pool_core::ActiveJobRegistry>,
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

    async fn on_notify_sent(&self, job: pool_core::Job) {
        self.job_registry.register(job).await;
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
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
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

    // Send submit - job_id "0" matches placeholder job, expect acceptance
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(
        submit_json.get("result") == Some(&serde_json::json!(true)),
        "submit should be accepted when job_id matches"
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

    // Verify share in recent - accepted when job_id matches
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
    assert_eq!(shares[0]["accepted"], true);
}

#[tokio::test]
async fn sv1_configure_is_accepted_and_subscribe_authorize_still_work() {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();

    let configure_req = serde_json::json!({
        "id": 1,
        "method": "mining.configure",
        "params": [
            ["version-rolling"],
            {
                "version-rolling.mask": "1fffe000",
                "version-rolling.min-bit-count": 2
            }
        ]
    });
    let configure_resp = send_sv1_request(&mut stream, &configure_req).await;
    let configure_json: serde_json::Value = serde_json::from_str(&configure_resp).unwrap();
    assert_eq!(
        configure_json["result"],
        serde_json::json!({
            "version-rolling": false,
            "version-rolling.mask": "00000000",
            "version-rolling.min-bit-count": 0
        })
    );
    assert!(configure_json.get("error").is_none() || configure_json["error"].is_null());

    let subscribe_req = serde_json::json!({
        "id": 2,
        "method": "mining.subscribe",
        "params": []
    });
    let subscribe_resp = send_sv1_request(&mut stream, &subscribe_req).await;
    let subscribe_json: serde_json::Value = serde_json::from_str(&subscribe_resp).unwrap();
    assert!(subscribe_json.get("result").is_some());

    let authorize_req = serde_json::json!({
        "id": 3,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let authorize_resp = send_sv1_request(&mut stream, &authorize_req).await;
    let set_diff = read_sv1_line(&mut stream).await;
    let notify = read_sv1_line(&mut stream).await;

    let authorize_json: serde_json::Value = serde_json::from_str(&authorize_resp).unwrap();
    assert_eq!(authorize_json["result"], true);

    let set_diff_json: serde_json::Value = serde_json::from_str(&set_diff).unwrap();
    assert_eq!(set_diff_json["method"], "mining.set_difficulty");

    let notify_json: serde_json::Value = serde_json::from_str(&notify).unwrap();
    assert_eq!(notify_json["method"], "mining.notify");
}

/// Subscribed/authorized miner with live JobSource receives mining.notify built from that job.
#[tokio::test]
async fn sv1_live_notify_has_job_data() {
    let live_job = Job {
        job_id: "live-abc123".to_string(),
        prev_hash: {
            let mut h = [0u8; 32];
            h[31] = 0x42;
            h
        },
        coinbase_part1: vec![0x01, 0x02, 0x03],
        coinbase_part2: vec![0xfe, 0xff],
        merkle_branch: vec![],
        version: 0x20000000,
        nbits: 0x1a020e7c,
        ntime: 0x69b33a70,
        clean_jobs: true,
        block_assembly: None,
    };
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(live_job));
    let pool_services = Arc::new(PoolServices::new("test-pool", job_source));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;

    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let line1 = send_sv1_request(&mut stream, &authorize_req).await;
    let _line2 = read_sv1_line(&mut stream).await; // set_difficulty
    let line3 = read_sv1_line(&mut stream).await; // mining.notify

    let authorize_json: serde_json::Value = serde_json::from_str(&line1).unwrap();
    assert_eq!(authorize_json["result"], true);

    let notify_json: serde_json::Value = serde_json::from_str(&line3).unwrap();
    assert_eq!(notify_json["method"], "mining.notify");
    let params = notify_json["params"].as_array().unwrap();
    assert_eq!(params[0], "live-abc123", "job_id from live job");
    assert_eq!(params[5], "20000000", "version");
    assert_eq!(params[6], "1a020e7c", "nbits");
    assert_eq!(params[7], "69b33a70", "ntime");
    assert_eq!(params[8], true, "clean_jobs");
}

/// When no job is available, miner receives authorize response but no set_difficulty or notify.
#[tokio::test]
async fn sv1_no_job_no_notify() {
    let pool_services = Arc::new(PoolServices::with_no_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;

    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let line1 = send_sv1_request(&mut stream, &authorize_req).await;

    let authorize_json: serde_json::Value = serde_json::from_str(&line1).unwrap();
    assert_eq!(authorize_json["result"], true);

    // With no job, we get only the authorize response. No set_difficulty, no notify.
    tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        read_sv1_line(&mut stream),
    )
    .await
    .expect_err("no further lines when no job");
}

/// Malformed mining.submit is rejected with explicit reason.
#[tokio::test]
async fn sv1_submit_malformed_rejected() {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    // Malformed: ntime "zz" invalid hex
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "zz", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("ntime"));
}

/// Malformed nonce (wrong length) is rejected at parse time.
#[tokio::test]
async fn sv1_submit_malformed_nonce_rejected() {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    // Malformed: nonce "123" has 3 hex chars, must be 8
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "00000000", "123"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("nonce"));
}

/// Malformed ntime (wrong length) is rejected at parse time.
#[tokio::test]
async fn sv1_submit_malformed_ntime_rejected() {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    // Malformed: ntime "123" has 3 hex chars, must be 8
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "123", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("ntime"));
}

/// Wrong extranonce2 size (session expects 4 bytes) is rejected and recorded in recent shares.
#[tokio::test]
async fn sv1_submit_wrong_extranonce2_size_rejected() {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    // extra_nonce2 "00" decodes to 1 byte; session expects 4 (from subscribe)
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("extra_nonce2"));

    drop(stream);

    // Recent shares API reflects rejected attempt
    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
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
    assert_eq!(shares[0]["accepted"], false);
}

/// Structurally valid but low-difficulty share rejected with explicit reason (crypto validation).
#[tokio::test]
async fn sv1_submit_low_difficulty_rejected() {
    let job = Job::placeholder();
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(job));
    let share_validator: Arc<dyn ShareValidator> = Arc::new(AzcoinShareValidator::new());
    let pool_services = Arc::new(PoolServices::new_with_validator(
        "test-pool",
        job_source,
        share_validator,
        4,
    ));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    // Structurally valid but hash will be above pool target (nonce=0, ntime=0)
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("target"));

    drop(stream);

    // Recent shares records LowDifficulty rejection
    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
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
    assert_eq!(shares[0]["accepted"], false);
}

/// Structurally valid share that meets pool target is accepted (using AcceptAll validator).
#[tokio::test]
async fn sv1_submit_meets_pool_target_accepted() {
    let job = Job::placeholder();
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(job));
    let share_validator: Arc<dyn ShareValidator> = Arc::new(pool_core::AcceptAllShareValidator);
    let pool_services = Arc::new(PoolServices::new_with_validator(
        "test-pool",
        job_source,
        share_validator,
        4,
    ));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "0", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(submit_json["result"], true);

    drop(stream);

    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
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
    assert_eq!(shares[0]["accepted"], true);
}

/// Malformed extranonce1 (invalid hex) rejected before hashing.
#[tokio::test]
async fn sv1_submit_malformed_extranonce1_rejected() {
    let job = Job::placeholder();
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(job.clone()));
    let share_validator: Arc<dyn ShareValidator> = Arc::new(AzcoinShareValidator::new());
    let pool_services = Arc::new(PoolServices::new_with_validator(
        "test-pool",
        job_source,
        share_validator,
        4,
    ));
    pool_services.job_registry.register(job).await;
    // Manually build share with invalid extranonce1 hex (bypasses protocol)
    let share = pool_core::ShareSubmission {
        job_id: "0".to_string(),
        worker: pool_core::WorkerIdentity::new("user.worker1"),
        extra_nonce2: vec![0, 0, 0, 0],
        ntime: 0,
        nonce: 0,
        validation_context: Some(pool_core::ShareValidationContext {
            expected_extra_nonce2_len: Some(4),
            extranonce1_hex: Some("zz".to_string()),
        }),
    };
    let result = pool_services.share_processor.process_share(share).await;
    assert!(matches!(result, pool_core::ShareResult::Malformed { .. }));
    assert!(result.reject_reason().unwrap().contains("extranonce1"));
}

/// Unknown job_id is rejected and appears in recent shares.
#[tokio::test]
async fn sv1_submit_unknown_job_rejected() {
    let pool_services = Arc::new(PoolServices::with_placeholder_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    // job_id "unknown-job" does not match placeholder job "0"
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "unknown-job", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unknown job"));

    drop(stream);

    // Recent shares API reflects rejected attempt
    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
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
    assert_eq!(shares[0]["accepted"], false);
    assert_eq!(shares[0]["job_id"], "unknown-job");
    assert!(shares[0]["reject_reason"]
        .as_str()
        .unwrap()
        .contains("unknown job"));
}

/// Job is registered when mining.notify is emitted (on_notify_sent), not when authorize returns.
#[tokio::test]
async fn sv1_job_registered_when_notify_emitted() {
    let live_job = pool_core::Job {
        job_id: "notify-job".to_string(),
        prev_hash: [0u8; 32],
        coinbase_part1: vec![0x01],
        coinbase_part2: vec![0xff],
        merkle_branch: vec![],
        version: 0x20000000,
        nbits: 0x1d00ffff,
        ntime: 0,
        clean_jobs: true,
        block_assembly: None,
    };
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(live_job));
    let pool_services = Arc::new(PoolServices::new("test-pool", job_source));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let line_notify = read_sv1_line(&mut stream).await;
    let notify: serde_json::Value = serde_json::from_str(&line_notify).unwrap();
    assert_eq!(notify["params"][0], "notify-job", "notify was sent");

    // Submit for that job - accepted only if job was registered when notify was emitted
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "notify-job", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(
        submit_json["result"], true,
        "job was registered when notify emitted"
    );
}

/// Valid submit tied to known job is accepted and appears in recent shares.
#[tokio::test]
async fn sv1_submit_valid_accepted_in_recent() {
    let live_job = pool_core::Job {
        job_id: "known-job-42".to_string(),
        prev_hash: [0u8; 32],
        coinbase_part1: vec![0x01],
        coinbase_part2: vec![0xff],
        merkle_branch: vec![],
        version: 0x20000000,
        nbits: 0x1d00ffff,
        ntime: 0,
        clean_jobs: true,
        block_assembly: None,
    };
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(live_job));
    let pool_services = Arc::new(PoolServices::new("test-pool", job_source));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    let _ = read_sv1_line(&mut stream).await;
    let _ = read_sv1_line(&mut stream).await;

    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "known-job-42", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(submit_json["result"], true);

    drop(stream);

    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
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
    assert_eq!(shares[0]["accepted"], true);
    assert_eq!(shares[0]["job_id"], "known-job-42");
}

/// Submit for recently issued prior job is accepted (registry validates, not only current job).
#[tokio::test]
async fn sv1_submit_prior_job_accepted() {
    let job_a = Job {
        job_id: "job-a".to_string(),
        prev_hash: [0u8; 32],
        coinbase_part1: vec![0x01],
        coinbase_part2: vec![0xff],
        merkle_branch: vec![],
        version: 0x20000000,
        nbits: 0x1d00ffff,
        ntime: 0,
        clean_jobs: true,
        block_assembly: None,
    };
    let job_b = Job {
        job_id: "job-b".to_string(),
        prev_hash: [1u8; 32],
        coinbase_part1: vec![0x02],
        coinbase_part2: vec![0xfe],
        merkle_branch: vec![],
        version: 0x20000000,
        nbits: 0x1d00ffff,
        ntime: 1,
        clean_jobs: true,
        block_assembly: None,
    };
    let job_source: Arc<dyn JobSource> =
        Arc::new(VecJobSource::new(vec![job_a.clone(), job_b.clone()]));
    let pool_services = Arc::new(PoolServices::new("test-pool", job_source));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Miner 1: connect, subscribe, authorize -> job_a (registered)
    let mut stream1 = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream1, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream1, &authorize_req).await;
    let _ = read_sv1_line(&mut stream1).await;
    let line_notify1 = read_sv1_line(&mut stream1).await;
    let notify1: serde_json::Value = serde_json::from_str(&line_notify1).unwrap();
    assert_eq!(notify1["params"][0], "job-a");

    // Miner 2: connect, subscribe, authorize -> job_b (registered; job_a still in registry)
    let mut stream2 = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let _ = send_sv1_request(&mut stream2, &subscribe_req).await;
    let authorize_req2 = serde_json::json!({
        "id": 1,
        "method": "mining.authorize",
        "params": ["user.worker2", "x"]
    });
    let _ = send_sv1_request(&mut stream2, &authorize_req2).await;
    let _ = read_sv1_line(&mut stream2).await;
    let _ = read_sv1_line(&mut stream2).await;

    // Miner 1 submits for job_a (prior job, not current). Should be accepted.
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "job-a", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream1, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(
        submit_json.get("result"),
        Some(&serde_json::json!(true)),
        "submit for prior job should be accepted"
    );
}

/// Submit when no issued jobs exist is rejected.
#[tokio::test]
async fn sv1_submit_no_issued_jobs_rejected() {
    let pool_services = Arc::new(PoolServices::with_no_job_source("test-pool"));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(TestSv1Handler {
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", stratum_port))
        .await
        .unwrap();
    let subscribe_req = serde_json::json!({"id": 1, "method": "mining.subscribe", "params": []});
    let _ = send_sv1_request(&mut stream, &subscribe_req).await;
    let authorize_req = serde_json::json!({
        "id": 2,
        "method": "mining.authorize",
        "params": ["user.worker1", "x"]
    });
    let _ = send_sv1_request(&mut stream, &authorize_req).await;
    // No notify (authorize returned None)

    // Submit with arbitrary job_id - registry is empty, should reject
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "any-job-id", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert!(submit_json.get("error").is_some());
    assert!(submit_json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unknown job"));

    // Recent shares API records rejected attempt
    let api_state = ApiState {
        pool_services: Arc::clone(&pool_services),
    };
    let app = api_router(api_state);
    let response = app
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
    assert_eq!(shares[0]["accepted"], false);
}
