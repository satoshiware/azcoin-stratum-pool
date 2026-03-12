# System Context

## Overview

AZCOIN mining pool is a Stratum-based mining pool that:

1. Accepts miner connections (Stratum V1 initially)
2. Distributes jobs from the AZCOIN daemon
3. Validates and records shares
4. Manages rounds and block submission
5. Exposes operational APIs for monitoring

## External Actors

- **Miners** — Connect via Stratum protocol, submit shares
- **AZCOIN Node** — Provides block templates, accepts block submissions
- **Operators** — Query health, stats, workers via HTTP API

## Internal Components

- **Protocol Layer** — `protocol_sv1` (and future `protocol_sv2`)
- **Domain Layer** — `pool_core` (models, traits)
- **Coin Layer** — `coin_azcoin` (daemon, block template, block submit)
- **Persistence** — `storage` (workers, shares, rounds)
- **API** — `api_server` (health, ready, stats, workers)

## Out of Scope (for now)

- Full payout logic
- Web frontend/UI
- Stratum V2 (planned for later)
