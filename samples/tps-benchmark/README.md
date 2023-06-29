# Transactions-per-second benchmark

This is a benchmark designed to test the throughput performance of a Validated Streams network.

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

<details><summary>To run without Docker:</summary>

1. Run `scripts/generate_keys.sh`. E.g. with a release-build node in the usual target directory:
    ```bash
    scripts/generate_keys.sh ../../target/release/vstreams-node chainSpecRaw.json setup
    ```

    (to generate keys for a smaller network, pass the number of nodes as a 4-th parameter.)
2. The script will produce a number of lines that look like this (along with a json file, e.g. `chainSpecRaw.json`):
    ```bash
    scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND chainSpecRaw.json 1 "blood dragon stool habit peace token cube risk suffer one keep clever" 6058e741333ba81580dfd7b56b4df742c3e595942202d648918831b1e3eb6fe3

    scripts/tps_bench_setup.sh $NODE_COMMAND $CLIENT_COMMAND chainSpecRaw.json 2 "reward kingdom thing window globe aware impact athlete fantasy heart toy merit" /ip4/$FIRST_MACHINE/tcp/30333/p2p/12D3KooWD5yV3pdniD2ucnFFTrHRbxFWCiexLwgQTxymbB3gkLqb
    ```

    Each one of those lines is a command-line invocation for a different node of the benchmark.
3. On each node, run the corresponding command lines, replacing:
    `$NODE_COMMAND` with the path to the `target/XX/vstreams-node` binary.
    `$CLIENT_COMMAND` with the path to the `samples/tps-benchmark/target/XX/vstreams-tps-benchmark` binary.
    `$FIRST_MACHINE` with the ip(v4) address of the first node of the list.

Note: When running, make sure all the machines' clocks are roughly in sync (not more than a few seconds off), and start executing the code around the same time. Otherwise, you risk some of the nodes getting slashed and the benchmark not testing the whole network.

Note: You might be able to use the `compose-vol-remote` output mode of `generate_keys.sh` to run the benchmark on a set of remote nodes while still using docker-compose for orchestrating and firing up everything.

</details>

## Our results

To test the performance of Validated Streams, we have developed a benchmark and a set of deployment scripts, to test the sustained transactions-per-second (throughput) behavior of our network. Initially, we estimated the benchmark network should easily be able to handle up to a thousand events validated per second -- a far cry from Visa's 20k+ average transactions per second, but still plenty to outpace traditional Proof-of-Work blockchains like Ethereum and Bitcoin (that achieve ~30 average TPS, [chart](https://blockchair.com/ethereum/charts/transactions-per-second)), and even some Proof-of-Stake blockchains like Tendermint (~600 sustained TPS, [paper](https://www.inf.usi.ch/faculty/pedone/Paper/2021/srds2021a.pdf)).

The benchmark was designed to send more and more events to the network, increasing (currently, exponentially) the count of events sent with every successive block until it the network starts lagging behind, at which point it would start decreasing (also exponentially) the amount of events sent, in a control loop aiming to make the amount of events sent match (or oscillate endlessly around) the amount of events processed. The benchmark keeps sending events until some limit is reached, waits for the network to process the backlog, and exits. Afterward, the logs from the benchmark execution were analyzed by hand to collect the metrics collected by the script as well as any additional metrics that could be gather from that.

### Run 1: 2023-06-24

The benchmark code (as of commit ad2ec44704fcac0439a4868ec52f095694abcfc2) was started on bare-metal spot instances. It was left running for about  21 minutes total, after which the logs were aggregated in a Grafana cluster and analyzed.

All the bare-metal nodes used for the benchmark were rented from Equinix, using their c3.small.x86 instance class. Hence, each node was equipped with:
* Intel Xeon E-2278G CPU (8 cores @ 3.40 GHz)
* 32 GB RAM
* No permanent storage
* 2 x 480 GB SSD for booting up
* 2 x 10 Gbps NICs

The benchmark was configured using the `compose-vol-remote` configuration and the `tps_bench_setup_remote.sh` script. That means the machine used for setup node transferred the necessary files for running the benchmark over `scp`, then `ssh`-ed and started the regular `tps_bench_setup.sh` script on each node.

For the first run, we ran the benchmark with the following parameters:

```
Nodes: 32
Total events sent: 10000, Increase factor: 2, Decrease factor: 2
```

#### Collected data

<details> <summary> Events and blocks data: </summary>

| Block # | Time delta (s) <!-- From trusted client log entries of node 7 (randomly chosen) --> | Events count |
| --- | --- | --- |
| 1-16 | N/A | 0 |
| 17 | 5.83 | 0 |
| 18 | 5.86 | 3 |
| 19 | 5.82 | 4 |
| 20 | 7.34 | 8 |
| 21 | 4.37 | 16 |
| 22 | 7.26 | 32 |
| 23 | 6.22 | 64 |
| 24 | 6.03 | 128 |
| 25 | 5.82 | 256 |
| 26 | 5.99 | 512 |
| 27 | 12.22 | 0 |
| 28 | 5.94 | 0 |
| 29 | 12.05 | 1024 |
| 30 | 5.99 | 0 |
| 31 | 19.66 | 2048 |
| 32 R | 39.45 | 0 |
| 33 R | 0.00 | 2109 |
| 34 R | 0.00 | 1374 |
| 35 | 0.00 | 2423 |

R: Blocks were part of a reorg.

Overall, the blocks during which the results were benchmarked were produced over the course of 179 seconds / 3 minutes.

</details>

<details> <summary> Nodes: </summary>

| Node # | Region | Comments |
| --- | --- | --- |
| 1 | Amsterdam | |
| 2 | Amsterdam | |
| 3 X | Dallas | "Unable to pin block for import notification" error for block #33. Failed to finalize past block #32.|
| 4 | Dallas | |
| 5 | Dallas | |
| 6 | Frankfurt | |
| 7 | Frankfurt | |
| 8 | Helsinki | |
| 9 | Hong Kong | |
| 10 | Melbourne | |
| 11 | Montreal | |
| 12 X | New York | "Unable to pin block for import notification" block error for block #33. Failed to finalize past block #32. |
| 13 | New York | |
| 14 | New York | |
| 15 | Paris | |
| 16 | Sao Paulo | |
| 17 | Silicon Valley | |
| 18 | Silicon Valley | |
| 19 | Silicon Valley | |
| 20 | Singapore | |
| 21 | Singapore | |
| 22 | Toronto | |
| 23 | Washington, DC | |
| 24 | Washington, DC | "Unable to pin block for import notification" block error for block #33. |
| 25 | Washington, DC | |
| 26 | Sydney | |
| 27 | Sydney | |
| 28 | Sydney | |
| 29 | Frankfurt | |
| 30 | Frankfurt | |
| 31 | Frankfurt | |
| 32 | Frankfurt | |

X: Node failed to sync during the benchmark.

Note: all the nodes reported connection/serialization errors (UnexpectedEof) between blocks #17-#20.

We did not measure the latency/throughput/packet drop rates of the connections between the various nodes, but we assume they are similar to would-be production deployments of Validated Streams, given the cloud provider's connectedness to global network hubs.

</details>

Collected metrics:

| Metric | Description | Median value measured by nodes <!-- From non-faulty nodes' trusted clients' logs --> |
| --- | --- | --- |
| Peak TPS | Max momentary events per second achieved during a single block | 364.32 s<sup>-1</sup> |
| Average TPS | Average events per second, total events over total time | 69.77 s<sup>-1</sup> |
| Sustained TPS | Max events per second the network can sustain without backlog | Not measured |

<details> <summary> Average TPS reported at each block: </summary>

![Average TPS reported at each block](https://user-images.githubusercontent.com/5276727/249825387-18205eb6-b652-4bf4-87c0-802d7357a6a0.png)

</details>

#### Analysis

All the nodes experienced connection/serialization errors somewhere around the first few blocks. It is likely those are a result of Substrate's Swarm discovering Validated Streams's Swarm's ports (or vice versa) and proceeding to connect to them, only for the two swarms to find they are incompatible once Validated Streams starts producing messages.

Due to the reorg around blocks #32-#34, two of the nodes failed to re-sync with the network. Considering that they both got stuck with low peer counts, it's likely that the other peers deprioritized communications with those nodes. Upon further investigation of the logs, the actual reason for the failure to re-sync appears to be that the newly-proposed #33 contained events that the failing nodes had not received the gossip for, which ended with that block being unpinned and pruned by the time they tried to import block #34.
Most other nodes did not end up unpinning the block at that time, and thus managed to survive despite the couple of imports failing due to unwitnessed events. Only one other node had the same "unable to pin block" error, but it appears to have somehow raced the pruning operation.

Observing the events processed per block, we gladly note that the control loop managed to gently increase the value before reaching the limit. However, the way it did that was quite inefficient -- half of the active runtime of the benchmark, it was not even close to the peak TPS. A possible improvement would be to make the control loop jump to a target value faster, while still adapting to maintaining a sustainable TPS. In addition, before starting, it waited about as many blocks it needed to get to the final value, which is another thing that could be optimized.

The data collected in this run is an insufficient sample to make general conclusions about the performance characteristics of a Validated Streams network, however, it is barely sufficient to observe that the sustained TPS would be around 2000 events per block, which resolves to ~250 TPS. We arrive at that value by observing that between blocks 33 (2109) and 34 (1374), the decrease mechanism of the control loop triggered, causing the event count to fall down -- hence, we guess that the ideal sustained TPS would fall between those two values (the reorg at those same blocks suggests this guess might be wrong). Observing that the following block has an even higher event count (2423), we guess that the sustained TPS is near the higher end of the range, hence somewhere between 2000 and 2100. Dividing by the average block time in the benchmark (~8 s), we get ~250-260 TPS.


### Conclusion

We ran one run of our TPS benchmark. During it we measured an average of ~70 TPS, peaking at a high of 364 TPS for a few individual blocks. Based on the data collected, we estimate that the network can sustain a rate of ~250 TPS over long stretches of time, but additional runs would be needed to confirm that estimate.

In the future, we want to run the benchmark a few additional times to collect more data. But first, we could refine the control loop we uses to gradually increase the amounts of events sent to stress the network. In addition, we would like to fix the failure of a few nodes to resynchronize. Finally, a more throughout investigation of the performance characteristics of Validated Streams would include observing the effect of changing the number of nodes (and any other tweakable security parameters), as well as measuring the latency of event validation (though that is arguably not in the scope of this benchmark).
