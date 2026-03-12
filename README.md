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

# Run (uses defaults if config.toml missing)
cargo run -p azcoin-pool

# API: http://localhost:8080/health
# Stratum: tcp://localhost:3333 (when implemented)
```

## Configuration

Copy `deploy/configs/config.example.toml` to `config.toml`. See `.env.example` for environment variables.

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

Bootstrap scaffold. Stratum listener, share processing, and block submission are stubbed. API `/health` and `/ready` work.

## License

MIT
