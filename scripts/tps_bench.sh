#!/bin/bash
function stop_processes {
  pkill -f tps_bench
}
trap stop_processes SIGINT
echo "Press Ctrl+C to quit."
./run-example.sh stop
./run-example.sh start
../samples/TpsBench/target/release/tps_bench http://127.0.0.1:5556 2 3200 &
../samples/TpsBench/target/release/tps_bench http://127.0.0.1:5557 2 3200 > /dev/null &
../samples/TpsBench/target/release/tps_bench http://127.0.0.1:5558 2 3200 > /dev/null &
../samples/TpsBench/target/release/tps_bench http://127.0.0.1:5559 2 3200 > /dev/null
