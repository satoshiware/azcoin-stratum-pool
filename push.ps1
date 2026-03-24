git status
git add -A
git commit -m "Implementing mining methods and calls into software V0.1.4-r2"
git tag v0.1.4-r2
git push origin main
git push origin v0.1.4-r2

docker build -f deploy/docker/Dockerfile -t ghcr.io/satoshiware/azcoin-stratum-pool:v0.1.4r2 .
docker push ghcr.io/satoshiware/azcoin-stratum-pool:v0.1.4r2
tag ghcr.io/satoshiware/azcoin-stratum-pool:v0.1.4r2 satoshiware/azcoin-stratum-pool:latest
docker push satoshiware/azcoin-stratum-pool:latest