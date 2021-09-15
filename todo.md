# version 0

## misc

- [x] random ID param for jsonrpc requests (bin/drk.rs)
- [x] merge cashier branch
- [x] update cashierd.rs to new config handling. note: password param in toml
- [ ] sqlcipher: better document install process or otherwise remove friction of using bundled version
- [x] remove default config from binaries and add to the readme
- [x] delete zkvm
- [x] SOL bridge poc
- [ ] Optional Cargo "features" for cashierd/darkfid, to {en,dis}able different chains
- [x] delete ALL old directories and files
- [x] fix spelling mistakes
- [ ] make cashier asset vector 
- [ ] asset IDs as token addresses on the chosen network
- [ ] cashierd config has explicit mainnet and testnet configurations, simplify this and have a single endpoint
- [ ] drk -wk reports success on subsequent calls, it should rather tell that things are already initialized
- [ ] use f64 (and only positive/absolute) for amounts only on the client-facing side. internally, use u64 and num of decimals

## deposit

- [x] drk: key generation
- [x] drk: deposit cli option
- [x] darkfid: check addresses are valid
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
- [ ] cashierd: send the received coins to the main address of cashierd
- [ ] cashierd: burn dbtc, send back btc (see tx.rs)

## drk -> drk

- [x] drk: transfer cli option
- [x] darkfid: construct a new spend tx
- [x] darkfid: check address is valid
- [x] darkfid: build tx
- [x] darkfid: verify tx
- [x] darkfid: state transition function
- [x] darkfid: send tx data to rocksdb

# research

Open research questions.

## light-clients

- [ ] Fast efficient batch DH technique. Currently all new transactions need to be scanned. There should be a means of efficiently batching this test for light clients initially syncing against a server.
- [ ] Anonymous fetch using an Oblivious-Transfer protocol. Light clients potentially leak info to servers based on the data they request, but with an OT protocol they do not reveal exactly what they are requesting.

## cryptography

- [x] FFT for polynomial multiplication
- [ ] finish bulletproofs impl
- [ ] halo2 lookup
- [ ] read groth permutation paper

## blockchain

- [ ] basic sequencer architecture design
- [ ] basic DHT design
- [ ] consensus algorithm
- [ ] solve double verify problem (potentially need need a payment inside the contract to handle exceptions)
- [ ] research polygon design
- [ ] code up a simple demo

## product

- [ ] move DRK in and out of contracts from the root chain
- [ ] first MPC services
- [ ] DAO
- [ ] auctions
- [ ] staking. Look up how TORN was distributed anonymously.
- [ ] swaps
- [ ] token issuance
- [ ] NFTs

## dev

- [ ] make bitreich halo2 impl
- [ ] doc on circuit design

# halo2

- [x] mint circuit poc
- [ ] burn circuit poc
- [x] research port from jubjub to pasta (success)
- [x] research port from blake2b to sinsemilla and/or poseidon
- [ ] solve poseidon gadget to hash >2 elements at a time
- [ ] integrate with actual codebase

# org

- [ ] clean up shared repo and migrate to wiki
