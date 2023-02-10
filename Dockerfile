# syntax=docker/dockerfile:1.3-labs

FROM rust:1-slim-bullseye AS build
RUN apt-get update && apt-get install -y cmake libprotobuf-dev protobuf-compiler clang
COPY . /validated-streams/
#RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/validated-streams/target cd /validated-streams/ && cargo fetch
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/validated-streams/target cd /validated-streams/ && cargo build --release -Z unstable-options --out-dir=out

FROM debian:bullseye-slim AS runtime
COPY --from=build /validated-streams/out/node /bin/stream_node
COPY ./scripts/private_chain_setup.sh /bin/private_chain_setup.sh
WORKDIR /bin/
RUN chmod +x private_chain_setup.sh
EXPOSE 5555
ENTRYPOINT ["/bin/private_chain_setup.sh"]


# FROM rust:1 AS chef
# RUN cargo install cargo-chef
# RUN apt-get update && apt-get install -y cmake libprotobuf-dev protobuf-compiler clang
# WORKDIR app
#
# FROM chef AS plan
# COPY . .
# RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/rustup cargo chef prepare  --recipe-path recipe.json
#
# FROM chef AS build
# COPY --from=plan /app/recipe.json /app/rust-toolchain.toml .
# RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/app/target cargo chef cook --release --recipe-path recipe.json
# COPY . .
# RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/rustup --mount=type=cache,target=/app/target cargo build --release -Z unstable-options --out-dir=out

# # We do not need the Rust toolchain to run the binary!
# FROM debian:buster-slim AS runtime
# WORKDIR app
# COPY --from=builder /app/target/release/app /usr/local/bin
# ENTRYPOINT ["/usr/local/bin/app"]

#
# # Copy our sources
# COPY . /app/
#
# # A bit of magic here!
# # * We're mounting that cache again to use during the build, otherwise it's not present and we'll have to download those again - bad!
# # * EOF syntax is neat but not without its drawbacks. We need to `set -e`, otherwise a failing command is going to continue on
# # * Rust here is a bit fiddly, so we'll touch the files (even though we copied over them) to force a new build
# RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/root/.rustup cd /app/ && cargo build --release
#
# CMD ["/app/target/release/my-app"]
#
# # Again, our final image is the same - a slim base and just our app
# FROM debian:buster-slim AS app
# COPY --from=build /app/target/release/my-app /my-app
# CMD ["/my-app"]
#
#
#
# FROM rust
# COPY ./stream_node /bin/stream_node
# COPY ./private_chain_setup.sh /bin/private_chain_setup.sh
# WORKDIR /bin/
# RUN chmod +x private_chain_setup.sh
# EXPOSE 5555
# ENTRYPOINT ["/bin/private_chain_setup.sh"]
