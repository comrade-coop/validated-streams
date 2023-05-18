#!/bin/bash
cd $(dirname $0)
DOCKER_COMPOSE='docker-compose'
COMMAND=""
for arg in "$@"; do
  shift
  case "$arg" in
    '--podman') DOCKER_COMPOSE='podman-compose' ;;
    *)
      if [ -z "$COMMAND" ]; then
        COMMAND="$arg"
      else
        COMMAND=""; break;
      fi ;;
  esac
done

if [ "$COMMAND" = "stop" ]; then
  $DOCKER_COMPOSE -f docker-compose-example.yml down
elif [ "$COMMAND" = "start" ]; then
  $DOCKER_COMPOSE -f docker-compose-example.yml up -d
  echo waiting until all grpc ports are open
  sleep 35
  validators=(
    "localhost:5556"
    "localhost:5557"
    "localhost:5558"
    "localhost:5559"
  )
  #base64 encoded hash for 256 zeroes
  #hash_value="ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQ="
  for i in {1..10000}; do
    # create a random hash every time
    hash_value=$(openssl rand -base64 32)
    req='{
      "event_id": "'"$hash_value"'"
    }'
      for server in "${validators[@]}"; do
        RESPONSE=$(grpcurl -plaintext -import-path ../proto -proto streams.proto  -d "$req" "$server" ValidatedStreams.Streams/WitnessEvent) &
      done
  done
else
  echo "Usage: $0 COMMAND [--podman]"
  echo ""
  echo "Commands:"
  echo "  start   Start the example network"
  echo "  stop    Stop the example network"
  exit 64
fi
