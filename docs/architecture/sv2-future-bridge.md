# Stratum V2 Future Bridge

## Goal

Add Stratum V2 support without rewriting the pool. SV1 and SV2 should share the same domain logic.

## Architecture

```
                    ┌─────────────────┐
                    │   pool_core     │
                    │ (domain models) │
                    └────────┬────────┘
                             │
          ┌──────────────────┼──────────────────┐
          │                  │                  │
          ▼                  ▼                  ▼
   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐
   │ protocol_sv1│   │ protocol_sv2│   │   storage   │
   │ (SV1 wire)  │   │ (SV2 wire)  │   │ (repos)     │
   └─────────────┘   └─────────────┘   └─────────────┘
          │                  │
          └──────────────────┘
                    │
                    ▼
         ShareSubmission, Job, etc.
```

## Integration Points (TODO)

1. **Create `protocol_sv2` crate** — SV2 protocol handling
2. **Protocol adapter trait** — Common interface for both protocols
3. **Main binary** — Wire both listeners, share `JobSource`, `ShareProcessor`
4. **Session routing** — Detect protocol from initial handshake

## Key Invariants

- `protocol_sv1` and `protocol_sv2` both produce `ShareSubmission`
- Both consume `Job` from `JobSource`
- Neither owns `BalanceLedger`, `RoundManager`, or `PayoutExecutor`
