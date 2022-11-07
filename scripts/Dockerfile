FROM rust
COPY ./stream_node /bin/node
EXPOSE 5555
ENTRYPOINT ["/bin/node","--unsafe-ws-external"]
