darkfid localnet
================

This will start one `darkfid` node in localnet mode,
along with a `minerd` daemon to mine blocks.

If we want to test wallet stuff, we must generate
a testing wallet and pass its address to the `darkfid`
config, so the wallet gets the block rewards the node
produces. We generate a wallet, set it as the default
and set its address as the `recipient` field in
`darkfid.toml`, using the porvided automated script:
```
% ./init-wallet.sh
```

Then start `darkfid` and wait until its initialized:
```
% ./tmux_sessions.sh
```

After some blocks have been generated we
will see some `DRK` in our test wallet.
```
% ./wallet-balance.sh
```

See the user guide in the book for more info.

## Wallet testing

Here is a table of all the `drk` stuff that needs to be tested to verify
wallet and node functionalities work as expected, based on the current
testnet user guide.
Note: List is not exhaustive. Missing functionalities that are not part
of the guide can be added for future regressions.

| #  | Description                  | Command                                                         | Status |
|----|------------------------------|-----------------------------------------------------------------|--------|
| 0  | Initialization               | wallet --initialize                                             | Pass   |
| 1  | Key generation               | wallet --keygen                                                 | Pass   |
| 2  | Set default wallet           | wallet --default-address {ADDR_ID}                              | Pass   |
| 3  | Default address retrieval    | wallet --address                                                | Pass   |
| 4  | Block scanning               | scan                                                            | Pass   |
| 5  | Block subscribing            | subscribe                                                       | Pass   |
| 6  | Balance retrieval            | wallet --balance                                                | Pass   |
| 7  | Aliases retrieval            | alias show                                                      | Pass   |
| 8  | Mint auth generation         | token generate-mint                                             | Pass   |
| 9  | Mint auths retrieval         | token list                                                      | Pass   |
| 10 | Alias add                    | alias add {ALIAS} {TOKEN}                                       | Pass   |
| 11 | Aliases retrieval            | alias show                                                      | Pass   |
| 12 | Mint generation              | token mint {ALIAS} {AMOUNT} {ADDR}                              | Pass   |
| 13 | Token freeze                 | token freeze {ALIAS}                                            | Pass   |
| 14 | Transfer                     | transfer {AMOUNT} {ALIAS} {ADDR}                                | Pass   |
| 15 | Coins retrieval              | wallet --coins                                                  | Pass   |
| 16 | OTC initialization           | otc init -v {AMOUNT}:{AMOUNT} -t {ALIAS}:{ALIAS}                | Pass   |
| 17 | OTC join                     | otc join                                                        | Pass   |
| 18 | OTC sign                     | otc sign                                                        | Pass   |
| 19 | DAO create                   | dao create {LIMIT} {QUORUM} {EARLY_EXEC_QUORUM} {RATIO} {TOKEN} | Pass   |
| 20 | DAO view                     | dao view                                                        | Pass   |
| 21 | DAO import                   | dao import                                                      | Pass   |
| 22 | DAO update keys              | dao update-keys                                                 | Pass   |
| 23 | DAO list                     | dao list                                                        | Pass   |
| 24 | DAO mint                     | dao mint {DAO}                                                  | Pass   |
| 25 | DAO balance                  | dao balance {DAO}                                               | Pass   |
| 26 | DAO proposals retrieval      | dao proposals {DAO}                                             | Pass   |
| 27 | DAO propose a transfer       | dao propose-transfer {DAO} {DUR} {AMOUNT} {TOKEN} {ADDR}        | Pass   |
| 28 | DAO propose generic          | dao propose-generic  {DAO} {DUR} {AMOUNT} {TOKEN} {ADDR}        | Pass   |
| 29 | DAO proposal retrieval       | dao proposal {PROPOSAL_BULLA}                                   | Pass   |
| 30 | DAO proposal export          | dao proposal {PROPOSAL_BULLA} --export                          | Pass   |
| 31 | DAO proposal import          | dao proposal-import                                             | Pass   |
| 32 | DAO proposal mint            | dao proposal {PROPOSAL_BULLA} --mint-proposal                   | Pass   |
| 33 | DAO vote                     | dao vote {PROPOSAL_BULLA} {VOTE} {WEIGHT}                       | Pass   |
| 34 | DAO proposal execution       | dao exec {PROPOSAL_BULLA}                                       | Pass   |
| 35 | DAO proposal early execution | dao exec --early {PROPOSAL_BULLA}                               | Pass   |
| 36 | Coins unspend                | unspend {COIN}                                                  | Pass   |
| 37 | Transaction inspect          | inspect                                                         | Pass   |
| 38 | Transaction simulate         | explorer simulate-tx                                            | Pass   |
| 39 | Transaction broadcast        | broadcast                                                       | Pass   |
| 40 | Transaction attach fee       | attach-fee                                                      | Pass   |

## Transactions fees

This table contains each executed transaction fee in `DRK`.

| Type        | Description                                             | Fee        |
|-------------|---------------------------------------------------------|------------|
| Transfer    | Native token transfer with single input and output      | 0.00525303 |
| Transfer    | Native token transfer with single input and two outputs | 0.00557027 |
| Transfer    | Native token transfer with two inputs and single output | 0.00570562 |
| Transfer    | Native token transfer with two inputs and outputs       | 0.00602301 |
| Token mint  | Custom token mint                                       | 0.00518391 |
| Transfer    | Custom token transfer with single input and two outputs | 0.00557027 |
| OTC swap    | Atomic swap between two custom tokens                   | 0.00601657 |
| DAO mint    | Mint a generated DAO onchain                            | 0.00474321 |
| Transfer    | Send tokens to a DAO treasury                           | 0.00602301 |
| DAO propose | Mint a generated DAO transfer proposal onchain          | 0.00574667 |
| DAO vote    | Vote for a minted DAO transfer proposal                 | 0.00601218 |
| DAO exec    | Execute (early) a passed DAO transfer proposal          | 0.00988316 |
| DAO propose | Mint a generated DAO generic proposal onchain           | 0.00574445 |
| DAO vote    | Vote for a minted DAO generic proposal                  | 0.00601218 |
| DAO exec    | Execute (early) a passed DAO generic proposal           | 0.00530605 |
| Token mint  | Custom token mint for a DAO treasury                    | 0.00518391 |
| DAO propose | Mint a generated DAO to DAO transfer proposal onchain   | 0.00574667 |
| DAO vote    | Vote for a minted DAO to DAO transfer proposal          | 0.00601218 |
| DAO exec    | Execute (early) a passed DAO to DAO transfer proposal   | 0.00988316 |
