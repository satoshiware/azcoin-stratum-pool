git status
git add -A
git commit -m "Implementing SV2 support and starting stronger logging for miners V0.1.6-r1"
git tag v0.1.6-r1
git push origin main
git push origin v0.1.6-r1

docker build `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:sha-$SHA `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:latest `
  .

docker push ghcr.io/satoshiware/azcoin-stratum-pool:sha-$SHA
docker push ghcr.io/satoshiware/azcoin-stratum-pool:latest