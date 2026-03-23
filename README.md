# azcoin-stratum-pool

Production AZCOIN mining pool. Stratum V1 first, Stratum V2-ready via clean adapter boundaries.

## Architecture

Layered Rust workspace:

| Crate | Purpose |
|-------|---------|
| `common` | Shared types, errors, config, tracing |
| `protocol_sv1` | Stratum V1 wire protocol only |
| `pool_core` | Protocol-agnostic domain logic |
| `coin_azcoin` | AZCOIN daemon, block template, block submit |
| `storage` | Persistence (workers, shares, rounds) |
| `api_server` | Operational API (/health, /ready, /v1/pool/*) |
| `azcoin-pool` | Main binary |

## Quick Start

```bash
# Build
cargo build --release -p azcoin-pool

# Configure
cp deploy/configs/config.example.toml config.toml

# Run
cargo run --release -p azcoin-pool

# API: http://localhost:8080/health
# Ready: http://localhost:8080/ready
# Stratum: tcp://localhost:3333
```

## Configuration

Copy `deploy/configs/config.example.toml` to `config.toml`. See `.env.example` for environment variables.

Important fields for real-stack validation:

- `pool.payout_script_pubkey_hex`: required to arm live block-found submission
- `daemon.job_source_mode = "rpc"`: use azcoind-compatible `getblocktemplate` + `submitblock`
- `daemon.job_source_mode = "api"`: use the existing node API path `GET /v1/az/mining/template/current`
- `daemon.url`: base URL for the selected daemon/node API mode
- `daemon.rpc_user` / `daemon.rpc_password`: JSON-RPC auth for RPC mode
- `daemon.node_api_token`: Bearer token for API mode if required
- For end-to-end `submitblock` validation, use RPC mode unless `daemon.url` serves both the node API and JSON-RPC.

At startup the pool logs whether block-found submission is armed or disabled based on `pool.payout_script_pubkey_hex`.

## Documentation

- [ADR 0001](docs/adr/0001-new-repo-and-layered-architecture.md) — Layered architecture
- [ADR 0002](docs/adr/0002-sv1-first-sv2-ready-boundary.md) — SV1 first, SV2-ready
- [Context](docs/architecture/context.md) — System context
- [Data Flow](docs/architecture/data-flow.md) — Data flow
- [SV2 Bridge](docs/architecture/sv2-future-bridge.md) — Future Stratum V2 integration

## Deployment

- **Docker**: `deploy/docker/`
- **systemd**: `deploy/systemd/azcoin-pool.service`

## Status

Stratum V1 share handling, job sourcing, and guarded block-found submission wiring are present. API `/health` and `/ready` work.

## License

MIT
