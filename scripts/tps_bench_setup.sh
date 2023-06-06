#!/bin/bash
function stop_processes {
  kill $(jobs -p) &> /dev/null
}
set -e

trap stop_processes SIGINT
if [ $# -lt 5 ] || [ $# -gt 6 ]; then
  echo "USAGE: $0 <path/to/node> <path/to/client> <path/to/chainspec> <id: 1..32> <secret-phrase> [bootnode-or-node-key]"
  echo "Example: $0 ../target/release/node ../samples/TpsBench/target/release/tps_bench 1 \"<1's secret phrase>\" 173b2adc7bd10ac4575cd31428ca3049dcf6a5dc675b30fd8140ccd47b2e92ad"
  echo "       : $0 ../target/release/node ../samples/TpsBench/target/release/tps_bench 2 \"<2's secret phrase>\" /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWC11J8smiZvWoovfd28aM7SE5twyJTKEh8cEz8jguwR6i"
  exit 1
fi

NODE_COMMAND=$1
CLIENT_COMMAND=$2
CHAINSPEC_PATH=$3
ID=$4
SECRET_PHRASE=$5
BOOTNODE=$6

echo "Press Ctrl+C to quit."
$NODE_COMMAND purge-chain --base-path "/tmp/node$ID" --chain "$CHAINSPEC_PATH" -y

$NODE_COMMAND key insert --base-path "/tmp/node$ID" --chain "$CHAINSPEC_PATH" --suri "$SECRET_PHRASE" --password "$ID" --scheme Sr25519 --key-type aura
$NODE_COMMAND key insert --base-path "/tmp/node$ID" --chain "$CHAINSPEC_PATH" --suri "$SECRET_PHRASE" --password "$ID" --scheme Ed25519 --key-type gran

ARGS=(
  --base-path "/tmp/node$ID"
  --chain "$CHAINSPEC_PATH"
  --port 30333
  --ws-port 9945
  --rpc-port 9933
  --validator
  --rpc-methods Unsafe
  --name "validator$ID"
  --password "$ID"
  --pool-limit 23000
  --grpc-port 6000
  --gossip-port 15000
)

if [ "$BOOTNODE" != "" ]; then
  if [[ "$BOOTNODE" =~ "/" ]]; then
    ADDR=$(echo "$BOOTNODE" | cut -d'/' -f3)
    ARGS+=(
      --bootnodes "$BOOTNODE"
      --peers-multiaddr "/ip4/$ADDR/tcp/15000"
    )
  else
    ARGS+=(
      --node-key "$BOOTNODE"
    )
  fi
fi
$NODE_COMMAND "${ARGS[@]}" &
sleep 10
$CLIENT_COMMAND http://127.0.0.1:6000 2 2 10000
sleep 10
stop_processes

wait
