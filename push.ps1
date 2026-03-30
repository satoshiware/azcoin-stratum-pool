git status
git add -A
git commit -m "Implementing mining methods and calls into software and fixing some bugs V0.1.4-r4"
git tag v0.1.4-r4
git push origin main
git push origin v0.1.4-r4

docker build `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:sha-$SHA `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:stable `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:latest `
  .

docker push ghcr.io/satoshiware/azcoin-stratum-pool:sha-$SHA
docker push ghcr.io/satoshiware/azcoin-stratum-pool:stable
docker push ghcr.io/satoshiware/azcoin-stratum-pool:latest