FROM rust:1-slim-bullseye as build
RUN apt-get update && apt-get install protobuf-compiler -y
WORKDIR /src/
COPY rust-toolchain.toml rust-toolchain.toml
COPY proto proto
COPY samples/TpsBench/ samples/TpsBench/
WORKDIR /src/samples/TpsBench/
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/validated-streams/target cargo build --release
FROM debian:buster-slim
COPY --from=build /tps_bench/target/release/tps_bench /bin/tps_bench
ENTRYPOINT ["./bin/tps_bench"]

