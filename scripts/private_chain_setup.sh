#!/bin/sh
cd $(dirname $0)
./stream_node --execution Native --base-path /tmp/$1 --chain local $1 --grpc-port 6000 $2 $3 --kademlia-disjoint-query-paths
