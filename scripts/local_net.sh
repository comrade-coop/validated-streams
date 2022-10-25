#purge old data
../target/release/node purge-chain --base-path /tmp/alice --chain local -y
../target/release/node purge-chain --base-path /tmp/bob --chain local -y
../target/release/node purge-chain --base-path /tmp/charlie --chain local -y
# change kitty with default terminal
kitty ../target/release/node \
--execution Native \
--base-path /tmp/alice \
--chain local \
--alice \
--port 30333 \
--ws-port 9945 \
--rpc-port 9933 \
--node-key 0000000000000000000000000000000000000000000000000000000000000001 \
--telemetry-url "wss://telemetry.polkadot.io/submit/ 0" \
--validator &
kitty ../target/release/node \
--execution Native \
--base-path /tmp/bob \
--chain local \
--bob \
--port 30334 \
--ws-port 9946 \
--rpc-port 9934 \
--telemetry-url "wss://telemetry.polkadot.io/submit/ 0" \
--validator \
--bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp &
kitty ../target/release/node \
--execution Native \
--base-path /tmp/charlie \
--chain local \
--charlie \
--port 30335 \
--ws-port 9947 \
--rpc-port 9935 \
--telemetry-url "wss://telemetry.polkadot.io/submit/ 0" \
--validator \
--bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp

