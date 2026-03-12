# Data Flow

## Miner → Pool

1. Miner connects via Stratum (TCP)
2. Miner sends `mining.subscribe` → pool responds with session info
3. Miner sends `mining.authorize` → pool creates/validates worker
4. Pool sends `mining.notify` (job) when job available
5. Miner sends `mining.submit` (share) → pool validates
6. Pool responds with accept/reject

## Share Processing

1. `protocol_sv1` parses `mining.submit` → `Sv1DomainCommand::SubmitShare`
2. Domain mapper converts to `ShareSubmission` (with `WorkerIdentity`)
3. `ShareProcessor` validates share (difficulty, duplicate, etc.)
4. `ShareRepository` stores share
5. `BalanceLedger` credits worker (when implemented)

## Block Flow

1. `JobSource` (via `coin_azcoin` daemon) fetches block template
2. Template converted to `Job`, distributed to miners
3. Miner finds valid block → submits share with block solution
4. `ShareProcessor` detects block → produces `BlockCandidate`
5. `BlockSubmitter` submits to daemon

## API Flow

- `/health` — Liveness check
- `/ready` — Readiness (e.g. DB connected)
- `/v1/pool/stats` — Pool stats (hashrate, workers, round)
- `/v1/pool/workers` — Worker list
