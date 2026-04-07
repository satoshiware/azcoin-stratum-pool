//! Stratum V1 server entry boundary. Accepts TCP, parses JSON-RPC, dispatches subscribe/authorize/submit/notify.

use crate::domain_mapper::map_request_to_command;
use crate::messages::{Sv1DomainCommand, Sv1Request, Sv1Response};
use crate::notify::build_mining_notify;
use crate::session;
use crate::session_state::SessionState;
use async_trait::async_trait;
use pool_core::{Job, ShareResult, ShareSubmission, ShareValidationContext, WorkerIdentity};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tracing::{info, warn};

const DEFAULT_INITIAL_DIFFICULTY: u32 = 1;

static NEXT_EXTRANONCE1: AtomicU32 = AtomicU32::new(1);

/// Callback for session lifecycle, authorize, and submit.
#[async_trait]
pub trait SessionEventHandler: Send + Sync {
    fn on_connect(&self, peer: SocketAddr);
    fn on_disconnect(&self, peer: SocketAddr);

    /// Called when mining.authorize is received. Return Ok(Some(job)) to accept and send notify,
    /// Ok(None) to accept without notify, Err(msg) to reject.
    async fn on_authorize(&self, username: &str) -> Result<Option<Job>, String>;

    /// Called when mining.notify is actually sent to the miner. Register the job here.
    async fn on_notify_sent(&self, _job: Job) {}

    /// Called when mining.submit is received with a valid ShareSubmission.
    async fn on_submit(&self, share: ShareSubmission) -> ShareResult;
}

/// No-op handler when no pool wiring is provided.
#[async_trait]
impl SessionEventHandler for () {
    fn on_connect(&self, _peer: SocketAddr) {}
    fn on_disconnect(&self, _peer: SocketAddr) {}

    async fn on_authorize(&self, _username: &str) -> Result<Option<Job>, String> {
        Ok(None)
    }

    async fn on_notify_sent(&self, _job: Job) {}

    async fn on_submit(&self, _share: ShareSubmission) -> ShareResult {
        ShareResult::Rejected {
            reason: "submit not wired".to_string(),
        }
    }
}

/// Start the SV1 TCP listener with a job broadcast channel for server-push notifies.
pub async fn run_stratum_listener(
    bind: &str,
    port: u16,
    handler: Arc<dyn SessionEventHandler>,
    initial_difficulty: u32,
    job_tx: broadcast::Sender<Job>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("{}:{}", bind, port);
    let listener = TcpListener::bind(&addr).await?;
    run_listener_inner(listener, handler, initial_difficulty, job_tx).await
}

/// Run SV1 listener on an already-bound TcpListener. For tests with ephemeral ports.
pub async fn run_stratum_listener_accept(
    listener: TcpListener,
    handler: Arc<dyn SessionEventHandler>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (job_tx, _) = broadcast::channel(1);
    run_listener_inner(listener, handler, DEFAULT_INITIAL_DIFFICULTY, job_tx).await
}

/// Run SV1 listener on an already-bound TcpListener with an explicit static difficulty.
pub async fn run_stratum_listener_accept_with_difficulty(
    listener: TcpListener,
    handler: Arc<dyn SessionEventHandler>,
    initial_difficulty: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (job_tx, _) = broadcast::channel(1);
    run_listener_inner(listener, handler, initial_difficulty, job_tx).await
}

async fn run_listener_inner(
    listener: TcpListener,
    handler: Arc<dyn SessionEventHandler>,
    initial_difficulty: u32,
    job_tx: broadcast::Sender<Job>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = listener.local_addr()?;
    info!(addr = %addr, "Stratum V1 listener started");

    loop {
        let (stream, peer) = listener.accept().await?;
        let peer_addr = peer;
        info!(peer = %peer_addr, "Stratum session connected");
        handler.on_connect(peer_addr);

        let handler_clone = Arc::clone(&handler);
        let job_rx = job_tx.subscribe();
        tokio::spawn(async move {
            handle_sv1_session(stream, Arc::clone(&handler_clone), initial_difficulty, job_rx)
                .await;
            info!(peer = %peer_addr, "Stratum session disconnected");
            handler_clone.on_disconnect(peer_addr);
        });
    }
}

/// Handle a single Stratum session with server-push job support via tokio::select!.
async fn handle_sv1_session(
    stream: TcpStream,
    handler: Arc<dyn SessionEventHandler>,
    initial_difficulty: u32,
    mut job_rx: broadcast::Receiver<Job>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut session_state = SessionState::default();

    loop {
        tokio::select! {
            result = reader.read_line(&mut line) => {
                let n = match result {
                    Ok(n) => n,
                    Err(e) => {
                        warn!(error = %e, "SV1 read error");
                        break;
                    }
                };
                if n == 0 {
                    info!("SV1 session closed by peer");
                    break;
                }
                let trimmed = line.trim().to_string();
                line.clear();

                if trimmed.is_empty() {
                    continue;
                }

                let req: Sv1Request = match serde_json::from_str(&trimmed) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(line = %trimmed, error = %e, "invalid SV1 JSON");
                        let response = session::build_error_response(None, -32700, "Parse error");
                        let response_json = match serde_json::to_string(&response) {
                            Ok(json) => json,
                            Err(serialize_error) => {
                                warn!(error = %serialize_error, "failed to serialize SV1 parse error response");
                                continue;
                            }
                        };
                        if let Err(write_error) = write_json_line(&mut writer, &response_json).await {
                            warn!(error = %write_error, "failed to write SV1 parse error response");
                            break;
                        }
                        continue;
                    }
                };

                let (notify_job, response) = dispatch_request(&req, &*handler, &mut session_state).await;
                let response_json = match serde_json::to_string(&response) {
                    Ok(json) => json,
                    Err(e) => {
                        warn!(method = %req.method, error = %e, "failed to serialize SV1 response");
                        continue;
                    }
                };
                let is_configure = req.method == "mining.configure";

                if is_configure {
                    info!(id = ?req.id, body = %response_json, "SV1 configure response");
                }
                match write_json_line(&mut writer, &response_json).await {
                    Ok(()) => {
                        if is_configure {
                            info!("SV1 configure response write succeeded");
                        }
                    }
                    Err(e) => {
                        warn!(method = %req.method, error = %e, "failed to write SV1 response");
                        break;
                    }
                }

                if let Some(job) = notify_job {
                    if !send_job_notify(&mut writer, &job, initial_difficulty).await {
                        break;
                    }
                    session_state.last_notify_job_id = Some(job.job_id.clone());
                    handler.on_notify_sent(job).await;
                }

                if is_configure {
                    info!("SV1 session waiting for next message after configure");
                }
            }
            result = job_rx.recv() => {
                let job = match result {
                    Ok(job) => job,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "SV1 session lagged on job broadcast");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Job broadcast channel closed, ending session");
                        break;
                    }
                };
                if session_state.authorized_worker.is_none() {
                    continue;
                }
                if session_state.last_notify_job_id.as_deref() == Some(job.job_id.as_str()) {
                    continue;
                }
                info!(
                    job_id = %job.job_id,
                    clean_jobs = job.clean_jobs,
                    "pushing mining.notify to session"
                );
                if !send_job_notify(&mut writer, &job, initial_difficulty).await {
                    break;
                }
                session_state.last_notify_job_id = Some(job.job_id.clone());
                handler.on_notify_sent(job).await;
            }
        }
    }
}

/// Send mining.set_difficulty + mining.notify for a job. Returns true if session is alive.
async fn send_job_notify(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    job: &Job,
    initial_difficulty: u32,
) -> bool {
    let set_diff = session::build_set_difficulty_notification(initial_difficulty);
    let set_diff_json = match serde_json::to_string(&set_diff) {
        Ok(json) => json,
        Err(e) => {
            warn!(error = %e, "failed to serialize mining.set_difficulty");
            return false;
        }
    };
    if let Err(e) = write_json_line(writer, &set_diff_json).await {
        warn!(error = %e, "failed to write mining.set_difficulty");
        return false;
    }

    let notify_msg = build_mining_notify(job);
    let notify_json = match serde_json::to_string(&notify_msg) {
        Ok(json) => json,
        Err(e) => {
            warn!(error = %e, "failed to serialize mining.notify");
            return false;
        }
    };
    if let Err(e) = write_json_line(writer, &notify_json).await {
        warn!(error = %e, "failed to write mining.notify");
        return false;
    }

    true
}

async fn write_json_line(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    json: &str,
) -> std::io::Result<()> {
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

/// Dispatch parsed request. Returns (optional job for notify, response).
async fn dispatch_request(
    req: &Sv1Request,
    handler: &dyn SessionEventHandler,
    session_state: &mut SessionState,
) -> (Option<Job>, Sv1Response) {
    let cmd = match map_request_to_command(req) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                None,
                session::build_error_response(req.id.clone(), -32601, "Method not found"),
            );
        }
        Err(msg) => {
            return (None, session::build_submit_reject(req.id.clone(), &msg));
        }
    };

    match cmd {
        Sv1DomainCommand::Configure {
            extensions,
            version_rolling,
        } => {
            let negotiated_version_rolling = version_rolling
                .as_ref()
                .and_then(session::negotiate_version_rolling);
            session_state.version_rolling = negotiated_version_rolling.clone();
            info!(
                method = "mining.configure",
                extensions = ?extensions,
                version_rolling = ?negotiated_version_rolling,
                "SV1 configure"
            );
            (
                None,
                session::build_configure_response(
                    req.id.clone(),
                    &extensions,
                    negotiated_version_rolling.as_ref(),
                ),
            )
        }
        Sv1DomainCommand::Subscribe => {
            let en1 = NEXT_EXTRANONCE1.fetch_add(1, Ordering::Relaxed);
            let en1_hex = format!("{:08x}", en1);
            info!(method = "mining.subscribe", extranonce1 = %en1_hex, "SV1 subscribe");
            session_state.subscribed = true;
            session_state.extranonce1 = en1_hex.clone();
            session_state.extranonce2_size = 4;
            (
                None,
                session::build_subscribe_response(req.id.clone(), &en1_hex),
            )
        }
        Sv1DomainCommand::Authorize { username, .. } => {
            match handler.on_authorize(&username).await {
                Ok(maybe_job) => {
                    session_state.authorized_worker = Some(WorkerIdentity::new(&username));
                    if maybe_job.is_some() {
                        info!(worker = %username, "SV1 authorize accepted, job dispatched");
                    } else {
                        info!(worker = %username, "SV1 authorize accepted");
                    }
                    (maybe_job, session::build_authorize_success(req.id.clone()))
                }
                Err(msg) => {
                    warn!(worker = %username, reason = %msg, "SV1 authorize rejected");
                    (
                        None,
                        session::build_error_response(req.id.clone(), -1, &msg),
                    )
                }
            }
        }
        Sv1DomainCommand::SubmitShare {
            username,
            job_id,
            extra_nonce2,
            ntime,
            nonce,
            version_bits,
        } => {
            let worker = match &session_state.authorized_worker {
                Some(w) => w.clone(),
                None => {
                    warn!("SV1 submit rejected: worker not authorized");
                    return (
                        None,
                        session::build_submit_reject(req.id.clone(), "worker not authorized"),
                    );
                }
            };
            if worker.id != username {
                warn!(expected = %worker.id, received = %username, "SV1 submit rejected: username mismatch");
                return (
                    None,
                    session::build_submit_reject(req.id.clone(), "username mismatch"),
                );
            }
            if version_bits.is_some() && session_state.version_rolling.is_none() {
                warn!("SV1 submit rejected: version rolling not negotiated");
                return (
                    None,
                    session::build_submit_reject(req.id.clone(), "version rolling not negotiated"),
                );
            }
            let validation_context = ShareValidationContext {
                expected_extra_nonce2_len: Some(session_state.extranonce2_size as usize),
                extranonce1_hex: Some(session_state.extranonce1.clone()),
                version_rolling_mask: session_state.version_rolling.as_ref().map(|cfg| cfg.mask),
                version_bits,
            };
            let share = ShareSubmission {
                job_id: job_id.clone(),
                worker,
                extra_nonce2,
                ntime,
                nonce,
                validation_context: Some(validation_context),
            };
            let result = handler.on_submit(share).await;
            let reject_reason = result.reject_reason();
            let (accepted, reason) = if result.is_accepted() {
                (true, None)
            } else {
                (false, reject_reason.as_deref())
            };
            info!(
                worker = %username,
                job_id = %job_id,
                accepted = accepted,
                reject_reason = ?reason,
                "SV1 submit"
            );
            let response = if result.is_accepted() {
                session::build_submit_success(req.id.clone())
            } else {
                session::build_submit_reject(
                    req.id.clone(),
                    reject_reason.as_deref().unwrap_or("rejected"),
                )
            };
            (None, response)
        }
    }
}
