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

| #  | Description               | Command                                                  | Status             |
|----|---------------------------|----------------------------------------------------------|--------------------|
| 0  | Initialization            | wallet --initialize                                      | Pass               |
| 1  | Key generation            | wallet --keygen                                          | Pass               |
| 2  | Set default wallet        | wallet --default-address {ADDR_ID}                       | Pass               |
| 3  | Default address retrieval | wallet --address                                         | Pass               |
| 4  | Block scanning            | scan                                                     | Pass               |
| 5  | Block subscribing         | subscribe                                                | Pass               |
| 6  | Balance retrieval         | wallet --balance                                         | Pass               |
| 7  | Aliases retrieval         | alias show                                               | Pass               |
| 8  | Mint auth generation      | token generate-mint                                      | Pass               |
| 9  | Mint auths retrieval      | token list                                               | Pass               |
| 10 | Alias add                 | alias add {ALIAS} {TOKEN}                                | Pass               |
| 11 | Aliases retrieval         | alias show                                               | Pass               |
| 12 | Mint generation           | token mint {ALIAS} {AMOUNT} {ADDR}                       | Pass               |
| 13 | Token freeze              | token freeze {ALIAS}                                     | Pass               |
| 14 | Transfer                  | transfer {AMOUNT} {ALIAS} {ADDR}                         | Pass               |
| 15 | Coins retrieval           | wallet --coins                                           | Pass               |
| 16 | OTC initialization        | otc init -v {AMOUNT}:{AMOUNT} -t {ALIAS}:{ALIAS}         | Pass               |
| 17 | OTC join                  | otc join                                                 | Pass               |
| 18 | OTC sign                  | otc sign                                                 | Pass               |
| 19 | DAO create                | dao create {LIMIT} {QUORUM} {RATIO} {TOKEN}              | Pass               |
| 20 | DAO view                  | dao view                                                 | Pass               |
| 21 | DAO import                | dao import                                               | Pass               |
| 22 | DAO list                  | dao list                                                 | Pass               |
| 23 | DAO mint                  | dao mint {DAO}                                           | Pass               |
| 24 | DAO balance               | dao balance {DAO}                                        | Pass               |
| 25 | DAO proposals retrieval   | dao proposals {DAO}                                      | Pass               |
| 26 | DAO propose a transfer    | dao propose-transfer {DAO} {DUR} {AMOUNT} {TOKEN} {ADDR} | Pass               |
| 27 | DAO proposals retrieval   | dao proposals {DAO}                                      | Pass               |
| 28 | DAO proposal retrieval    | dao proposal {PROPOSAL_BULLA}                            | Pass               |
| 29 | DAO proposal export       | dao proposal {PROPOSAL_BULLA} --export                   | Pass               |
| 30 | DAO proposal import       | dao proposal-import                                      | Pass               |
| 31 | DAO proposal mint         | dao proposal {PROPOSAL_BULLA} --mint-proposal            | Pass               |
| 32 | DAO vote                  | dao vote {PROPOSAL_BULLA} {VOTE} {WEIGHT}                | Pass               |
| 33 | DAO proposal execution    | dao exec {PROPOSAL_BULLA}                                | Failure: needs #32 |
| 34 | Coins unspend             | unspend {COIN}                                           | Pass               |
| 35 | Transaction inspect       | inspect                                                  | Pass               |
| 36 | Transaction simulate      | explorer simulate-tx                                     | Pass               |
| 37 | Transaction broadcast     | broadcast                                                | Pass               |
| 38 | Transaction attach fee    | attach-fee                                               | Pass               |

