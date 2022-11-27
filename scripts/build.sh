#!/bin/sh 
cd $(dirname $0)
docker rmi stream_node
rm stream_node
ln ../target/release/node stream_node
docker-compose build
