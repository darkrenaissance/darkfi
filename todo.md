# version 0

## misc

- [ ] sqlcipher: better document install process or otherwise remove friction of using bundled version

## v0-release

- [ ] change assetID variable to network and add switch statement in cashier::start()
- [ ] Optional Cargo "features" for cashierd/darkfid, to {en,dis}able different chains
- [ ] cashierd config has explicit mainnet and testnet configurations, simplify this and have a single endpoint
- [ ] drk -wk reports success on subsequent calls, it should rather tell that things are already initialized
- [ ] use f64 (and only positive/absolute) for amounts only on the client-facing side. internally, use u64 and num of decimals
- [ ] replace bin/drk, cashierd, darkfid with new binaries
- [ ] delete deprecated rpc code 
- [ ] drk2: check user input is valid tokenID and not symbol
- [ ] drk2: retrieve cashier features and error if don't support the network
- [ ] standardize error handling in drk2, cashierd, darkfid

## deposit

- [ ] cashierd: resume watch after restart

## bridge

- [ ] implement listen function

## withdraw

- [ ] cashierd: send the received coins to the main address of cashierd

## drk -> drk

- [ ] ...

# research

Open research questions.

## light-clients

- [ ] Fast efficient batch DH technique. Currently all new transactions need to be scanned. There should be a means of efficiently batching this test for light clients initially syncing against a server.
- [ ] Anonymous fetch using an Oblivious-Transfer protocol. Light clients potentially leak info to servers based on the data they request, but with an OT protocol they do not reveal exactly what they are requesting.

## cryptography

- [x] FFT for polynomial multiplication
- [x] finish bulletproofs impl
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
- [x] staking. Look up how TORN was distributed anonymously.
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
