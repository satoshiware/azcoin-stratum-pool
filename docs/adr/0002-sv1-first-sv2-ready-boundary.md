# ADR 0002: SV1 First, SV2-Ready Boundary

## Status

Accepted

## Context

Stratum V1 is the current standard for mining pools. Stratum V2 is emerging but not yet widely adopted. We must ship SV1 first while ensuring SV2 can be added without a rewrite.

## Decision

1. **protocol_sv1** handles only Stratum V1 wire protocol:
   - Listener, session, message types, parsing, serialization
   - Maps SV1 requests into internal domain commands (`Sv1DomainCommand`)
   - Does NOT own balances, payouts, or round accounting

2. **pool_core** is protocol-agnostic:
   - Defines `WorkerIdentity`, `MinerSession`, `Job`, `ShareSubmission`, `ShareResult`, etc.
   - Defines traits: `JobSource`, `ShareProcessor`, `RoundManager`, `BlockSubmitter`, etc.
   - No dependency on Stratum V1 or V2

3. **Future SV2 adapter** will:
   - Live in a new crate (e.g. `protocol_sv2`)
   - Implement the same domain interfaces (produce `ShareSubmission`, consume `Job`)
   - Share `pool_core` with `protocol_sv1`
   - Be wired via a protocol adapter layer in the main binary

4. **TODO markers** in code indicate where SV2 integration points will be added.

## Consequences

- SV1 and SV2 can coexist in the same pool
- Protocol logic stays isolated
- Domain types stay stable across protocol changes
