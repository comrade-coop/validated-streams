#!/bin/bash
#docker-compose up -d
#echo waiting until all grpc ports in local net are opened
#sleep 20
validators=(
  "localhost:5556"
  "localhost:5557"
  "localhost:5558"
  "localhost:5559"
)
#base64 encoded hash for 256 zeroes
hash_value="AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
req='{
  "event_id": "'"$hash_value"'"
}'

for server in "${validators[@]}"; do

  RESPONSE=$(grpcurl -plaintext -import-path ../proto -proto streams.proto  -d "$req" "$server" ValidatedStreams.Streams/ValidateEvent)
  echo Server: $server
  echo "$RESPONSE"
  echo
done

