# Validated Streams basic sample

Prerequisites:

* [Docker](https://docs.docker.com/get-docker/) with [docker-compose](https://docs.docker.com/compose/install/) ([Podman](https://github.com/containers/podman) with [podman-compose](https://github.com/containers/podman-compose) should work too)
* [grpcurl](https://github.com/fullstorydev/grpcurl)
* [pumba](https://github.com/alexei-led/pumba/releases) for the network resilience example
    * When downloading the `pumba` binary from [releases](https://github.com/alexei-led/pumba/releases), ensure that the "pumba" command is accessible in your system's PATH.

1. Witnessing events:

    We have set up a demonstration of a private chain comprised of four nodes (hence, the minimum number of nodes required to witness an event is 3 nodes) and a client that sends random events to them.
    Running the example:

    1. Build the docker image of a validated streams node (this might take a while the first time)
        ```bash
        docker build -t comradecoop/validated-streams .
        ```
    2. Start the example network:
        ```bash
        ./run-example.sh start
        ```
        (pass --podman to use podman-compose)
    3. Start an example trusted client witnessing a few thousand events to the example network:
        ```bash
        ./run-example.sh witness
        ```
    4. (in another shell) Listen for validated events:
        ```bash
        ./run-example.sh validated
        ```
    5. To stop the network:
        ```bash
        ./run-example.sh stop
        ```

    Alternatively, use the combined run command directly instead of steps 2-5:
    ```bash
    ./run-example.sh build
    ./run-example.sh run
    ```
2. Network resilience testing:

    The following example applies packet loss, frequent crash-recovery, and delayed packets to emulate challenging and poor network conditions. It tests the behavior and resilience of validators within the network under these adverse scenarios.

    ```
    ./run-example.sh disturb
    ```

## Architecture

In the basic example, events are witnessed directly from the test script. The script acts as a trusted client and connects to all nodes simultaneously. As such, this example doubles as a demonstration of the danger of exposing the trusted client endpoint to the public - it allows third parties to validate any event they might wish. That's why in a more realistic scenario (e.g. one of the other samples), each validator would run it's own trusted client.


