darkfid localnet
================

This will start one `darkfid` node in localnet mode,
along with an `xmrig` daemon to mine blocks.

If we want to test wallet stuff, we must generate a testing wallet and
pass its mining configuration to the `xmrig` daemon, so the wallet gets
the block rewards the node produces. We generate a wallet, set it as
the default and set its address as the `XMRIG_USER` field in
`tmux_sessions.sh`, using provided automated script:
```shell
% ./init-wallet.sh
```

Then make sure the `xmrig` daemon binary path is configured correctly
in `tmux_sessions.sh`, start the daemons and wait until `darkfid` is
initialized:
```shell
% ./tmux_sessions.sh
```

After some blocks have been generated we will see some `DRK` in our
test wallet:
```shell
% ./wallet-balance.sh
```

See the user guide in the book for more info.

## Wallet testing

Here is a table of all the `drk` stuff that needs to be tested to verify
wallet and node functionalities work as expected, based on the current
testnet user guide.

Notes:
1. List is not exhaustive. Missing functionalities that are not part of
the guide can be added for future regressions.
2. All commands were executed in `drk` interactive mode.

| #  | Description                  | Command                                                         | Status |
|----|------------------------------|-----------------------------------------------------------------|--------|
| 0  | Initialization               | wallet initialize                                               | Pass   |
| 1  | Key generation               | wallet keygen                                                   | Pass   |
| 2  | Set default wallet           | wallet default-address {ADDR_INDEX}                             | Pass   |
| 3  | Default address retrieval    | wallet address                                                  | Pass   |
| 4  | Block scanning               | scan                                                            | Pass   |
| 5  | Block subscribing            | subscribe                                                       | Pass   |
| 6  | Balance retrieval            | wallet balance                                                  | Pass   |
| 7  | Aliases retrieval            | alias show                                                      | Pass   |
| 8  | Mint auth generation         | token generate-mint                                             | Pass   |
| 9  | Mint auths retrieval         | token list                                                      | Pass   |
| 10 | Alias add                    | alias add {ALIAS} {TOKEN}                                       | Pass   |
| 11 | Aliases retrieval            | alias show                                                      | Pass   |
| 12 | Mint generation              | token mint {ALIAS} {AMOUNT} {ADDR}                              | Pass   |
| 13 | Token freeze                 | token freeze {ALIAS}                                            | Pass   |
| 14 | Transfer                     | transfer {AMOUNT} {ALIAS} {ADDR}                                | Pass   |
| 15 | Coins retrieval              | wallet coins                                                    | Pass   |
| 16 | OTC initialization           | otc init {AMOUNT}:{AMOUNT} {ALIAS}:{ALIAS}                      | Pass   |
| 17 | OTC join                     | otc join                                                        | Pass   |
| 18 | OTC sign                     | otc sign                                                        | Pass   |
| 19 | DAO create                   | dao create {LIMIT} {QUORUM} {EARLY_EXEC_QUORUM} {RATIO} {TOKEN} | Pass   |
| 20 | DAO view                     | dao view                                                        | Pass   |
| 21 | DAO import                   | dao import                                                      | Pass   |
| 22 | DAO list                     | dao list                                                        | Pass   |
| 23 | DAO mint                     | dao mint {DAO}                                                  | Pass   |
| 24 | DAO balance                  | dao balance {DAO}                                               | Pass   |
| 25 | DAO proposals retrieval      | dao proposals {DAO}                                             | Pass   |
| 26 | DAO propose a transfer       | dao propose-transfer {DAO} {DUR} {AMOUNT} {TOKEN} {ADDR}        | Pass   |
| 27 | DAO propose generic          | dao propose-generic  {DAO} {DUR} {AMOUNT} {TOKEN} {ADDR}        | Pass   |
| 28 | DAO proposal retrieval       | dao proposal {PROPOSAL_BULLA}                                   | Pass   |
| 29 | DAO proposal export          | dao proposal --export {PROPOSAL_BULLA}                          | Pass   |
| 30 | DAO proposal import          | dao proposal-import                                             | Pass   |
| 31 | DAO proposal mint            | dao proposal --mint-proposal {PROPOSAL_BULLA}                   | Pass   |
| 32 | DAO vote                     | dao vote {PROPOSAL_BULLA} {VOTE} {WEIGHT}                       | Pass   |
| 33 | DAO proposal execution       | dao exec {PROPOSAL_BULLA}                                       | Pass   |
| 34 | DAO proposal early execution | dao exec --early {PROPOSAL_BULLA}                               | Pass   |
| 35 | Contract auth generation     | contract generate-deploy                                        | Pass   |
| 36 | Contract auths retrieval     | contract list                                                   | Pass   |
| 37 | Contract deployment          | contract deploy {CONTRACT_ID} {WASM_PATH}                       | Pass   |
| 38 | Contract history retrieval   | contract list {CONTRACT_ID}                                     | Pass   |
| 39 | Contract transaction export  | contract export-data {TX_HASH}                                  | Pass   |
| 40 | Contract lock                | contract lock {CONTRACT_ID}                                     | Pass   |
| 41 | Coins unspend                | unspend {COIN}                                                  | Pass   |
| 42 | Transaction inspect          | inspect                                                         | Pass   |
| 43 | Transaction simulate         | explorer simulate-tx                                            | Pass   |
| 44 | Transaction broadcast        | broadcast                                                       | Pass   |
| 45 | Transaction attach fee       | attach-fee                                                      | Pass   |

## Transactions fees

This table contains each executed transaction fee in `DRK`.

| Type           | Description                                             | Fee        |
|----------------|---------------------------------------------------------|------------|
| Transfer       | Native token transfer with single input and output      | 0.00520957 |
| Transfer       | Native token transfer with single input and two outputs | 0.00551294 |
| Transfer       | Native token transfer with two inputs and single output | 0.00564359 |
| Transfer       | Native token transfer with two inputs and two outputs   | 0.00594684 |
| Token mint     | Custom token mint                                       | 0.00513825 |
| Token freeze   | Custom token freeze                                     | 0.00472310 |
| Transfer       | Custom token transfer with single input and two outputs | 0.00551294 |
| OTC swap       | Atomic swap between two custom tokens                   | 0.00594094 |
| DAO mint       | Mint a generated DAO onchain                            | 0.00472200 |
| Transfer       | Send tokens to a DAO treasury                           | 0.00551294 |
| DAO propose    | Mint a generated DAO transfer proposal onchain          | 0.00567447 |
| DAO vote       | Vote for a minted DAO transfer proposal                 | 0.00593839 |
| DAO exec       | Execute (early) a passed DAO transfer proposal          | 0.00962249 |
| DAO propose    | Mint a generated DAO generic proposal onchain           | 0.00567656 |
| DAO vote       | Vote for a minted DAO generic proposal                  | 0.00593839 |
| DAO exec       | Execute (early) a passed DAO generic proposal           | 0.00962249 |
| Token mint     | Custom token mint for a DAO treasury                    | 0.00513825 |
| DAO propose    | Mint a generated DAO to DAO transfer proposal onchain   | 0.00567652 |
| DAO vote       | Vote for a minted DAO to DAO transfer proposal          | 0.00593839 |
| DAO exec       | Execute (early) a passed DAO to DAO transfer proposal   | 0.00962249 |
| Contact deploy | Deploy a contract `WASM` bincode on-chain               | 0.00872699 |
| Contact lock   | Lock contract code on-chain                             | 0.00516328 |
