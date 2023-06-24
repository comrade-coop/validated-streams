#!/bin/bash

# SPDX-License-Identifier: MIT

SSH_HOST=$1
SSH_KEY=$2

argsToForward=""
for item in "${@:3}"; do
    argsToForward+='"'"$item"'" '
done

ssh -o "StrictHostKeyChecking no" -i /home/root/.ssh/$SSH_KEY $SSH_HOST "pkill -f tps && rm -rf /mnt/*"
scp -r -o "StrictHostKeyChecking no" -i /home/root/.ssh/$SSH_KEY /mnt/* $SSH_HOST:/mnt
ssh -o "StrictHostKeyChecking no" -i /home/root/.ssh/$SSH_KEY $SSH_HOST "/mnt/tps_bench_setup.sh $argsToForward" &
wait