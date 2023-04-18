#!/bin/bash
timeout 50s ./run-example.sh start &
sleep 10
docker stop validator4 > /dev/null
echo "validator4 disconnected from the network, sleeping for 40 secs"
sleep 40
docker start validator4 > /dev/null
echo "validator4 joined the network"
docker logs -f validator4 2>&1 | grep -E 'Deffered|📥|Retreived'

