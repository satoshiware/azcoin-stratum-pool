# Vertical Slice 4: mining.submit + ShareSubmission Wiring

## Summary

Add `mining.submit` path: parse share submissions, attach to authorized session state, route into `ShareSubmission`, return structured accept/reject. Conventional order: response, set_difficulty, notify.

## What Was Implemented

### 1. pool_core

- **ShareSubmission** — Confirmed (job_id, worker, extra_nonce2, ntime, nonce)
- **ShareResult** — Confirmed (Accepted, Block, Rejected { reason })
- **ShareProcessor** — Confirmed trait
- **StubShareProcessor** — Rejects all with "share validation not implemented"
- **RecentSharesBuffer** — In-memory buffer of recent (share, result) for API
- **PoolServices** — Added share_processor, recent_shares

### 2. protocol_sv1

- **SessionState** — Per-connection: authorized_worker, subscribed, extranonce1, extranonce2_size
- **SubmitShare** — Added username to domain command for verification
- **mining.submit** — Full handling: check authorized, verify username match, build ShareSubmission, call on_submit
- **SessionEventHandler** — Added async fn on_submit(share) -> ShareResult
- **Conventional order** — Response first, then set_difficulty (stub), then notify
- **build_submit_success**, **build_submit_reject** — Response builders

### 3. azcoin-pool main

- **Sv1SessionHandler** — Holds share_processor, implements on_submit
- Structured logging: worker, job_id, accepted, reject_reason

### 4. api_server

- **GET /v1/pool/shares/recent** — Returns recent share attempts from stub processor

### 5. Testing

- **sv1_subscribe_authorize** — Subscribe, authorize, verify response/set_difficulty/notify order, submit placeholder share, verify rejection, verify share in API
- **pool_shares_recent_returns_array** — API test for empty recent shares

## Architecture Choices

- **Session state per connection** — SessionState tracks authorized_worker; submit uses it, not params
- **Username verification** — Submit params include username; must match session's authorized worker
- **Response before notify** — Conventional Stratum order
- **RecentSharesBuffer** — Shared between StubShareProcessor and API; processor records on each process_share
- **on_submit callback** — Handler calls ShareProcessor; protocol layer stays independent of business logic

## Commands

```bash
cargo build -p azcoin-pool
cargo test -p azcoin-pool
cargo clippy --workspace -- -D warnings
```

## Next Slice: Real Job Source from AZCOIN Daemon + Share Prevalidation

- Implement DaemonClient getblocktemplate
- Convert block template to Job
- Wire AzcoinBlockTemplateProvider to PoolServices
- Add share prevalidation boundaries (job exists, format checks) before full validation
