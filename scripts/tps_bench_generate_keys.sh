#!/bin/bash

if [ $# -lt 2 ] || [ $# -gt 2 ]; then
  echo "USAGE: $0 <path/to/node> <path/to/chainspec-to-generate.json>"
  echo "Example: $0 target/release/node chainSpecRaw.json"
  echo "The command would output a bunch of scripts/tps_bench_setup.sh command-line invocations that would need to be run later on each target machine."
  echo "Those invocations expect NODE_COMMAND, CLIENT_COMMAND and FIRST_MACHINE to be set to, respectively, the path to the validated streams node binary, the TPS benchmark binary, and the IP of the first machine."
  exit 1
fi

NODE_COMMAND=$1
SPEC_PATH=$2

$NODE_COMMAND build-spec --disable-default-bootnode > "$SPEC_PATH.init" 2>/dev/null

JQ_FILTERS=". "

JQ_FILTERS+='| .name = "Benchmarking"'
JQ_FILTERS+='| .id = "benchmarking_testnet"'
JQ_FILTERS+='| .genesis.runtime.aura.authorities = []'
JQ_FILTERS+='| .genesis.runtime.grandpa.authorities = []'

bootnode_key=$($NODE_COMMAND key generate-node-key 2>/dev/null)
bootnode="/ip4/\$FIRST_MACHINE/tcp/30333/p2p/$(echo "$bootnode_key" | $NODE_COMMAND key inspect-node-key)"


for ((i=1; i<=32; i++)); do
  output=$($NODE_COMMAND key generate --scheme Sr25519 --password $i)
  secret_phrase=$(echo "$output" | awk -F ': ' '/Secret phrase:/ { gsub(/^ +/, "", $2); print $2 }')
  aura_key=$(echo "$output" | awk -F ': ' '/SS58 Address:/ { gsub(/ /, "", $2); print $2 }')
  output2=$($NODE_COMMAND key inspect --password $i --scheme Ed25519 "$secret_phrase")
  grandpa_key=$(echo "$output2" | awk -F ': ' '/SS58 Address:/ { gsub(/ /, "", $2); print $2 }')

  JQ_FILTERS+='| .genesis.runtime.aura.authorities += ["'$aura_key'"]'
  JQ_FILTERS+='| .genesis.runtime.grandpa.authorities += [["'$grandpa_key'", 1]]'

  if [ "$i" -eq 1 ]; then
    bootnode_or_key=$bootnode_key
  else
    bootnode_or_key=$bootnode
  fi

  echo
  echo "scripts/tps_bench_setup.sh \$NODE_COMMAND \$CLIENT_COMMAND $SPEC_PATH $i \"$secret_phrase\" $bootnode_or_key"
done

jq "$JQ_FILTERS" "$SPEC_PATH.init" >"$SPEC_PATH.full"
rm "$SPEC_PATH.init"
$NODE_COMMAND build-spec --chain "$SPEC_PATH.full" --raw >"$SPEC_PATH" 2>/dev/null
#rm "$SPEC_PATH.full"
