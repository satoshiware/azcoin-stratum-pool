# azcoin-stratum-pool

Production AZCOIN mining pool. Stratum V1 first, Stratum V2-ready via clean adapter boundaries.

## Features

- Full Stratum V1 lifecycle: `mining.configure`, `mining.subscribe`, `mining.authorize`, `mining.notify`, `mining.submit`
- Version-rolling negotiation (`mining.configure` with `version-rolling` extension)
- Live job sourcing from AZCOIN daemon via `getblocktemplate` (RPC) or Node REST API
- Server-push job updates: background poller detects new work every 5 seconds and broadcasts `mining.notify` to all connected miners via `tokio::broadcast`
- Cryptographic share validation (double-SHA256 hash check against pool difficulty target)
- Block-found detection and `submitblock` submission to daemon with stale-block guard
- Active job registry with `clean_jobs` invalidation
- Operational HTTP API for health, stats, workers, jobs, and recent shares
- Configurable pool difficulty via `mining.set_difficulty`

## Architecture

Layered Rust workspace:

| Crate | Purpose |
|-------|---------|
| `common` | Shared types, errors, config, tracing |
| `protocol_sv1` | Stratum V1 wire protocol: TCP listener, JSON-RPC parse/dispatch, session loop with `tokio::select!` for server-push notify |
| `pool_core` | Protocol-agnostic domain logic: Job, ShareSubmission, ShareResult, ActiveJobRegistry, WorkerIdentity, stats |
| `coin_azcoin` | AZCOIN daemon integration: RPC client, `getblocktemplate`, block template mapping, coinbase construction, share validation, `submitblock` |
| `storage` | Persistence layer (stubs — workers, shares, rounds) |
| `api_server` | Operational HTTP API: `/health`, `/ready`, `/v1/pool/stats`, `/v1/pool/workers`, `/v1/pool/jobs/current`, `/v1/pool/shares/recent` |
| `azcoin-pool` | Main binary: service composition, SV1 session handler, job poller, startup wiring |
| `sv2_scaffold` | Stratum V2 integration scaffold (future) |

## Quick Start

```bash
# Build
cargo build --release -p azcoin-pool

# Configure
cp deploy/configs/config.example.toml config.toml
# Edit config.toml with your daemon URL, RPC credentials, and pool settings

# Run
cargo run --release -p azcoin-pool

# API
curl http://localhost:8080/health
curl http://localhost:8080/v1/pool/stats
curl http://localhost:8080/v1/pool/workers
curl http://localhost:8080/v1/pool/jobs/current
curl http://localhost:8080/v1/pool/shares/recent

# Stratum: tcp://localhost:3333
```

## Configuration

Copy `deploy/configs/config.example.toml` to `config.toml`. See `.env.example` for environment variables.

### Pool settings

| Field | Description |
|-------|-------------|
| `pool.name` | Pool identifier for logging and API |
| `pool.initial_difficulty` | Static difficulty for `mining.set_difficulty` and share validation |
| `pool.payout_script_pubkey_hex` | Pool payout scriptPubKey; required to arm block-found submission |

### Daemon settings

| Field | Description |
|-------|-------------|
| `daemon.job_source_mode` | `"rpc"` for `getblocktemplate` + `submitblock`, `"api"` for Node REST API |
| `daemon.url` | Base URL for the selected mode |
| `daemon.rpc_user` / `daemon.rpc_password` | JSON-RPC auth (RPC mode) |
| `daemon.node_api_token` | Bearer token (API mode, if required) |
| `daemon.share_api_url` | Optional share submission sink URL |

At startup the pool logs whether block-found submission is armed or disabled based on `pool.payout_script_pubkey_hex`. For end-to-end `submitblock` validation, use RPC mode unless `daemon.url` serves both the Node API and JSON-RPC.

## How It Works

### SV1 Session Lifecycle

1. Miner connects via TCP
2. Miner sends `mining.configure` (optional) — pool negotiates version-rolling
3. Miner sends `mining.subscribe` — pool assigns extranonce1
4. Miner sends `mining.authorize` — pool registers worker, sends `mining.set_difficulty` + `mining.notify` with current job
5. Background job poller detects template changes every 5 seconds and pushes `mining.notify` to all authorized sessions
6. Miner sends `mining.submit` — pool validates share cryptographically, accepts or rejects
7. If share meets network target (`ShareResult::Block`), pool reconstructs solved block header and calls `submitblock`

### Server-Push Job Updates

Each SV1 session runs a `tokio::select!` loop that listens for both miner requests and job broadcasts. A background poller task calls `current_job()` on the configured `JobSource` every 5 seconds. When the `job_id` or block height changes, the new job is broadcast to all sessions via `tokio::sync::broadcast`. Each session deduplicates by `job_id` and sends `mining.set_difficulty` + `mining.notify` with the correct `clean_jobs` flag.

### Block Submission

When a share passes cryptographic validation and meets the network target, the pool:

1. Retrieves the job from `ActiveJobRegistry`
2. Checks freshness via the stale-block guard (compares job against current template)
3. Reconstructs the full solved block header from the job template + miner's nonce/extranonce
4. Serializes the complete block (header + coinbase + transactions)
5. Submits via `submitblock` JSON-RPC to the daemon

## Testing

```bash
# All tests
cargo test --workspace

# Per-crate
cargo test -p protocol_sv1      # 16 unit tests (SV1 wire format, notify, session, domain mapper)
cargo test -p pool_core          # 2 unit tests (ActiveJobRegistry, share sink)
cargo test -p azcoin-pool        # ~30 integration tests (SV1 lifecycle, API, block submission)

# Lint
cargo clippy --workspace -- -D warnings
```

## Documentation

- [ADR 0001](docs/adr/0001-new-repo-and-layered-architecture.md) — Layered architecture
- [ADR 0002](docs/adr/0002-sv1-first-sv2-ready-boundary.md) — SV1 first, SV2-ready
- [Context](docs/architecture/context.md) — System context
- [Data Flow](docs/architecture/data-flow.md) — Data flow
- [SV2 Bridge](docs/architecture/sv2-future-bridge.md) — Future Stratum V2 integration
- [Vertical Slice 1](docs/vertical-slice-1.md) — Happy path bootstrap
- [Vertical Slice 2](docs/vertical-slice-2.md) — SV1 subscribe/authorize skeleton
- [Vertical Slice 3](docs/vertical-slice-3.md) — mining.notify + job abstraction
- [Vertical Slice 4](docs/vertical-slice-4.md) — mining.submit + share submission
- [Vertical Slice 5](docs/vertical-slice-5.md) — Live mining: job source, validation, block submission, push notify

## Deployment

- **Docker**: `deploy/docker/`
- **systemd**: `deploy/systemd/azcoin-pool.service`
- **Operations**: [First Block Checklist](FIRST_BLOCK_OPERATIONS_CHECKLIST.md)

## Status

The pool is operational for production SV1 mining. The following subsystems are live:

- SV1 protocol: full lifecycle including `mining.configure` version-rolling
- Job sourcing: live daemon templates via RPC or Node API, with 5-second poll interval
- Server-push notify: all authorized sessions receive fresh work within seconds of a new block
- Share validation: cryptographic double-SHA256 validation against pool difficulty target
- Block submission: solved blocks are submitted to daemon with stale-block guard
- API: health, stats, workers, current job, recent shares

Not yet implemented: full payout logic, round management, persistent storage, web UI, Stratum V2.

## License

MIT
