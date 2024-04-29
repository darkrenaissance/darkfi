darkfid localnet
================

This will start one `darkfid` node in localnet mode,
along with a `minerd` daemon to mine blocks.

If we want to test wallet stuff, we must generate
a testing wallet and pass its address to the `darkfid`
config, so the wallet gets the block rewards the node
produces. We generate a wallet, set it as the default
and grab its address:
```
% ../../../drk -c drk.toml wallet --initialize
% ../../../drk -c drk.toml wallet --keygen
% ../../../drk -c drk.toml wallet --default-address 1
% ../../../drk -c drk.toml wallet --address
```

Then we replace the `recipient` field in `darkfid.toml`
config with the output of the last command, start
`darkfid` and wait until its initialized:
```
% ./tmux_sessions.sh
```

After some blocks have been generated we
will see some `DRK` in our test wallet.
```
% ../../../drk -c drk.toml scan
% ../../../drk -c drk.toml wallet --balance
```

See the user guide in the book for more info.

## Wallet testing

Here is a table of all the `drk` stuff that needs to be tested to verify
wallet and node functionalities work as expected, based on the current
testnet user guide.
Note: List is not exhaustive. Missing functionalities that are not part
of the guide can be added for future regressions.

| #  | Description               | Command                                          | Status                           |
|    |---------------------------|--------------------------------------------------|----------------------------------|
| 0  | Initialization            | wallet --initialize                              | Pass                             |
| 1  | Key generation            | wallet --keygen                                  | Pass                             |
| 2  | Set default wallet        | wallet --default-address {ADDR_ID}               | Pass                             |
| 3  | Default address retrieval | wallet --address                                 | Pass                             |
| 4  | Block scanning            | scan                                             | Pass                             |
| 5  | Block subscribing         | subscribe                                        | Pass                             |
| 6  | Balance retrieval         | wallet --balance                                 | Pass                             |
| 7  | Aliases retrieval         | alias show                                       | Pass                             |
| 8  | Mint auth generation      | token generate-mint                              | Pass                             |
| 9  | Mint auths retrieval      | token list                                       | Pass                             |
| 10 | Alias add                 | alias add {ALIAS} {TOKEN}                        | Pass                             |
| 11 | Aliases retrieval         | alias show                                       | Pass                             |
| 12 | Mint generation           | token mint {ALIAS} {AMOUNT} {ADDR}               | Failure: disabled                |
| 13 | Transfer                  | transfer {AMOUNT} {ALIAS} {ADDR}                 | Failure: fee is missing          |
| 14 | Coins retrieval           | wallet --coins                                   | Pass                             |
| 15 | OTC initialization        | otc init -v {AMOUNT}:{AMOUNT} -t {ALIAS}:{ALIAS} | Failure: needs #12               |
| 16 | OTC join                  | otc join                                         | Failure: needs #15               |
| 17 | OTC sign                  | otc sign                                         | Failure: needs #16               |
| 18 | DAO create                | dao create {LIMIT} {QUORUM} {RATIO} {TOKEN}      | Failure: needs #12               |
| 19 | DAO view                  | dao view                                         | Failure: needs #18               |
| 20 | DAO import                | dao import                                       | Failure: needs #18               |
| 21 | DAO list                  | dao sign                                         | Failure: needs #18               |
| 22 | DAO mint                  | dao mint {DAO}                                   | Failure: needs #18               |
| 23 | DAO balance               | dao balance {DAO}                                | Failure: needs #18               |
| 24 | DAO propose               | dao propose {DAO} {ADDR} {AMOUNT} {TOKEN}        | Failure: needs #18               |
| 25 | DAO proposals retrieval   | dao proposals {DAO}                              | Failure: needs #24               |
| 26 | DAO proposal retrieval    | dao proposal {DAO} {PROPOSAL_ID}                 | Failure: needs #24               |
| 27 | DAO vote                  | dao vote {DAO} {PROPOSAL_ID} {VOTE} {WEIGHT}     | Failure: needs #24               |
| 28 | DAO proposal execution    | dao exec {DAO} {PROPOSAL_ID}                     | Failure: needs #27               |
| 29 | Coins unspend             | unspend {COIN}                                   | Pass                             |
| 30 | Transaction inspect       | inspect                                          | Pass                             |
| 31 | Transaction simulate      | explorer simulate-tx                             | Pass                             |
| 31 | Transaction broadcast     | broadcast                                        | Pass                             |

