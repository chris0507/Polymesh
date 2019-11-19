[![Gitter](https://img.shields.io/badge/chat-gitter-green.svg)](https://gitter.im/PolymathNetwork/Lobby)
[![Telegram](https://img.shields.io/badge/50k+-telegram-blue.svg)](https://gitter.im/PolymathNetwork/Lobby)

![Polymath logo](Polymath.png)

# Polymesh - The Polymath Blockchain

Polymesh is a blockchain for regulated securities and open finance.

# Build

Install Rust:

```bash
curl https://sh.rustup.rs -sSf | sh
```

Install required tools:

```bash
./scripts/init.sh
```

Build Wasm and native code:

```bash
cargo build --release
```

# Run

## Single node development chain

You can start a development chain with:

```bash
./target/release/polymesh --dev --pool-limit 100000 -d /tmp/pmesh-primary-node
```

Detailed logs may be shown by running the node with the following environment variables set:  
`RUST_LOG=debug RUST_BACKTRACE=1 ./target/release/polymesh --dev --pool-limit 100000 -d /tmp/pmesh-primary-node`.

## Multi-node local testnet

If you want to see the multi-node consensus algorithm in action locally, then you can create a local testnet with two validator nodes for Alice and Bob, who are the initial authorities of the genesis chain that have been endowed with testnet units.

```bash
cd scripts/cli
npm install
./run.sh
```

This uses pm2 to run a local three node network to demonstrate simple consensus.

# Initialise

You can seed the network with some identities, claims, signing keys and assets run.

```bash
cd scripts/cli
node ./index.js -n 2 -t 1 -d /tmp/pmesh-primary-node
```

See https://github.com/PolymathNetwork/Polymesh/tree/master/scripts/cli[README] for details.

# License

https://github.com/PolymathNetwork/Polymesh/blob/master/LICENSE[LICENSE]

# Substrate Framework

Polymesh is built on https://www.parity.io/what-is-substrate/[Substrate].

# Links    

- [Polymath Website](https://polymath.network)