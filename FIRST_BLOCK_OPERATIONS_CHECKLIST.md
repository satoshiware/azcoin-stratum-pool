# FIRST BLOCK OPERATIONS CHECKLIST

## Purpose
Operational checklist to confirm AZCoin pool first-block success end-to-end:

miner -> share accepted -> block share detected -> solved header reconstructed -> submitblock attempt -> daemon acceptance

---

## Current known-good assumptions
- Pool container: `azcoin-pool`
- Daemon container: `azcoind`
- Compose file: `deploy/docker/docker-compose.yml`
- Pool startup confirms:
  - `block-found submission is armed: pool payout scriptPubKey is configured`
- Share validator logs are live
- Accepted shares are flowing
- No evidence yet of `ShareResult::Block`

---

## 1) Basic pool health check

Run:
```bash
cd ~/repos/azcoin-local-stack
docker logs --since=10m azcoin-pool 2>&1 | tail -n 200