#!/bin/sh

"$(dirname "$0")/stream_node" --execution Native --base-path "/tmp/$1" --chain local --grpc-port 6000 "$@"
