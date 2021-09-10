# version 0

## misc

- [x] random ID param for jsonrpc requests (bin/drk.rs)
- [x] merge cashier branch
- [x] update cashierd.rs to new config handling. note: password param in toml
- [ ] sqlcipher: better document install process or otherwise remove friction of using bundled version
- [X] remove default config from binaries and add to the readme
- [X] delete zkvm
- [X] SOL bridge poc
- [ ] Optional Cargo "features" for cashierd/darkfid, to {en,dis}able different chains

## deposit

- [x] drk: key generation
- [x] drk: deposit cli option
- [ ] darkfid: check addresses are valid
- [x] darkfid: send drk public key to cashierd.rs over tcp, triggered by drk.rs
- [x] cashierd: receive BTC, mint dBTC (see tx.rs)
- [x] cashierd: push tx to rocksdb (type: deposit, signed by cashier key)
- [x] cashierd: watch address for deposit
- [ ] cashierd: resume watch after restart
- [x] darkfid: poll gateway for new tx
- [x] darkfid: for every new coin received, add to merkle tree
- [x] darkfid: decode tx
- [x] darkfid: perform state transition function
- [x] darkfid: compute merklepath need to spend coin (see tx.rs)

## withdraw

- [x] drk: withdraw cli option
- [x] darkfid: check address is valid
- [x] darkfid: send cashout request to cashier with btc pub key
- [x] cashierd: receive cashout request, reply with drk pub key
- [x] darkfid: send dbtc to the cashier drk pub key
- [ ] cashierd: burn dbtc, send back btc (see tx.rs)

## drk -> drk

- [x] drk: transfer cli option
- [x] darkfid: construct a new spend tx
- [x] darkfid: check address is valid
- [x] darkfid: build tx
- [x] darkfid: verify tx
- [x] darkfid: state transition function
- [x] darkfid: send tx data to rocksdb

# blockchain

- [ ]

# halo2

- [x] mint circuit poc
- [ ] burn circuit poc
- [x] research port from jubjub to pasta (success)
- [x] research port from blake2b to sinsemilla and/or poseidon
- [ ] solve poseidon gadget to hash >2 elements at a time
- [ ] integrate with actual codebase

# org

- [ ] clean up shared repo and migrate to wiki
