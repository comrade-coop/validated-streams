#!/bin/bash
cd $(dirname $0)
DOCKER='docker'
DOCKER_COMPOSE='docker-compose'
DOCKER_COMPOSE_FILE='docker-compose-example.yml'
COMMAND=''
FLAGS=''
for arg in "$@"; do
  shift
  case "$arg" in
    '--podman')
      DOCKER='podman'
      DOCKER_COMPOSE='podman-compose' ;;
    '--docker')
      DOCKER='docker'
      DOCKER_COMPOSE='docker-compose' ;;
    '--direct-sample')
      DOCKER_COMPOSE_FILE='docker-compose-example.yml' ;;
    '--irc-sample')
      DOCKER_COMPOSE_FILE='../samples/ValidatedStreams.Irc.TrustedClient/docker-compose-irc.yml' ;;
    '--help')
      COMMAND="help"
      break ;;
    *)
      if [ -z "$COMMAND" ]; then
        COMMAND="$arg"
      else
        echo "Unexpected '$arg'"
        COMMAND=""; break;
      fi ;;
  esac
  if [ "$COMMAND" != "$arg" ]; then
    FLAGS+="$arg"
  fi
done

validators=(
  "localhost:5556"
  "localhost:5557"
  "localhost:5558"
  "localhost:5559"
)

function command_run__cleanup {
  echo
  echo "Note: Run '$0 stop $FLAGS' to stop the example network"
  exit 130
}

function command_run {
  if [ "$DOCKER_COMPOSE_FILE" != "docker-compose-example.yml" ]; then
    echo "'$0 run' can only be used with the --direct-sample sample.";
    echo "Use '$0 start $FLAGS' instead.";
    exit 1;
  fi
  trap command_run__cleanup SIGINT
  command_start
  command_validated &
  command_witness
  kill %1 # command_validated
  command_stop
}

function command_build {
  $DOCKER_COMPOSE -f $DOCKER_COMPOSE_FILE build
}

function command_start {
  $DOCKER_COMPOSE -f $DOCKER_COMPOSE_FILE up -d
}

function command_stop {
  $DOCKER_COMPOSE -f $DOCKER_COMPOSE_FILE down
}

function command_logs {
  $DOCKER_COMPOSE -f $DOCKER_COMPOSE_FILE logs -f | grep -E "💤|🔁|👌|❌"
}

function command_disturb {
  command_stop
  command_start
  wait_bootstrap
  if [ "$DOCKER_COMPOSE_FILE" = "docker-compose-example.yml" ]; then
    witness_events &
  fi
  command_logs &
  echo "********** 🔌 Applying a 60 seconds frequent crash-recovery for validator4 + delayed packets for the rest of validators **********"
  # randomly delay all packet transmissions for all containers with 6 seconds delay time and a variation of 0.5 seconds
  # which makes the delay sometimes more than block production time whilst also having frequent crash-recovery for validator4
  for i in {1..4}; do
    pumba --random netem --duration 8s delay -t 6000 --jitter 500 validator3 validator2 validator1 2>/dev/null &
    docker pause validator4 >/dev/null
    sleep 10
    docker unpause validator4 >/dev/null
    #show output of validator4
    sleep 4
  done
  echo "********** 🔌 Applying packet loss for 30 seconds **********"
  # make validator4 lose 90% of all its packets and select randomly one of the rest of validators and lose 50% of packets
  for i in {1..15}; do
    pumba --random netem --duration 3s loss -p 50 validator3 validator2 validator1 2> /dev/null &
    pumba netem --duration 3s loss -p 90 validator4 2> /dev/null
  done
  echo "********** 😵 Emulating a 1 minute crash-recovery fault for validator4 **********"
  docker pause validator4 >/dev/null
  sleep 60
  echo "********** 🔗 Restoring validator4 **************"
  docker unpause validator4 >/dev/null
  wait
}
function wait_bootstrap {
  echo "Waiting for all validators to start up"
  for server in "${validators[@]}"; do
    for i in {1..35}; do
      #base64 encoded hash for 256 zeroes
      req='{
        "event_id": "'"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="'"
      }'
      if grpcurl -plaintext -import-path ../proto -proto streams.proto -d "$req" "$server" ValidatedStreams.Streams/WitnessEvent >/dev/null 2>&1; then
        break
      else
        sleep 1
      fi
    done
  done
}
function witness_events {
  echo "Witnessing events"
  for i in {1..10000}; do
    # create a random hash every time
    hash_value=$(openssl rand -base64 32)
    req='{
      "event_id": "'"$hash_value"'"
    }'
    for server in "${validators[@]}"; do
      grpcurl -plaintext -import-path ../proto -proto streams.proto -d "$req" "$server" ValidatedStreams.Streams/WitnessEvent >/dev/null 2>&1 & #redirect all errors to null
    done
  done
  wait
}
function command_witness {
  if [ "$DOCKER_COMPOSE_FILE" != "docker-compose-example.yml" ]; then
    echo "'$0 witness' can only be used with the --direct-sample sample.";
    exit 1;
  fi
  wait_bootstrap
  witness_events &
  command_logs
}

function command_validated {
  grpcurl -plaintext -import-path ../proto -proto streams.proto -d "{}" "${validators[0]}" ValidatedStreams.Streams/ValidatedEvents
}

case "$COMMAND" in
  'run') command_run ;;
  'build') command_build ;;
  'start') command_start ;;
  'stop') command_stop ;;
  'logs') command_logs ;;
  'witness') command_witness ;;
  'validated') command_validated ;;
  'disturb') command_disturb ;;
  *)
    echo "Usage: $0 COMMAND [flags]"
    echo ""
    echo "Flags:"
    echo "  --docker        Use docker / docker-compose to run the samples (default)"
    echo "  --podman        Use podman / podman-compose"
    echo "  --direct-sample Run the 'direct' sample, witnessing events directly to a network of validators"
    echo "  --irc-sample    Run the 'irc' sample, witnessing events submitted to an irc channel (localhost:6667#validated-stream)"
    echo ""
    echo "Commands:"
    echo "  run       Run the sample (equivalent to running start; validated & witness; stop)"
    echo "  build     Build the container image"
    echo "  start     Start the example network"
    echo "  witness   Witness a lot of random events to the example network"
    echo "  validated Show the events finalized by the example network"
    echo "  logs      Display logs from the example network"
    echo "  stop      Stop the example network"
    echo "  disturb   Start the network Resiliency example"
    exit 64 ;;
esac
