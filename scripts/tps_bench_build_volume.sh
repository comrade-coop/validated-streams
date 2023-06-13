#!/bin/bash

cd "$(dirname "$0")"/.. || exit 1

if [ $# -lt 0 ] || [ $# -gt 1 ]; then
  echo "USAGE: $0 [docker-volume]"
  exit 1
fi

DOCKER=docker
DOCKER_VOLUME=${1:-vol-tps-bench}

#$DOCKER build -t comradecoop/validated-streams .
$DOCKER build -t comradecoop/validated-streams-tps-bench . -f samples/TpsBench/Dockerfile
$DOCKER build -t comradecoop/validated-streams-tps-bench-full . -f samples/TpsBench/Dockerfile-combined

$DOCKER volume create "$DOCKER_VOLUME"

$DOCKER run --rm -v "$DOCKER_VOLUME":/mnt --entrypoint cp comradecoop/validated-streams-tps-bench-full /bin/{stream_node,tps_bench,tps_bench_setup.sh} /mnt

