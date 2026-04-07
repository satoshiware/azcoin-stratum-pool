# System Context

## Overview

AZCOIN mining pool is a Stratum-based mining pool that:

1. Accepts miner connections via Stratum V1 (with `mining.configure` version-rolling support)
2. Sources jobs from the AZCOIN daemon via `getblocktemplate` RPC or Node REST API
3. Pushes fresh work to all connected miners when the block template changes
4. Validates shares cryptographically (double-SHA256 against pool difficulty target)
5. Detects block-found shares and submits solved blocks to the daemon
6. Exposes operational APIs for monitoring pool health, workers, jobs, and shares

## External Actors

- **Miners** — Connect via Stratum V1 TCP, receive jobs via `mining.notify`, submit shares via `mining.submit`
- **AZCOIN Node** — Provides block templates (`getblocktemplate` or REST API), accepts block submissions (`submitblock`)
- **Operators** — Query health, stats, workers, current job, and recent shares via HTTP API

## Internal Components

- **Protocol Layer** — `protocol_sv1`: SV1 wire protocol, TCP listener, JSON-RPC dispatch, `tokio::select!` session loop for server-push notify
- **Domain Layer** — `pool_core`: Job, ShareSubmission, ShareResult, ActiveJobRegistry, WorkerIdentity, stats, traits
- **Coin Layer** — `coin_azcoin`: daemon RPC client, block template mapping, coinbase construction, share validation (double-SHA256), block submission
- **Persistence** — `storage`: stubs for workers, shares, rounds (future: PostgreSQL)
- **API** — `api_server`: health, ready, stats, workers, current job, recent shares
- **Application** — `azcoin-pool`: service composition, SV1 session handler, background job poller, startup wiring

## Runtime Architecture

```
                      ┌────────────────────────────┐
                      │      azcoin-pool (main)     │
                      │  job_poller ──broadcast──┐  │
                      └──────────┬───────────────┘  │
                                 │                  │
              ┌──────────────────┼──────────────────┘
              │                  │
              ▼                  ▼
     ┌─────────────┐    ┌──────────────┐
     │ protocol_sv1│    │  api_server   │
     │  SV1 TCP    │    │  HTTP :8080   │
     │  :3333      │    └──────┬───────┘
     └──────┬──────┘           │
            │                  │
            ▼                  ▼
     ┌─────────────┐    ┌─────────────┐
     │  pool_core  │    │  pool_core  │
     │ ShareProc,  │    │  stats,     │
     │ JobRegistry │    │  workers    │
     └──────┬──────┘    └─────────────┘
            │
            ▼
     ┌─────────────┐
     │ coin_azcoin │
     │ RPC client, │
     │ validation, │
     │ submitblock │
     └──────┬──────┘
            │
            ▼
     ┌─────────────┐
     │ AZCOIN Node │
     └─────────────┘
```

## Out of Scope (for now)

- Full payout logic and round management
- Persistent storage (PostgreSQL integration)
- Web frontend / dashboard UI
- Stratum V2 (architecture is ready; `sv2_scaffold` crate exists)
- Variable difficulty adjustment per miner
- ZMQ-based new-block notification (currently poll-based)
