FROM debian:bullseye-slim
COPY ./stream_node /bin/stream_node
COPY ./private_chain_setup.sh /bin/private_chain_setup.sh
WORKDIR /bin/
RUN chmod +x private_chain_setup.sh
EXPOSE 5555
ENTRYPOINT ["/bin/private_chain_setup.sh"]
