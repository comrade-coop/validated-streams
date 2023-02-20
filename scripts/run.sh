#!/bin/bash
docker-compose up -d
echo waiting until all grpc ports in local net are opened
sleep 35
validators=(
  "localhost:5556"
  "localhost:5557"
  "localhost:5558"
  "localhost:5559"
)
#base64 encoded hash for 256 zeroes
#hash_value="ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQ="
for i in {1..10000}; do 
  # create a random hash everytime 
  hash_value=$(openssl rand -base64 32)
  req='{
    "event_id": "'"$hash_value"'"
  }'
		for server in "${validators[@]}"; do
		  RESPONSE=$(grpcurl -plaintext -import-path ../proto -proto streams.proto  -d "$req" "$server" ValidatedStreams.Streams/ValidateEvent)
		  echo $RESPONSE
		done
done
