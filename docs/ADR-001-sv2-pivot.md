# ADR-001: Adopt SV2 via translator-first architecture

## Status

Accepted

## Decision

Adopt SV2 via translator-first architecture.

## Why

The current Stratum V1 path is working, but it keeps protocol and pool behavior coupled around the older centralized model. Stratum V2 moves toward a more modular architecture with clearer boundaries between translator, pool, and future job declaration roles. A translator-first pivot lets us stand up that shape without disrupting the existing V1 stack.

## What We Keep From V1

- Share validation
- RPC integration
- Payout logic

## What We Are Adding

- Translator
- SV2 pool
- JD server (future)

## Non-Goals

- No removal of V1 this week
- No rewriting validation logic
- No production rollout yet

## Notes

This ADR establishes the boot path and repository scaffold only. Existing V1 behavior remains the active mining path.
