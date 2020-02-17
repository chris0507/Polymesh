# PercentageTransferManager
This smart contract is a transfer manager for limiting the percentage of token supply that a single address can hold.
  

## Pre-requsite
`cargo-contract` Install using below command
```
cargo install --git https://github.com/paritytech/cargo-contract cargo-contract --force
```

### Build example contract and generate the contracts metadata

To build a single example and generate the contracts Wasm file, navigate to the root of the example smart contract and run:

```
cargo contract build

```

To generate the contract metadata (a.k.a. the contract ABI), run the following command:

```
cargo contract generate-metadata

```

You should now have an optimized  `<contract-name>.wasm`  file and an  `metadata.json`  file in the  `target`  folder of the contract.

For further information, please have a look at our  [smart contracts workshop](https://substrate.dev/substrate-contracts-workshop/).