# Transactions-per-second benchmark

Work in progress.

## Running the benchmark

1. Run `./scripts/tps_bench_generate_keys.sh`. E.g. with a release-build node in the usual target directory:
    ```bash
    scripts/tps_bench_generate_keys.sh target/release/node chainSpecRaw.json
    ```

    (to generate keys for a smaller network, pass the number of nodes as a 4-th parameter.)
2. The script will produce a number of lines that look like this (along with a json file, e.g. `chainSpecRaw.json`):
    ```bash
    scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND chainSpecRaw.json 1 "blood dragon stool habit peace token cube risk suffer one keep clever" 6058e741333ba81580dfd7b56b4df742c3e595942202d648918831b1e3eb6fe3

    scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND chainSpecRaw.json 2 "reward kingdom thing window globe aware impact athlete fantasy heart toy merit" /ip4/$FIRST_MACHINE/tcp/30333/p2p/12D3KooWD5yV3pdniD2ucnFFTrHRbxFWCiexLwgQTxymbB3gkLqb
    ```

    Each one of those lines is a command-line invocation for a different node of the benchmark.
3. On each node, run the corresponding command lines, replacing:
    `$NODE_COMMAND` with the path to the `target/XX/node` binary.
    `$CLIENT_COMMAND` with the path to the `samples/TpsBench/target/XX/tps_bench` binary.
    `$FIRST_MACHINE` with the ip(v4) address of the first node of the list.

Optional: to use Docker / etc.:

1. Build `Dockerfile-combined` after building the two prerequisite Docker images:
    ```bash
    # (from the repo root)
    docker build -t comradecoop/validated-streams .
    docker build -t comradecoop/validated-streams-tps-bench . -f samples/TpsBench/Dockerfile
    docker build -t comradecoop/validated-streams-tps-bench-full - < samples/TpsBench/Dockerfile-combined
    ```
2. After running `scripts/tps_bench_generate_keys.sh`, remove each of the generated `scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND` and pass the rest of the command line to the container running the combined docker image. Make sure to also mount te `chainSpecRaw.json` file and adjust its path. E.g.:
    ```bash
    docker run --rm -v $(pwd):/pwd:ro comradecoop/validated-streams-tps-bench-full /pwd/chainSpecRaw.json 2 "reward kingdom thing window globe aware impact athlete fantasy heart toy merit" /ip4/$FIRST_MACHINE/tcp/30333/p2p
    ```

Note: When running, make sure all the machines' clocks are roughly in sync (not more than a few seconds off), and start the code roughly simultaineously. Otherwise, you risk some of the nodes getting slashed and the benchmark not testing the whole network.
