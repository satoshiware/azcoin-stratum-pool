# Data Flow

## Miner -> Pool (SV1 Session Lifecycle)

1. Miner connects via Stratum TCP
2. Miner sends `mining.configure` (optional) -> pool negotiates version-rolling mask
3. Miner sends `mining.subscribe` -> pool responds with extranonce1 + extranonce2_size
4. Miner sends `mining.authorize` -> pool registers worker, sends `mining.set_difficulty` + `mining.notify`
5. **Server-push loop**: job poller broadcasts new work -> session sends `mining.set_difficulty` + `mining.notify`
6. Miner sends `mining.submit` (share) -> pool validates cryptographically -> accepts or rejects
7. If `ShareResult::Block` -> pool reconstructs header and submits block to daemon

## Job Distribution (Server-Push)

```
JobSource (RPC/API)
    |
    v
Job Poller (5s interval)
    | detects job_id or height change
    v
broadcast::Sender<Job>
    |
    +---> Session 1: tokio::select! -> mining.notify
    +---> Session 2: tokio::select! -> mining.notify
    +---> Session N: tokio::select! -> mining.notify
```

1. Background task polls `JobSource::current_job()` every 5 seconds
2. Compares `job_id` and `block_assembly.height` with last known values
3. On change, broadcasts `Job` via `tokio::sync::broadcast` channel (capacity 16)
4. Each session's `tokio::select!` loop receives the job on its `broadcast::Receiver`
5. Session checks: authorized? same job already sent? If new, sends `mining.set_difficulty` + `mining.notify`
6. `on_notify_sent` registers the job in `ActiveJobRegistry` for share validation

## Share Processing

1. `protocol_sv1` parses `mining.submit` -> `Sv1DomainCommand::SubmitShare`
2. `dispatch_request` builds `ShareSubmission` with `ShareValidationContext` (extranonce1, version-rolling mask)
3. `Sv1SessionHandler::on_submit` calls `ShareProcessor::process_share`
4. `JobAwareShareProcessor` validates:
   - extranonce2 length matches session's extranonce2_size
   - version_bits within negotiated mask (if version-rolling)
   - job_id exists in `ActiveJobRegistry`
   - coin-specific crypto validation via `ShareValidator` (double-SHA256 hash vs pool target)
5. Result recorded in `RecentSharesBuffer` and optionally posted to share sink API
6. If `ShareResult::Block` -> triggers block submission path

## Block Submission Flow

1. `ShareResult::Block` detected in `on_submit`
2. `maybe_submit_block_candidate` retrieves job from `ActiveJobRegistry`
3. Stale-block guard: compares job against `JobSource::current_job()` (height + job_id)
4. `build_solved_block_header` reconstructs 80-byte header from template + miner nonce/extranonce
5. `submit_block_candidate` serializes full block (header + coinbase + template transactions)
6. `BlockSubmitter::submit_block` sends `submitblock` JSON-RPC to daemon
7. Result logged: Submitted / Rejected / LocalError

## Active Job Registry

- Jobs registered when `mining.notify` is sent (`on_notify_sent` callback)
- Bounded to 64 recent jobs (FIFO eviction)
- `clean_jobs=true` clears all prior jobs (new block invalidates old work)
- Share validation looks up job by `job_id` to get template data for hash verification

## API Flow

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Liveness check |
| `GET /ready` | Readiness check (stub: always OK) |
| `GET /v1/pool/stats` | Pool stats: hashrate, worker count, round height/status |
| `GET /v1/pool/workers` | Registered worker list |
| `GET /v1/pool/jobs/current` | Current job metadata (job_id, prev_hash, version, nbits, ntime, clean_jobs) |
| `GET /v1/pool/shares/recent` | Recent share attempts with accept/reject status |
