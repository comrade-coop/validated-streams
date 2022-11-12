#!/bin/sh 
./stream_node --execution Native --base-path /tmp/$1 --chain local --$1 --port $2 --ws-port $3 --rpc-port $4 \
$5 --validator $6
