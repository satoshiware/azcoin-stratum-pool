# Vertical Slice 5: Live Mining — Job Source, Validation, Block Submission, Push Notify

## Summary

Complete the live mining path: source real jobs from the AZCOIN daemon, validate shares cryptographically, submit solved blocks, and push fresh work to all connected miners via server-initiated `mining.notify`. This slice brings the pool from stub-based testing to production-capable mining.

## What Was Implemented

### 1. coin_azcoin — Real Job Source

- **DaemonClient** — JSON-RPC client for `getblocktemplate` and `submitblock`
- **NodeApiClient** — REST client for `GET /v1/az/mining/template/current` and share submission
- **RpcJobSource** — `JobSource` implementation backed by `getblocktemplate`; converts template to `Job` with full block assembly data
- **NodeApiJobSource** — `JobSource` implementation backed by Node REST API
- **template_mapper** — Converts RPC `getblocktemplate` response to `Job` with coinbase construction, merkle branch, and block assembly preservation
- **api_template_mapper** — Converts Node API template response to `Job`
- **coinbase_builder** — Constructs coinbase transaction with height commitment, pool payout, and optional witness commitment
- **raw_block_builder** — Serializes complete block (header + coinbase + transactions) for `submitblock`
- **AzcoinShareValidator** — Cryptographic share validation: reconstructs block header from job template + miner nonce/extranonce, computes double-SHA256, checks against pool difficulty target
- **AzcoinBlockSubmitter** — `BlockSubmitter` implementation that calls `submitblock` JSON-RPC
- **build_solved_block_header** — Reconstructs 80-byte solved header from job + share data
- **submit_block_candidate** — Full block serialization and submission pipeline

### 2. pool_core — ActiveJobRegistry + Share Validation

- **ActiveJobRegistry** — Bounded registry (64 jobs max) of recently issued jobs. `clean_jobs=true` clears all prior entries. Used for share validation job lookup.
- **JobAwareShareProcessor** — Validates extranonce2 length, version-rolling mask compliance, job existence in registry, then delegates to coin-specific `ShareValidator`
- **ShareResult::Block** — New variant for shares that meet the network difficulty target
- **ShareResult::UnknownJob** — New variant for shares referencing non-existent jobs
- **ShareResult::Malformed** — New variant for structurally invalid shares
- **ShareResult::LowDifficulty** — New variant for shares below pool target
- **BlockAssemblyData** — Preserved from template for block reconstruction (height, coinbase_value, transactions, witness commitment)
- **ShareValidationContext** — Per-share context: extranonce1, expected extranonce2 length, version-rolling mask
- **ShareSink trait** — Optional external share reporting (posted to share API endpoint)
- **on_notify_sent callback** — `SessionEventHandler` method called when `mining.notify` is actually sent; used to register jobs in the registry

### 3. protocol_sv1 — Version Rolling + Server-Push Notify

- **mining.configure** — Full `mining.configure` support with `version-rolling` extension negotiation (mask intersection, min-bit-count)
- **SessionState.version_rolling** — Per-session negotiated version-rolling configuration
- **SessionState.last_notify_job_id** — Deduplication field for push-notify
- **Server-push job notify** — Session loop rewritten with `tokio::select!` to receive jobs from `tokio::sync::broadcast` channel alongside miner requests
- **send_job_notify helper** — Extracted `mining.set_difficulty` + `mining.notify` write sequence, reused for both authorize-time and push-time notify
- **Backward-compatible listener API** — `run_stratum_listener` now accepts `broadcast::Sender<Job>` for production use; `run_stratum_listener_accept` and `run_stratum_listener_accept_with_difficulty` retain their original signatures for tests (internally create a dummy channel)

### 4. azcoin-pool — Job Poller + Stale Guard

- **Job poller task** — Background `tokio::spawn` loop that polls `JobSource::current_job()` every 5 seconds, detects changes by `job_id` or `block_assembly.height`, and broadcasts via `tokio::sync::broadcast` (capacity 16)
- **broadcast channel** — Created in `main.rs`, sender cloned for poller task, original passed to SV1 listener
- **Sv1SessionHandler** — Implements `on_notify_sent` to register jobs in `ActiveJobRegistry`; `on_submit` checks for `ShareResult::Block` and triggers block submission
- **Stale block guard** — `maybe_submit_block_candidate` compares solved job against `JobSource::current_job()` by job_id and height before submitting; prevents submitting blocks for stale templates
- **Composition** — `build_pool_services` wires `JobSource`, `ShareValidator`, and optional `ShareSink` from config; `build_job_source` selects RPC or API mode

### 5. Testing

- **protocol_sv1** — 16 unit tests: SV1 wire format, notify construction, session response builders, domain mapper, version-rolling negotiation
- **azcoin-pool integration** — 19 SV1 lifecycle tests: subscribe/authorize/notify/submit, version-rolling, difficulty, malformed input rejection, unknown job rejection, crypto validation, prior job acceptance
- **block_found_submission** — 4 integration tests: block share detection, candidate submission, payout script guard, daemon rejection handling
- **api_integration** — 6 tests: health, ready, stats, workers, current job, recent shares
- **job_source_composition** — 4 tests: RPC/API mode wiring, unreachable daemon handling
- **pool_core** — 2 unit tests: clean_jobs invalidation, share sink failure tolerance

## Architecture Choices

- **Broadcast channel for push notify** — `tokio::sync::broadcast` allows a single producer (poller) to fan out to many consumers (sessions) without maintaining a session registry. Each session subscribes independently. Lagged sessions receive a warning and skip to the next job.
- **tokio::select! session loop** — Each session owns both its TCP read half and broadcast receiver. `select!` enables the session to respond to miner requests and receive pushed jobs within a single task, avoiding concurrent writes.
- **Job deduplication** — `SessionState.last_notify_job_id` prevents sending the same job twice (once from authorize, once from the poller broadcasting the same job shortly after).
- **Backward-compatible listener functions** — Test-facing `run_stratum_listener_accept` creates a dummy channel internally, avoiding changes to 20+ existing test call sites.
- **Stale block guard** — Prevents submitting blocks for templates that are no longer current. Compares job_id and block height against a fresh `current_job()` call at submission time.
- **clean_jobs invalidation** — When `ActiveJobRegistry::register` receives a job with `clean_jobs=true`, it clears all prior jobs. This matches Stratum V1 semantics where `clean_jobs=true` means a new block was found and old work is invalid.

## Commands

```bash
cargo build -p azcoin-pool
cargo test -p protocol_sv1
cargo test -p azcoin-pool
cargo test -p pool_core
cargo clippy --workspace -- -D warnings
```

## Next Steps

- Configurable poll interval (currently hardcoded 5 seconds)
- ZMQ `hashblock` notification for sub-second new-block detection
- Variable difficulty adjustment per miner
- Round management and payout logic
- Persistent storage (PostgreSQL)
- Stratum V2 support via `sv2_scaffold`
