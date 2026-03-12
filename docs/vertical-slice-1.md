# Vertical Slice 1: Happy Path Bootstrap

## Summary

First end-to-end vertical slice proving the architecture works without full mining pool implementation.

## What Was Implemented

### 1. protocol_sv1 — Stub SV1 Server Entry

- **`server.rs`** — New module with `run_stratum_listener(bind, port, handler)`
- Accepts TCP connections on Stratum port
- Logs session connect/disconnect
- `SessionEventHandler` trait for callbacks (keeps protocol_sv1 independent of pool_core)
- No full Stratum parsing yet—structure only

### 2. pool_core — Service Wiring

- **`services.rs`** — New module with:
  - `InMemoryWorkerRegistry` — register/list workers
  - `InMemoryStatsSnapshot` — record connections, worker count, snapshot for API
  - `StubJobSource` — implements `JobSource`, returns `None`
  - `PoolServices` — bundles all for wiring

### 3. coin_azcoin — RPC Client Skeleton

- **`daemon.rs`** — Config-driven `DaemonClient::new(url)`
- `probe()` — connectivity probe stub (returns `Ok(None)` for now)
- `get_block_template()`, `submit_block()` — placeholders

### 4. api_server — Extended Routes

- `/health` — Liveness
- `/ready` — Readiness (stub: always OK)
- `/v1/pool/stats` — Pool stats from `PoolServices.stats`
- `/v1/pool/workers` — Worker list from `PoolServices.worker_registry`

### 5. Application Bootstrap (main.rs)

- Load config
- Initialize tracing
- Construct `PoolServices`
- Create `Sv1SessionHandler` for connect/disconnect
- Spawn `run_stratum_listener` in background
- Start API server with `pool_services` wired

### 6. Tests

- `crates/azcoin-pool/tests/api_integration.rs` — health, ready, stats, workers

## Architecture Choices

- **SessionEventHandler trait** — protocol_sv1 doesn't depend on pool_core. Main passes a handler that updates stats.
- **PoolServices** — Single struct bundling stubs for wiring. API and main both use it.
- **Stats from registry** — `/v1/pool/stats` worker_count comes from `worker_registry.count()` for consistency.
- **Daemon probe stub** — Returns `Ok(None)` until HTTP JSON-RPC is implemented.

## Commands

```bash
# Build
cargo build -p azcoin-pool

# Run (stops any running instance first)
cargo run -p azcoin-pool

# Test health
curl http://localhost:8080/health
curl http://localhost:8080/ready
curl http://localhost:8080/v1/pool/stats
curl http://localhost:8080/v1/pool/workers

# Run tests
cargo test -p azcoin-pool
```

## Next Slice: SV1 Subscribe/Authorize Skeleton

- Parse `mining.subscribe` and `mining.authorize` in protocol_sv1
- Map to domain commands
- Register worker on authorize
- Return stub subscribe response (extranonce, difficulty)
- Wire `ShareProcessor` stub for `mining.submit` (reject all for now)
