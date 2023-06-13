FROM rust:1-slim-bullseye as build
RUN apt-get update && apt-get install protobuf-compiler -y
WORKDIR /src/
COPY rust-toolchain.toml rust-toolchain.toml
COPY proto proto
COPY samples/TpsBench/ samples/TpsBench/
WORKDIR /src/samples/TpsBench/
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/src/samples/TpsBench/target cargo build --release  -Z unstable-options --out-dir=out
FROM debian:bullseye-slim
COPY --from=build /src/samples/TpsBench/out/tps_bench /bin/tps_bench
ENTRYPOINT ["./bin/tps_bench"]

