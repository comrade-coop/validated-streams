# Transactions-per-second benchmark

Work in progress.

## Running the benchmark

Prerequisites:
* [Docker](https://docs.docker.com/get-docker/) with [docker-compose](https://docs.docker.com/compose/install/) (Podman might work too with some adjustments to the scripts, but has not been tested)
* [jq](https://jqlang.github.io/jq/)

Quick run with Docker Compose:

```bash
# (in repo root)
scripts/build_volume.sh
scripts/generate_keys.sh docker /tmp/chainSpecRaw.json compose-vol > /tmp/docker_compose_tps_bench.yml
docker-compose -f /tmp/docker_compose_tps_bench.yml up
```

Explanation:

1. `scripts/build_volume.sh` creates a docker volume called `vol-tps-bench` and fills it with build artifacts from the Debian Bullseye -based `comradecoop/validated-streams-tps-bench-full` image. It also builds `comradecoop/validated-streams`.
2. `scripts/generate_keys.sh docker /tmp/chainSpecRaw.json compose-vol` uses the `comradecoop/validated-streams` image ("docker" argument) to generate and populate a chainspec, then produces a Docker Compose configuration that uses the `vol-tps-bench` volume to pass the binaries and chainspec to the Debian Bullseye official image.
    * Note that the image used for running and the one used for building _must_ have the same libc, or otherwise the binaries will simply fail to execute.
3. `docker-compose` finally just launches the benchmark locally. If not running in docker swarm (or similar) expect less-than-ideal performance as all the nodes compete for CPU time.


To run without Docker:

1. Run `scripts/generate_keys.sh`. E.g. with a release-build node in the usual target directory:
    ```bash
    scripts/generate_keys.sh ../../target/release/vstreams_node chainSpecRaw.json setup
    ```

    (to generate keys for a smaller network, pass the number of nodes as a 4-th parameter.)
2. The script will produce a number of lines that look like this (along with a json file, e.g. `chainSpecRaw.json`):
    ```bash
    scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND chainSpecRaw.json 1 "blood dragon stool habit peace token cube risk suffer one keep clever" 6058e741333ba81580dfd7b56b4df742c3e595942202d648918831b1e3eb6fe3

    scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND chainSpecRaw.json 2 "reward kingdom thing window globe aware impact athlete fantasy heart toy merit" /ip4/$FIRST_MACHINE/tcp/30333/p2p/12D3KooWD5yV3pdniD2ucnFFTrHRbxFWCiexLwgQTxymbB3gkLqb
    ```

    Each one of those lines is a command-line invocation for a different node of the benchmark.
3. On each node, run the corresponding command lines, replacing:
    `$NODE_COMMAND` with the path to the `target/XX/vstreams_node` binary.
    `$CLIENT_COMMAND` with the path to the `samples/tps-benchmark/target/XX/vstreams_tps_benchmark` binary.
    `$FIRST_MACHINE` with the ip(v4) address of the first node of the list.

Note: When running, make sure all the machines' clocks are roughly in sync (not more than a few seconds off), and start executing the code around the same time. Otherwise, you risk some of the nodes getting slashed and the benchmark not testing the whole network.
