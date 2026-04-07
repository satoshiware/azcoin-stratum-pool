git status
git add -A
git commit -m "Fixing issues invalid merkle branch and not being able to successfully submit a block V0.1.6-r4"
git tag v0.1.6-r4
git push origin main
git push origin v0.1.6-r4

docker build `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:sha-$SHA `
  -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:latest `
  .

docker push ghcr.io/satoshiware/azcoin-stratum-pool:sha-$SHA
docker push ghcr.io/satoshiware/azcoin-stratum-pool:latest