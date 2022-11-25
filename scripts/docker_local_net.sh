#!/bin/sh 
cd $(dirname $0)
kitty docker run -it --rm -p 5555:5555 -p 9944:9944 --name first stream_node alice 30333 9944 9933 '--node-key 0000000000000000000000000000000000000000000000000000000000000001' --unsafe-ws-external &
kitty docker run -it --rm --name second stream_node bob 30333 9944 9933 '' '' &
kitty docker run -it --rm --name third stream_node charlie 30333 9944 9933 '' '' &
kitty docker run -it --rm --name forth stream_node dave 30333 9944 9933 '' ''
