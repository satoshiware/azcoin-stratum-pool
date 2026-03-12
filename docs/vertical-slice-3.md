# Vertical Slice 3: mining.notify + Job Abstraction Wiring

## Summary

After successful SV1 subscribe/authorize, send a valid stub `mining.notify` based on an internal `Job` abstraction. Add `/v1/pool/jobs/current` API.

## What Was Implemented

### 1. pool_core

- **Job model** — Refined with SV1-aligned fields: job_id, prev_hash, coinbase_part1, coinbase_part2, merkle_branch, version, nbits, ntime, clean_jobs
- **Job::placeholder()** — Constructor for stub job
- **StubJobSource** — Returns `Some(Job::placeholder())` instead of `None`
- **PoolServices** — `job_source` typed as `Arc<dyn JobSource>`

### 2. protocol_sv1

- **notify.rs** — `build_mining_notify(job: &Job)` builds `mining.notify` JSON-RPC notification
- **SessionEventHandler** — `on_authorize` now returns `Result<Option<Job>, String>`; `Some(job)` triggers notify
- **server.rs** — After authorize success, if `Some(job)`, send notify then response
- **pool_core** — Added as dependency for Job type

### 3. azcoin-pool main

- **Sv1SessionHandler** — Holds `job_source`, returns `job_source.current_job().await` from `on_authorize`
- Structured logging: "job dispatched" when notify is sent

### 4. api_server

- **GET /v1/pool/jobs/current** — Returns current job metadata (job_id, prev_hash, version, nbits, ntime, clean_jobs as JSON)

### 5. Testing

- **sv1_subscribe_authorize** — Reads two lines after authorize; first is mining.notify, second is authorize response; asserts notify params
- **pool_jobs_current_returns_stub_job** — API test for `/v1/pool/jobs/current`

## Architecture Choices

- **on_authorize returns Option<Job>** — Handler fetches job from JobSource; protocol layer builds and sends notify. Keeps protocol_sv1 responsible for wire format.
- **protocol_sv1 depends on pool_core** — For Job type only. Domain types stay in pool_core; wire format in protocol.
- **Prev hash reversed** — Stratum V1 uses little-endian hex; `hex_encode_reversed` handles this.
- **Notify before response** — Server push (notify) then request-response (authorize). Matches typical Stratum flow.

## Commands

```bash
cargo build -p azcoin-pool
cargo test -p azcoin-pool
cargo clippy --workspace -- -D warnings
cargo run -p azcoin-pool
```

## Next Slice: mining.submit + Share Abstraction Wiring

- Parse `mining.submit` in protocol_sv1
- Map to `ShareSubmission` domain type
- Add `ShareProcessor` stub that returns `ShareResult::Rejected` for all
- Wire `ShareProcessor` into SV1 handler
- Session state: track authorized worker per connection for submit
