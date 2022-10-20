#!/bin/sh

prefix=/media/D/f/my/rust/substrate-node-template/target/release/build/tikv-jemalloc-sys-a997642a97ee0ce3/out
exec_prefix=/media/D/f/my/rust/substrate-node-template/target/release/build/tikv-jemalloc-sys-a997642a97ee0ce3/out
libdir=${exec_prefix}/lib

LD_PRELOAD=${libdir}/libjemalloc.so.2
export LD_PRELOAD
exec "$@"
