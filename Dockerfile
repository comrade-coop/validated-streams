# syntax=docker/dockerfile:1.3-labs

FROM rust:1-slim-bullseye AS build
RUN apt-get update && apt-get install -y cmake libprotobuf-dev protobuf-compiler clang git iproute2
COPY . /validated-streams/
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/validated-streams/target cd /validated-streams/ && cargo build --release -Z unstable-options --out-dir=out

FROM debian:bullseye-slim AS runtime
COPY --from=build /validated-streams/out/vstreams_node /bin/vstreams_node

EXPOSE 6000

# via https://www.fosslinux.com/35730
HEALTHCHECK \
  --interval=60s \
  --timeout=3s \
  --start-period=60s \
  --retries=3 CMD \
    bash -c 'echo > /dev/tcp/127.0.0.1/6000'

ENTRYPOINT ["/bin/vstreams_node", "--execution", "Native"]
CMD ["--dev", "--grpc-addr", "0.0.0.0:6000"]
