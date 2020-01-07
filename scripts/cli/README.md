# Polymesh CLI

A small client-side Polymesh script to exercise major functionality in Polymesh.

Scripts to quickly run a local three node Polymesh testnet.

## Installation

```shell
$ yarn install #Project deps
```

## Usage

To run the three node local Polymesh testnet:

```shell
# Orchestrate the environment
$ ./run.sh 
# Viewing Substrate logs
$ ./node_modules/.bin/pm2 log pmesh-primary-node
$ ./node_modules/.bin/pm2 log pmesh-peer-node-1
$ ./node_modules/.bin/pm2 log pmesh-peer-node-2
```

To run the script and execute transactions:

```shell
$ node ./index.js -n 30 -t 5 -c 10 -p demo -d /tmp/pmesh-primary-node -f
```

The script can be run against any websocket endpoint, which can be modified by editing index.js.

Arguments are:

`-n 30` - specifies the number of master key accounts, issuer DIDs and tokens to create

`-t 5` - specifies the number of claim issuers to create

`-c 10` - specifies the number of claims that each issuer DID receives from a claim issuer

`-p demo` - specifies the name space for key derivation paths and DIDs. If you run the script multiple times you will need to vary this parameter to avoid namespace clashes on DIDs and token tickers

`-d /tmp/pmesh-primary-node` - specifies the directory to monitor for disk space growth during execution

`-f` - specifies that the script runs in "fast" mode - transactions are not monitored directly for completion meaning they can be submitted faster to flood transaction pool queues.

## Output

### Normal Run

```
$ node ./index.js -n 30 -t 5 -c 10 -p demo -d /tmp/pmesh-primary-node
Multiple versions of @polkadot/keyring detected, ensure that there is only one version in your dependency tree
Welcome to Polymesh Stats Collector. Creating 30 accounts and DIDs, with 10 claims per DID.
Unknown types found, no types for MaybeVrf
Initial storage size (/tmp/pmesh-primary-node): 13.828125MB
Generating Master Keys
Generating Signing Keys
Generating Claim Keys
=== Processing Transactions ===
████████████████████████████████████████ | Submit  : TPS                             | 30/30
████████████████████████████████████████ | Complete: TPS                             | 30/30
████████████████████████████████████████ | Submit  : DISTRIBUTE POLY                 | 65/65
████████████████████████████████████████ | Complete: DISTRIBUTE POLY                 | 65/65
████████████████████████████████████████ | Submit  : CREATE ISSUER IDENTITIES        | 30/30
████████████████████████████████████████ | Complete: CREATE ISSUER IDENTITIES        | 30/30
████████████████████████████████████████ | Submit  : ADD SIGNING KEYS                | 30/30
████████████████████████████████████████ | Complete: ADD SIGNING KEYS                | 30/30
████████████████████████████████████████ | Submit  : SET SIGNING KEY ROLES           | 30/30
████████████████████████████████████████ | Complete: SET SIGNING KEY ROLES           | 30/30
████████████████████████████████████████ | Submit  : ISSUE SECURITY TOKEN            | 30/30
████████████████████████████████████████ | Complete: ISSUE SECURITY TOKEN            | 30/30
████████████████████████████████████████ | Submit  : CREATE CLAIM ISSUER IDENTITIES  | 5/5
████████████████████████████████████████ | Complete: CREATE CLAIM ISSUER IDENTITIES  | 5/5
████████████████████████████████████████ | Submit  : ADD CLAIM ISSUERS               | 30/30
████████████████████████████████████████ | Complete: ADD CLAIM ISSUERS               | 30/30
████████████████████████████████████████ | Submit  : MAKE CLAIMS                     | 30/30
████████████████████████████████████████ | Complete: MAKE CLAIMS                     | 30/30
Total storage size delta: 8312KB
Total number of failures: 0
Transactions processed:
	Block Number: 2		Processed: 98	Time (ms): 5998
	Block Number: 3		Processed: 2	Time (ms): 6031
	Block Number: 4		Processed: 2	Time (ms): 5999
	Block Number: 5		Processed: 127	Time (ms): 5973
	Block Number: 6		Processed: 2	Time (ms): 6031
	Block Number: 7		Processed: 2	Time (ms): 5966
	Block Number: 8		Processed: 32	Time (ms): 6004
	Block Number: 9		Processed: 32	Time (ms): 5998
DONE
```

### Fast Run

NB - note failures in DID creation and token issuance as the "demo" namespace was reused

```
$ node ./index.js -n 30 -t 5 -c 10 -p demo -d /tmp/pmesh-primary-node -f
Multiple versions of @polkadot/keyring detected, ensure that there is only one version in your dependency tree
Welcome to Polymesh Stats Collector. Creating 30 accounts and DIDs, with 10 claims per DID.
Unknown types found, no types for MaybeVrf
Initial storage size (/tmp/pmesh-primary-node): 21.9453125MB
Generating Master Keys
Generating Signing Keys
Generating Claim Keys
=== Processing Transactions ===
████████████████████████████████████████ | Submit  : TPS                             | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: TPS                             | 0/30
████████████████████████████████████████ | Submit  : DISTRIBUTE POLY                 | 65/65
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: DISTRIBUTE POLY                 | 0/65
████████████████████████████████████████ | Submit  : CREATE ISSUER IDENTITIES        | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: CREATE ISSUER IDENTITIES        | 0/30
████████████████████████████████████████ | Submit  : ADD SIGNING KEYS                | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: ADD SIGNING KEYS                | 0/30
████████████████████████████████████████ | Submit  : SET SIGNING KEY ROLES           | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: SET SIGNING KEY ROLES           | 0/30
████████████████████████████████████████ | Submit  : ISSUE SECURITY TOKEN            | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: ISSUE SECURITY TOKEN            | 0/30
████████████████████████████████████████ | Submit  : CREATE CLAIM ISSUER IDENTITIES  | 5/5
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: CREATE CLAIM ISSUER IDENTITIES  | 0/5
████████████████████████████████████████ | Submit  : ADD CLAIM ISSUERS               | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: ADD CLAIM ISSUERS               | 0/30
████████████████████████████████████████ | Submit  : MAKE CLAIMS                     | 30/30
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ | Complete: MAKE CLAIMS                     | 0/30
Total storage size delta: 2048KB
Total number of failures: 0
Transactions processed:
	Block Number: 17		Processed: 97	Time (ms): 5987
	Block Number: 18		Processed: 127	Time (ms): 6004
	Block Number: 19		Processed: 32	Time (ms): 5997
	Block Number: 20		Processed: 2	Time (ms): 6004
	Block Number: 21		Processed: 32	Time (ms): 5995
DONE
```
