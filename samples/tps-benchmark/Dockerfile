FROM rust:1-slim-bullseye as build
RUN apt-get update && apt-get install protobuf-compiler -y
WORKDIR /src/
COPY rust-toolchain.toml rust-toolchain.toml
COPY proto proto
COPY samples/tps-benchmark/ samples/tps-benchmark/
WORKDIR /src/samples/tps-benchmark/
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/src/samples/tps-benchmark/target cargo build --release  -Z unstable-options --out-dir=out
FROM debian:bullseye-slim
COPY --from=build /src/samples/tps-benchmark/out/vstreams-tps-benchmark /bin/vstreams-tps-benchmark
ENTRYPOINT ["./bin/vstreams-tps-benchmark"]

