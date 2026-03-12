# ADR 0001: New Repo and Layered Architecture

## Status

Accepted

## Context

We are building a production AZCOIN mining pool. The previous `azcoin-stratum-gateway` was a custom project; this new repo (`azcoin-stratum-pool`) is the production direction and should not inherit custom gateway assumptions.

We need a clean, modular architecture that:
- Supports Stratum V1 first
- Allows Stratum V2 to be added later via adapter boundaries
- Follows proven mining-pool concepts (Miningcore-style)
- Avoids monolith design and tight coupling

## Decision

Create a layered Rust workspace with these crates:

1. **common** — Shared types, errors, config, tracing. No business logic.
2. **protocol_sv1** — Stratum V1 wire protocol only. No balances, payouts, rounds.
3. **pool_core** — Protocol-agnostic domain logic. Models and traits.
4. **coin_azcoin** — AZCOIN-specific integration (daemon, block template, block submit).
5. **storage** — Persistence layer. PostgreSQL-oriented, minimal for now.
6. **api_server** — Operational API (/health, /ready, /v1/pool/*).
7. **azcoin-pool** — Main binary wiring everything together.

Dependencies flow: `common` ← `protocol_sv1`, `pool_core`, `coin_azcoin`, `storage`, `api_server` ← `azcoin-pool`.

## Consequences

- Clear separation of concerns
- Protocol adapters can be swapped (SV2 later) without touching pool logic
- Each crate has a single responsibility
- Bootstrap can proceed incrementally
