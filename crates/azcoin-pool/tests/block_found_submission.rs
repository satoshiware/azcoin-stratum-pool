use api_server::{api_router, ApiState};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use azcoin_pool::sv1_handler::Sv1SessionHandler;
use pool_core::{
    BlockAssemblyData, BlockCandidate, BlockSubmitter, FixedJobSource, Job, JobSource,
    PoolServices, ShareResult, ShareSubmission, ShareValidator,
};
use protocol_sv1::{run_stratum_listener_accept, SessionEventHandler};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::util::ServiceExt;

struct AlwaysBlockValidator;

impl ShareValidator for AlwaysBlockValidator {
    fn validate_share(
        &self,
        _job: &Job,
        _share: &ShareSubmission,
        _extranonce1: &[u8],
        _pool_difficulty: u32,
    ) -> ShareResult {
        ShareResult::Block
    }
}

#[derive(Clone)]
struct RecordingBlockSubmitter {
    submitted: Arc<Mutex<Vec<Vec<u8>>>>,
    response: Result<bool, String>,
}

#[async_trait::async_trait]
impl BlockSubmitter for RecordingBlockSubmitter {
    async fn submit_block(&self, block: BlockCandidate) -> Result<bool, String> {
        self.submitted.lock().unwrap().push(block.raw_block);
        self.response.clone()
    }
}

fn fixed_job_with_block_assembly() -> Job {
    Job {
        job_id: "block-job".to_string(),
        prev_hash: [0u8; 32],
        coinbase_part1: vec![0x01],
        coinbase_part2: vec![0xff],
        merkle_branch: vec![],
        version: 0x20000000,
        nbits: 0x1d00ffff,
        ntime: 0,
        clean_jobs: true,
        block_assembly: Some(BlockAssemblyData {
            height: 100,
            coinbase_value: 5_000_000_000,
            coinbase_aux_flags: Some(vec![0xde, 0xad, 0xbe, 0xef]),
            template_transactions: vec![],
            default_witness_commitment: Some(
                hex::decode("6a24aa21a9ed11223344556677889900aabbccddeeff00112233445566778899")
                    .unwrap(),
            ),
        }),
    }
}

fn payout_script() -> Vec<u8> {
    hex::decode("76a91400112233445566778899aabbccddeeff0011223388ac").unwrap()
}

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

async fn start_handler(
    validator: Arc<dyn ShareValidator>,
    block_submitter: Arc<dyn BlockSubmitter>,
    payout_script_pubkey: Option<Vec<u8>>,
) -> (Arc<PoolServices>, u16) {
    let job = fixed_job_with_block_assembly();
    let job_source: Arc<dyn JobSource> = Arc::new(FixedJobSource::new(job));
    let pool_services = Arc::new(PoolServices::new_with_validator(
        "test-pool",
        job_source,
        validator,
        4,
    ));
    let sv1_handler: Arc<dyn SessionEventHandler> = Arc::new(Sv1SessionHandler {
        stats: Arc::clone(&pool_services.stats),
        worker_registry: Arc::clone(&pool_services.worker_registry),
        job_source: Arc::clone(&pool_services.job_source),
        job_registry: Arc::clone(&pool_services.job_registry),
        share_processor: Arc::clone(&pool_services.share_processor),
        block_submitter,
        payout_script_pubkey,
    });

    let stratum_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stratum_port = stratum_listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        run_stratum_listener_accept(stratum_listener, sv1_handler)
            .await
            .unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    (pool_services, stratum_port)
}

async fn connect_and_authorize(stratum_port: u16) -> tokio::net::TcpStream {
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
    stream
}

#[tokio::test]
async fn non_block_share_does_not_invoke_candidate_submission() {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let block_submitter: Arc<dyn BlockSubmitter> = Arc::new(RecordingBlockSubmitter {
        submitted: submitted.clone(),
        response: Ok(true),
    });
    let (_pool_services, stratum_port) = start_handler(
        Arc::new(pool_core::AcceptAllShareValidator),
        block_submitter,
        Some(payout_script()),
    )
    .await;

    let mut stream = connect_and_authorize(stratum_port).await;
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "block-job", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(submit_json["result"], true);
    assert!(submitted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn block_found_share_path_invokes_candidate_submission() {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let block_submitter: Arc<dyn BlockSubmitter> = Arc::new(RecordingBlockSubmitter {
        submitted: submitted.clone(),
        response: Ok(true),
    });
    let (_pool_services, stratum_port) = start_handler(
        Arc::new(AlwaysBlockValidator),
        block_submitter,
        Some(payout_script()),
    )
    .await;

    let mut stream = connect_and_authorize(stratum_port).await;
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "block-job", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(submit_json["result"], true);

    assert_eq!(submitted.lock().unwrap().len(), 1);
    assert_eq!(submitted.lock().unwrap()[0].len() > 80, true);
}

#[tokio::test]
async fn missing_payout_script_on_block_found_is_handled_cleanly() {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let block_submitter: Arc<dyn BlockSubmitter> = Arc::new(RecordingBlockSubmitter {
        submitted: submitted.clone(),
        response: Ok(true),
    });
    let (pool_services, stratum_port) =
        start_handler(Arc::new(AlwaysBlockValidator), block_submitter, None).await;

    let mut stream = connect_and_authorize(stratum_port).await;
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "block-job", "00000000", "00000000", "00000000"]
    });
    let submit_resp = send_sv1_request(&mut stream, &submit_req).await;
    let submit_json: serde_json::Value = serde_json::from_str(&submit_resp).unwrap();
    assert_eq!(submit_json["result"], true);
    assert!(submitted.lock().unwrap().is_empty());

    let app = api_router(ApiState {
        pool_services: Arc::clone(&pool_services),
    });
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
}

#[tokio::test]
async fn daemon_reject_on_block_found_is_handled_cleanly() {
    let submitted = Arc::new(Mutex::new(Vec::new()));
    let block_submitter: Arc<dyn BlockSubmitter> = Arc::new(RecordingBlockSubmitter {
        submitted: submitted.clone(),
        response: Err("submitblock rejected: high-hash".to_string()),
    });
    let (_pool_services, stratum_port) = start_handler(
        Arc::new(AlwaysBlockValidator),
        block_submitter,
        Some(payout_script()),
    )
    .await;

    let mut stream = connect_and_authorize(stratum_port).await;
    let submit_req = serde_json::json!({
        "id": 3,
        "method": "mining.submit",
        "params": ["user.worker1", "block-job", "00000000", "00000000", "00000000"]
    });
    let first_resp = send_sv1_request(&mut stream, &submit_req).await;
    let first_json: serde_json::Value = serde_json::from_str(&first_resp).unwrap();
    assert_eq!(first_json["result"], true);

    let second_resp = send_sv1_request(&mut stream, &submit_req).await;
    let second_json: serde_json::Value = serde_json::from_str(&second_resp).unwrap();
    assert_eq!(second_json["result"], true);
    assert_eq!(submitted.lock().unwrap().len(), 2);
}
