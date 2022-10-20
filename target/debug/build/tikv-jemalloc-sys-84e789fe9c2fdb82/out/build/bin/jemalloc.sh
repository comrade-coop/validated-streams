#!/bin/sh

prefix=/media/D/f/my/rust/substrate-node-template/target/debug/build/tikv-jemalloc-sys-84e789fe9c2fdb82/out
exec_prefix=/media/D/f/my/rust/substrate-node-template/target/debug/build/tikv-jemalloc-sys-84e789fe9c2fdb82/out
libdir=${exec_prefix}/lib

LD_PRELOAD=${libdir}/libjemalloc.so.2
export LD_PRELOAD
exec "$@"
