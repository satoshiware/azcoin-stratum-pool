# SV2 Local Test Runbook

## Startup

1. Copy `deploy/env/.env.sv2.example` to `deploy/env/.env.sv2`.
2. Review the placeholder values and replace any local RPC credentials as needed.
3. From the repository root, start the scaffold stack:

```bash
docker compose -f deploy/docker-compose.sv2.yml --env-file deploy/env/.env.sv2 up --build
```

## Expected Services

- `azcoind`
- `sv2-translator`
- `sv2-pool`
- `sv2-jd-server`
- `sv2-orchestrator`

## Verify Containers

1. Run `docker compose -f deploy/docker-compose.sv2.yml --env-file deploy/env/.env.sv2 ps`.
2. Confirm all expected services are listed.
3. Check logs with `docker compose -f deploy/docker-compose.sv2.yml --env-file deploy/env/.env.sv2 logs -f`.
4. Confirm `sv2-orchestrator` prints `SV2 orchestrator starting`.

## Miner Connection

TODO: add miner connection steps once translator and SV2 pool configuration is finalized.

## Rollback

Use existing `docker-compose.yml` for V1.
