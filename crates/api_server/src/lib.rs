//! Operational HTTP API backed by Axum.
//!
//! Routes:
//! - `GET /health` — Liveness check
//! - `GET /ready` — Readiness check (stub: always OK)
//! - `GET /v1/pool/stats` — Pool stats (hashrate, worker count, round height/status)
//! - `GET /v1/pool/workers` — Registered worker list
//! - `GET /v1/pool/jobs/current` — Current job metadata from `JobSource`
//! - `GET /v1/pool/shares/recent` — Recent share attempts with accept/reject status

pub mod routes;

pub use routes::*;
