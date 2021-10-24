# version 0

## misc

- [ ] git grep TODO

## v0-release

- [x] serialize signed btc transactions
- [x] add genesis btc coinbase addr as token id
- [x] serialize/encode btc keypairs
- [x] fix electrum rpc error: sendrawtransaction: TX decode failed
- [x] fix unsubscribe generating errors from electrum rpc
- [x] start hosting cashierd and gatewayd
- [x] add cashierd public key to darkfid.toml defaults

## post v0-release

- [ ] sollet btc / btc has same interface on drk
- [ ] add hdwallets to btc

## deposit

- [ ] ...

## bridge

- [ ] ...

## withdraw

- [ ] ...

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
- [ ] fflonk

## blockchain

- [ ] basic sequencer architecture design
- [ ] basic DHT design
- [ ] consensus algorithm
- [ ] solve double verify problem (potentially need need a payment inside the contract to handle exceptions)
- [ ] research polygon design
- [ ] code up a simple demo

## token

- [ ] simple amm script
- [ ] bonded curve script
- [ ] quadratic funding script
- [ ] write up DRK tokenomics
- [ ] simulate in CADCAD

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
- [x] jsonrpc interface for ETH bridge (interface with geth)

# halo2

- [x] mint circuit poc
- [ ] burn circuit poc
- [x] research port from jubjub to pasta (success)
- [x] research port from blake2b to sinsemilla and/or poseidon
- [ ] solve poseidon gadget to hash >2 elements at a time
- [ ] integrate with actual codebase

# org

- [ ] clean up shared repo and migrate to wiki
