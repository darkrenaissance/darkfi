# version 0 

## misc

[X] random ID param for jsonrpc requests (bin/drk.rs)
[X] merge cashier branch
[X] update cashierd.rs to new config handling. note: password param in toml
[ ] sqlcipher: document install process or otherwise remove friction of using bundled version

## deposit

[X] drk: key generation
[X] drk: deposit cli option
[ ] darkfid: check addresses are valid
[X] darkfid: send drk public key to cashierd.rs over tcp, triggered by drk.rs
[X] cashierd: recieve drk pub key, reply with btc pub key
[ ] cashierd: receive BTC, mint dBTC (see tx.rs)
[ ] cashierd: push tx to rocksdb (type: deposit, signed by cashier key)
[X] darkfid: poll gateway for new tx
[X] darkfid: for every new coin received, add to merkle tree
[X] darkfid: decode tx
[X] darkfid: perform state transition function
[X] darkfid: compute merklepath need to spend coin (see tx.rs)

## withdraw

[X] drk: withdraw cli option
[ ] darkfid: check address is valid
[X] darkfid: send cashout request to cashier with btc pub key
[ ] cashierd: receive cashout request, reply with drk pub key
[ ] darkfid: send dbtc to the cashier drk pub key 
[ ] cashierd: burn dbtc, send back btc (see tx.rs)

## drk -> drk

[X] drk: transfer cli option
[X] darkfid: construct a new spend tx
[ ] darkfid: check address is valid
[X] darkfid: build tx
[ ] darkfid: verify tx
[X] darkfid: state transition function
[X] darkfid: send tx data to rocksdb

# blockchain 

[ ] 

# halo2

[] mint circuit
[] burn circuit
[] research port from jubjub to pasta
[] research port from blake2b to sinsemilla and/or poseidon

# org

[ ] clean up shared repo and migrate to wiki
