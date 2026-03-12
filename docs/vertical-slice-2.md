# Vertical Slice 2: SV1 Subscribe/Authorize Skeleton

## Summary

Minimal Stratum V1 JSON-RPC handling for `mining.subscribe` and `mining.authorize`. Workers are registered in memory and visible via `/v1/pool/workers`.

## What Was Implemented

### 1. protocol_sv1

- **server.rs**: Read newline-delimited JSON, parse `Sv1Request`, dispatch via `map_request_to_command`
- **SessionEventHandler**: Added `async fn on_authorize(&self, username: &str) -> Result<(), String>`
- **session.rs**: `build_subscribe_response`, `build_authorize_success`, `build_error_response`
- **run_stratum_listener_accept**: New function for tests with ephemeral ports
- **mining.submit**: Returns "not yet implemented" error

### 2. pool_core

- No changes. `InMemoryWorkerRegistry` already supports `register` and `list`.
- `WorkerIdentity::new(username)` parses "user.worker" format.

### 3. api_server

- No changes. `/v1/pool/workers` already reads from `pool_services.worker_registry.list()`.

### 4. azcoin-pool main

- `Sv1SessionHandler` now holds `worker_registry` and implements `on_authorize`
- On authorize: `worker_registry.register(WorkerIdentity::new(username)).await`
- Structured tracing: `info!` for subscribe and authorize success, `warn!` for reject

### 5. Testing

- `sv1_subscribe_authorize.rs`: TCP connect, send subscribe, send authorize, verify worker in API

## Architecture Choices

- **on_authorize callback**: protocol_sv1 stays independent of pool_core. Main wires the handler to `worker_registry`.
- **No password validation**: Stub accepts all. Future: validate or store for payout address.
- **Subscribe response format**: Stub returns `[[subscription_details], extranonce1, extranonce2_size]` per Stratum V1 spec.
- **Logging**: `worker` field for authorize (no password). Info for success, warn for reject.

## Commands

```bash
cargo build -p azcoin-pool
cargo test -p azcoin-pool
cargo run -p azcoin-pool
```

## Next Slice: mining.notify + Job Abstraction Wiring

- Add `mining.notify` handling in protocol_sv1
- Wire `JobSource` from coin_azcoin or stub
- Broadcast job to connected sessions when template changes
- Session state: track extranonce per session for notify
