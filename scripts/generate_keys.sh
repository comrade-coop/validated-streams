#!/bin/bash
cd ..
rm aura_keys grandpa_keys keys
for ((i=1; i<=32; i++))
do
 output=$(./target/release/node key generate --scheme Sr25519 --password $i)
  secret_phrase=$(echo "$output" | awk -F ': ' '/Secret phrase:/ { print $2 }')
  aura_key=$(echo "$output" | awk -F ': ' '/SS58 Address:/ { print $2 }')
  output2=$(./target/release/node key inspect --password $i --scheme Ed25519 "$secret_phrase")
  grandpa_key=$(echo "$output2" | awk -F ': ' '/SS58 Address:/ { print $2 }')

  echo -e "\n Account $i Aura:$aura_key    Grandpa:$grandpa_key" >> keys
  echo -e "\n Account $i Aura:$aura_key    Grandpa:$grandpa_key"
  echo -e "\n $output" >> aura_keys
  echo -e "\n $output2" >> grandpa_keys
done
