#!/bin/sh 
cd $(dirname $0)
subxt metadata -f bytes >  artifacts/metadata.scale
