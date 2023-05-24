# Validated Streams
Validated Streams is a consensus mechanism that enables a decentralized network of nodes to agree on and respond to events they observe in the world around them. It empowers developers to create on-chain applications that reactively source data from off-chain applications, while requiring confirmation of the occurrence of off-chain events from at least two-thirds of validators.

Validated Streams also acts as a fundamental building block of [Apocryph](https://apocryph.network/), where it enables different instances of a [Perper Application](https://github.com/obecto/perper) to agree on a stream of inputs and synchronize their state through event sourcing. The goal is to develop proactive blockchain applications with richer interactions with the off-chain world, while maintaining trustlessness and decentralization.

## Examples

Prerequisites:
- [Docker](https://docs.docker.com/get-docker/) with [docker-compose](https://docs.docker.com/compose/install/) ([Podman](https://github.com/containers/podman) with [podman-compose](https://github.com/containers/podman-compose) should work too)
- [grpcurl](https://github.com/fullstorydev/grpcurl)

* Witnessing events:

    We have set up a demonstration of a private chain comprised of four nodes (hence, the minimum number of nodes required to witness an event is 3 nodes) and a client that sends random events to them.
    - Build the docker image of a validated streams node (this might take a while the first time)
        ```bash
        docker build -t comradecoop/validated-streams .
        ```
    - Start the example network and client:
        ```bash
        ./scripts/run-example.sh start
        ```
        (pass --podman to use podman-compose)
    - To stop the network:
        ```bash
        ./scripts/run-example.sh stop
        ```
* Network Partition example:

    To simulate a network partition, this command will isolate validator 4 from the network therefore it will unwitness some events. after a while, it will reconnect it to the network. The validator will initially reject the blocks (since it does not have enough proofs for the events within) and then catch up with the rest of the network.

    ```
    ./scripts/run-example.sh partition
    ```
## Architecture
![Diagram of Validated Streams, with Stream service ingesting events from an application, passing them to a gossip, which then leads to on-chain transactions, that, after block finalization, get forwarded back to the application. (validated-streams.drawio.png)](https://user-images.githubusercontent.com/5276727/211316562-ad73fdd0-0dec-4543-884e-fe60cb09ee7a.png)

Each of validator is a Substrate node that has an attached trusted client(s). The client submits hashes representing events that have been witnessed locally. Since a malicious client would be able to fabricate or censor data at whim, it is necessary that the operators of validators don't trust other validators (or third parties in general) with the task of running trusted clients, but run their own, perhaps even collocating it with the validator node.

Upon receiving an event hash, the validator gossips the hash, signed, to other validators. This step ensures that the chain is not swamped or stalled with blocks containing unverified events, particularly when trusted clients are just beginning to witness an event. The event hash is submitted as a Substrate extrinsic only after it has been witnessed by 2/3 of the validators. Once the event is finalized through any of the usual on-chain mechanisms such as GRANDPA, it is considered validated by the Validated Streams chain.

To avoid discrepancies between on-chain and off-chain states, the finalized event hashes are sent back to the trusted clients. Depending on the use case, this information can be used to adapt the trusted client's own state to the on-chain proceedings, witness a correction to the finalized events, or report the discrepancy to the trusted client's users/operators.

The communication of hashes between the trusted client and validator node occurs over a gRPC protocol, allowing clients to be written with a wide variety of programming languages and software development frameworks.

It should be noted that the trusted client only submits hashes, and a separate solution (such as IPFS) would be required to retrieve the actual event contents.

> __Note__
It is important to note that Validated Streams will only work in chains where the total number/weight of validators is known, such as proof-of-stake or private/consortium chains. Further Research may be able to lift this limitation in the future.

## On-chain proofs

Storing the event proofs on-chain can be advantageous in some situations. Therefore, we provide the `on-chain-proofs` feature for users who prefer it. in order to compile the project using on-chain proofs run the following command:

```
cargo build --release --features on-chain-proofs
```

## Testing
To run the tests, use the following commands in the root directory of the project:

#### validated-streams crate:
```
cargo test -p vstreams
```
#### Pallet:
* Default:

    ```
    cargo test -p pallet-validated-streams
    ```
* On-chain Proofs:
    ```
    cargo test -p pallet-validated-streams --features on-chain-proofs
    ```
## Benchmarking

* default
    ```
    cargo build --release --features runtime-benchmarks
    ```
* On-chain proofs:

    * add `on-chain-proofs` feature as a dependency to runtime-benchmakrs feature in `pallet/Cargo.toml`:
        ```
        runtime-benchmarks = ["frame-benchmarking/runtime-benchmarks","on-chain-proofs"]
        ```

    * re-run the build command:
        ```
        cargo build --release --features runtime-benchmarks
        ```
