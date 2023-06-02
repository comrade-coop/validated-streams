FROM rust:1-slim-bullseye as build
COPY . /client-app/
WORKDIR client-app
RUN apt-get update && apt-get install protobuf-compiler -y
RUN cargo build --release
FROM debian:buster-slim
COPY --from=build /client-app/target/release/tps_bench /bin/client
ENTRYPOINT ["./bin/client"]

