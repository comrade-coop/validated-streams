docker rmi stream_node
rm stream_node
ln ../target/release/node stream_node
docker build -t stream_node .
