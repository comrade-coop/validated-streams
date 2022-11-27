#!/bin/sh 
cd $(dirname $0)
./stream_node --execution Native --base-path /tmp/$1 --chain local $1 $2 $3 $4 $5 --validator $6
