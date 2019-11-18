#!/usr/bin/env bash
trap cleanup INT

cleanup() {
	pm2 kill
	rm -rf /tmp/pmesh-*-node*
}

set -xe

cd "$(dirname "$0")"

polymesh_binary=../../target/release/polymesh

pool_limit=${POOL_LIMIT:=100000}

# Cleanup
cleanup

pm2 start environment.config.js --only pmesh-primary-node

sleep 2

pm2 start environment.config.js --only "pmesh-peer-node-1,pmesh-peer-node-2"