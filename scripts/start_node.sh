#!/bin/bash
function stop_processes {
  pkill -f start_node
}
trap stop_processes SIGINT
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "USAGE: $0 <password: 1..32> [bootnodes]"
  echo "Example: $0 1 /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp"
  echo "Check the logs to retreive the local node identity to pass it in bootnodes multiaddr"
  exit 1
fi
echo "Press Ctrl+C to quit."
../target/release/node purge-chain --base-path /tmp/node$1 --chain ../customSpecRaw.json -y
command="../target/release/node \
  --base-path /tmp/node$1 \
  --chain ../customSpecRaw.json \
  --port 30333 \
  --ws-port 9945 \
  --rpc-port 9933 \
  --validator \
  --rpc-methods Unsafe \
  --name validator$1 \
  --password $1 \
  --pool-limit 23000 \
  --gossip-port 15000 "

if [ $# -eq 2 ]; then
  addr=$(echo "$2" | cut -d'/' -f3)
  echo $addr
  command+=" --bootnodes $2"
  command+=" --peers-multiaddr /ip4/$addr/tcp/15000"
fi

eval $command
