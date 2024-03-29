#!/bin/bash

# SPDX-License-Identifier: MIT

if [ $# -lt 3 ] || [ $# -gt 6 ]; then
  echo "USAGE: $0 <path/to/vstreams-node|docker> <path/to/chainspec-to-generate.json> <output format> [node count]"
  echo "Example: $0 target/release/vstreams-node chainSpecRaw.json setup"
  echo "Example: $0 docker /tmp/chainSpecRaw.json compose-vol"
  echo "'docker' as path to node:"
  echo "  Uses the vstreams-node binary found in the comradecoop/validated-streams image through docker."
  echo "'setup' format:"
  echo "  The command would output a bunch of scripts/tps_bench_setup.sh command-line invocations that would need to be run later on each target machine."
  echo "  Those invocations expect NODE_COMMAND, CLIENT_COMMAND and FIRST_MACHINE to be set to, respectively, the path to the validated streams node binary, the TPS benchmark binary, and the IP of the first machine."
  echo "'compose' format:"
  echo "  The command would output a docker-compose invocation."
  echo "  The chainspec will be linked as a docker-compose config."
  echo "'compose-vol' format:"
  echo "  The command would output a docker-compose invocation which uses a default debian image together with a docker volume named 'vol-tps-bench'."
  echo "  See build_volume.sh for a way to create that volume."
  echo "  The chainspec will be copied into the output volume as 'chainspec.json'."
  echo "'compose-vol-remote' format:"
  echo "  The command would output a docker-compose invocation which uses a default debian image together with a docker volume named 'vol-tps-bench'."
  echo "  The difference with compose-vol format is that the debian image acts as proxy for a remote machine that is actually executing the application."
  echo "  The command takes as additional last arguments: file with list of remote hosts (should be equal to the count of nodes) and name of SSH key." 
  echo "  See build_volume.sh for a way to create that volume."
  echo "  The chainspec will be copied into the output volume as 'chainspec.json'."
  exit 1
fi

DOCKER=docker
NODE_COMMAND=$1
NODE_COMMAND_O=$1
SPEC_PATH=$2
FORMAT=$3
COUNT=${4:-32}
SSH_HOSTS=$5
SSH_KEY=$6

if [ "$NODE_COMMAND_O" = "docker" ]; then
  $DOCKER rm -f vs-helper &>/dev/null
  if [ "$FORMAT" = "compose-vol" ] || [ "$FORMAT" = "compose-vol-remote" ]; then
    DOCKER_VOLUME=vol-tps-bench
  else
    DOCKER_VOLUME=$($DOCKER volume create)
  fi
  $DOCKER run -d -v "$DOCKER_VOLUME:/data" --name vs-helper --entrypoint sleep comradecoop/validated-streams infinity >/dev/null
  NODE_COMMAND="$DOCKER exec -i vs-helper /bin/vstreams-node"
fi

$NODE_COMMAND build-spec --disable-default-bootnode -lerror > "$SPEC_PATH.init"

JQ_FILTERS=". "

JQ_FILTERS+='| .name = "Benchmarking"'
JQ_FILTERS+='| .id = "benchmarking_testnet"'
JQ_FILTERS+='| .genesis.runtime.aura.authorities = []'
JQ_FILTERS+='| .genesis.runtime.grandpa.authorities = []'

if [ "$FORMAT" = "setup" ]; then
  bootnode_key=$($NODE_COMMAND key generate-node-key 2>/dev/null)
  bootnode="/ip4/\$FIRST_MACHINE/tcp/30333/p2p/$(echo "$bootnode_key" | $NODE_COMMAND key inspect-node-key)"
fi
if [[ "$FORMAT" =~ "compose" ]]; then
  echo "services:"
fi

for ((i=1; i<=$COUNT; i++)); do
  output=$($NODE_COMMAND key generate --scheme Sr25519 --password $i)
  secret_phrase=$(echo "$output" | awk -F ': ' '/Secret phrase:/ { gsub(/^ +/, "", $2); print $2 }')
  aura_key=$(echo "$output" | awk -F ': ' '/SS58 Address:/ { gsub(/ /, "", $2); print $2 }')
  output2=$($NODE_COMMAND key inspect --password $i --scheme Ed25519 "$secret_phrase")
  grandpa_key=$(echo "$output2" | awk -F ': ' '/SS58 Address:/ { gsub(/ /, "", $2); print $2 }')

  JQ_FILTERS+='| .genesis.runtime.aura.authorities += ["'$aura_key'"]'
  JQ_FILTERS+='| .genesis.runtime.grandpa.authorities += [["'$grandpa_key'", 1]]'

  if [ "$FORMAT" = "setup" ]; then
    if [ "$i" -eq 1 ]; then
      bootnode_or_key=$bootnode_key
    else
      bootnode_or_key=$bootnode
    fi

    echo "scripts/tps_bench_setup.sh \$NODE_COMMAND \$CLIENT_COMMAND $SPEC_PATH $i \"$secret_phrase\" $bootnode_or_key"
  elif [[ "$FORMAT" =~ "compose" ]]; then
    machine_ip="172.20.0.$(($i + 1))"
    if [ "$FORMAT" = "compose-vol-remote" ]; then
      machine_ip=$(sed "${i}q;d" $SSH_HOSTS)
    fi
    bootnode_key=$($NODE_COMMAND key generate-node-key 2>/dev/null)
    bootnode="/ip4/$machine_ip/tcp/30333/p2p/$(echo "$bootnode_key" | $NODE_COMMAND key inspect-node-key)"
    JQ_FILTERS+='| .bootNodes += ["'$bootnode'"]'
    node_conf="$i \"$secret_phrase\" $bootnode_key"
    echo "  node$i:"
    if [ "$FORMAT" = "compose" ]; then
      echo '    image: comradecoop/validated-streams-tps-bench-full'
      echo "    command: /chainspec $node_conf"
      echo "    configs:"
      echo "      - chainspec"
      echo '    restart: on-failure'
      echo '    networks:'
      echo '     tpsnetwork:'
      echo "       ipv4_address: $machine_ip"
    elif [ "$FORMAT" = "compose-vol" ]; then
      echo '    image: debian:bullseye'
      echo "    command: /mnt/tps_bench_setup.sh /mnt/vstreams-node /mnt/vstreams-tps-benchmark /mnt/chainspec.json $node_conf"
      echo "    volumes:"
      echo '      - "vol-tps-bench:/mnt/:ro"'
      echo '    restart: on-failure'
      echo '    networks:'
      echo '     tpsnetwork:'
      echo "       ipv4_address: $machine_ip"
    elif [ "$FORMAT" = "compose-vol-remote" ]; then
      echo '    image: kroniak/ssh-client'
      echo "    command: /mnt/tps_bench_setup_remote.sh $machine_ip $SSH_KEY /mnt/vstreams-node /mnt/vstreams-tps-benchmark /mnt/chainspec.json $node_conf 100"
      echo "    volumes:"
      echo '      - "vol-tps-bench:/mnt/:ro"'
      echo '      - "${HOME}/.ssh:/home/root/.ssh"'
      echo '    restart: on-failure'
    else
      echo "Unknown format: $FORMAT"
      exit 1
    fi
  else
    echo "Unknown format: $FORMAT"
    exit 1
  fi
done

jq "$JQ_FILTERS" "$SPEC_PATH.init" >"$SPEC_PATH.full"
rm "$SPEC_PATH.init"

if [ "$NODE_COMMAND_O" = "docker" ]; then
  $DOCKER cp "$SPEC_PATH.full" vs-helper:/data/chainspec.json.full >/dev/null

  $DOCKER exec -i vs-helper bash -c '/bin/vstreams-node build-spec --chain /data/chainspec.json.full --raw -lerror >/data/chainspec.json' >/dev/null

  $DOCKER cp vs-helper:/data/chainspec.json "$SPEC_PATH"

  $DOCKER rm -f vs-helper >/dev/null

  if [ "$FORMAT" != "compose-vol" ] && [ "$FORMAT" != "compose-vol-remote" ]; then
    $DOCKER volume remove "$DOCKER_VOLUME"
  fi
else
  $NODE_COMMAND build-spec --chain "$SPEC_PATH.full" --raw -lerror >"$SPEC_PATH"

  if [ "$FORMAT" = "compose-vol" ]; then
    $DOCKER run -v vol-tps-bench:/data --name vs-helper busybox true >/dev/null
    $DOCKER cp $SPEC_PATH vs-helper:/data/chainspec.json >/dev/null
    $DOCKER rm vs-helper >/dev/null
  fi
fi

if [ "$FORMAT" = "compose" ]; then
echo 'networks:'
echo '  tpsnetwork:'
echo '    driver: bridge'
echo '    ipam:'
echo '      config:'
echo '        - subnet: 172.20.0.0/16'
echo 'configs:'
echo '  chainspec:'
echo "    file: $SPEC_PATH"
elif [ "$FORMAT" = "compose-vol" ]; then
echo 'networks:'
echo '  tpsnetwork:'
echo '    driver: bridge'
echo '    ipam:'
echo '      config:'
echo '        - subnet: 172.20.0.0/16'
echo 'volumes:'
echo '  vol-tps-bench:'
echo '    external: true'
elif [ "$FORMAT" = "compose-vol-remote" ]; then
echo 'volumes:'
echo '  vol-tps-bench:'
echo '    external: true'
fi
#rm "$SPEC_PATH.full"
