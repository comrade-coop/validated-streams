#!/bin/bash

cd "$(dirname "$0")"/../../.. || exit 1

if [ $# -lt 0 ] || [ $# -gt 1 ]; then
  echo "USAGE: $0 [docker-volume]"
  exit 1
fi

DOCKER=docker
DOCKER_VOLUME=${1:-vol-tps-bench}

$DOCKER build -t comradecoop/validated-streams .
$DOCKER build -t comradecoop/validated-streams-tps-bench . -f samples/tps-benchmark/Dockerfile
$DOCKER build -t comradecoop/validated-streams-tps-bench-full samples/tps-benchmark/ -f samples/tps-benchmark/Dockerfile-combined

$DOCKER volume create "$DOCKER_VOLUME"

$DOCKER run --rm -v "$DOCKER_VOLUME":/mnt --entrypoint cp comradecoop/validated-streams-tps-bench-full /bin/{vstreams_node,vstreams_tps_benchmark,tps_bench_setup.sh} /mnt

