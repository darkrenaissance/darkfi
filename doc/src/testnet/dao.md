# DAO

On the testnet, we are also able to create an anonymous DAO. Using
the `drk` CLI tool, we have a `dao` subcommand that can perform the
necessary operations.

DarkFi DAOs have several configurable parameters, including:

- **Proposer_limit**: the minimum amount of governance tokens needed to
open a proposal.
- **Quorum**: The minimal threshold of participating total tokens
needed for a proposal to pass (expressed as an absolute value).
- **Early execution quorum**: The minimal threshold of participating
total tokens needed for a proposal to be considered as strongly
supported, enabling early execution. Must be greater or equal to normal
quorum.
- **Approval_ratio**: The ratio of winning/total votes needed for a
proposal to pass.
- **Governance token**: The DAO's governance token ID.

Let's create a DAO with the following parameters:

- **Proposer limit**: `20`
- **Quorum**: `10`
- **Early execution quorum**: `10`
- **Approval ratio**: `0.67`
- **Governance token**: `ANON`

You can use the tokens we created earlier to create new tokens. Return
to the definition of each parameter by running the `help` command in a
different terminal (not supported in interactive mode right now) like
this:

```shell
$ ./drk help dao create

drk-dao-create 0.4.1
Create DAO parameters

USAGE:
    drk dao create <proposer-limit> <quorum> <early-exec-quorum> <approval-ratio> <gov-token-id>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <proposer-limit>       The minimum amount of governance tokens needed to open a proposal for this DAO
    <quorum>               Minimal threshold of participating total tokens needed for a proposal to pass
    <early-exec-quorum>    Minimal threshold of participating total tokens needed for a proposal to be considered as
                           strongly supported, enabling early execution. Must be greater or equal to normal quorum
    <approval-ratio>       The ratio of winning votes/total votes needed for a proposal to pass (2 decimals)
    <gov-token-id>         DAO's governance token ID
```

Now let's create our DAO:

```shell
drk> dao create 20 10 10 0.67 ANON > anon_dao.toml
```

And view it:

```shell
drk> dao view < anon_dao.toml

DAO Parameters
==============
Proposer limit: 20 (2000000000)
Quorum: 10 (1000000000)
Early Exec Quorum: 10 (1000000000)
Approval ratio: 0.67
Governance Token ID: {TOKEN1}
Notes Public key: DiVGqk...SHy2zE
Notes Secret key: ARiqFg...Jg1xZB
Proposer Public key: F2BLix...9g6xRT
Proposer Secret key: DA7mgp...Ugk2qF
Proposals Public key: D1w2hG...7izmnS
Proposals Secret key: 8d8GG2...TPdGGx
Votes Public key: CVfKnc...kwKraQ
Votes Secret key: B5rGPz...7J5uJZ
Exec Public key: 3ZG5cK...GakTRS
Exec Secret key: HHVTM4...LhLYQQ
Early Exec Public key: 5xo4yj...gzCf3W
Early Exec Secret key: 9r9URX...TZCHPL
Bulla blind: 6TVkmM...Jjd5zC
```

Since its a normal `toml` file, you may open it with you favourite
editor, modify the keys configuration and/or maintain different config
versions for different DAO members. By default all keys are different,
so its up to the DAO founders to chose what configuration they are
going to use. After configuring the file(s) properly, it can be shared
among DAO members, so they hold the generated DAO information and keys.
The view command will show us the parameters. If everything looks fine,
we can now import it into our wallet:

```shell
drk> dao import AnonDAO < anon_dao.toml

Importing "AnonDAO" DAO into the wallet
```

```shell
drk> dao list

0. AnonDAO
```

```shell
drk> dao list AnonDAO

DAO Parameters
==============
Name: AnonDAO
Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposer limit: 20 (2000000000)
Quorum: 10 (1000000000)
Early Exec Quorum: 10 (1000000000)
Approval ratio: 0.67
Governance Token ID: {TOKEN1}
Notes Public key: DiVGqk...SHy2zE
Notes Secret key: ARiqFg...Jg1xZB
Proposer Public key: F2BLix...9g6xRT
Proposer Secret key: DA7mgp...Ugk2qF
Proposals Public key: D1w2hG...7izmnS
Proposals Secret key: 8d8GG2...TPdGGx
Votes Public key: CVfKnc...kwKraQ
Votes Secret key: B5rGPz...7J5uJZ
Exec Public key: 3ZG5cK...GakTRS
Exec Secret key: HHVTM4...LhLYQQ
Early Exec Public key: 5xo4yj...gzCf3W
Early Exec Secret key: 9r9URX...TZCHPL
Bulla blind: 6TVkmM...Jjd5zC
Leaf position: None
Transaction hash: None
Call index: None
```

## Minting

If parameters are shown, this means the DAO was successfully imported
into our wallet. We use the DAO name to reference it. Now we can create
a transaction that will mint the DAO on-chain, if we hold all its keys,
and broadcast it:

```shell
drk> dao mint AnonDAO | broadcast

[mark_tx_spend] Processing transaction: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
```

Now the transaction is broadcasted to the network. After confirmation
you should see a leaf position, a mint height and a transaction hash
when running:

```shell
drk> dao list AnonDAO

DAO Parameters
==============
Name: AnonDAO
Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposer limit: 20 (2000000000)
Quorum: 10 (1000000000)
Early Exec Quorum: 10 (1000000000)
Approval ratio: 0.67
Governance Token ID: {TOKEN1}
Notes Public key: DiVGqk...SHy2zE
Notes Secret key: ARiqFg...Jg1xZB
Proposer Public key: F2BLix...9g6xRT
Proposer Secret key: DA7mgp...Ugk2qF
Proposals Public key: D1w2hG...7izmnS
Proposals Secret key: 8d8GG2...TPdGGx
Votes Public key: CVfKnc...kwKraQ
Votes Secret key: B5rGPz...7J5uJZ
Exec Public key: 3ZG5cK...GakTRS
Exec Secret key: HHVTM4...LhLYQQ
Early Exec Public key: 5xo4yj...gzCf3W
Early Exec Secret key: 9r9URX...TZCHPL
Bulla blind: 6TVkmM...Jjd5zC
Leaf position: Position(0)
Mint height: 10
Transaction hash: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
Call index: 0
```

## Sending money to the treasury

Let's send some tokens to the DAO's treasury so we're able to make
a proposal to send those somewhere. First, find the DAO bulla, the
dao contract spend hook and the DAO notes public key.

Then create a transfer transaction as follows:

```shell
drk> dao spend-hook

6iW9nywZYvyhcM7P1iLwYkh92rvYtREDsC8hgqf2GLuT
```

```shell
drk> dao list AnonDAO

DAO Parameters
==============
Name: AnonDAO
Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposer limit: 20 (2000000000)
Quorum: 10 (1000000000)
Early Exec Quorum: 10 (1000000000)
Approval ratio: 0.67
Governance Token ID: {TOKEN1}
Notes Public key: DiVGqk...SHy2zE
Notes Secret key: ARiqFg...Jg1xZB
Proposer Public key: F2BLix...9g6xRT
Proposer Secret key: DA7mgp...Ugk2qF
Proposals Public key: D1w2hG...7izmnS
Proposals Secret key: 8d8GG2...TPdGGx
Votes Public key: CVfKnc...kwKraQ
Votes Secret key: B5rGPz...7J5uJZ
Exec Public key: 3ZG5cK...GakTRS
Exec Secret key: HHVTM4...LhLYQQ
Early Exec Public key: 5xo4yj...gzCf3W
Early Exec Secret key: 9r9URX...TZCHPL
Bulla blind: 6TVkmM...Jjd5zC
Leaf position: Position(0)
Mint height: 10
Transaction hash: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
Call index: 0
```

```shell
drk> transfer 10 DAWN {DAO_NOTES_PUBLIC_KEY} {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} | broadcast

[mark_tx_spend] Processing transaction: a4db439f75de88457cadd849131394ae37723c943ea5c088b218d6dc0f7982f1
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: a4db439f75de88457cadd849131394ae37723c943ea5c088b218d6dc0f7982f1
```

Wait for it to confirm. If you hold the DAO notes key, you can view the
balance like so:

```shell
drk> dao balance AnonDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN2} | DAWN    | 10
```

## Creating a proposal

Now that the DAO has something in its treasury, we can generate a
transfer proposal to send it somewhere, that will be up to vote
for 1 block period, if we hold the DAO proposer key. Let's propose
to send 5 of the 10 tokens to our address (we can find that with
`wallet address`):

```shell
drk> dao propose-transfer AnonDAO 1 5 DAWN {YOUR_ADDRESS}

Generated proposal: {PROPOSAL_BULLA}
```

After command was executed, it will output the generated proposal
bulla, which we will use to view the proposal full information:

```shell
drk> dao proposal {PROPOSAL_BULLA}

Proposal parameters
===================
Bulla: {PROPOSAL_BULLA}
DAO Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposal leaf position: None
Proposal mint height: None
Proposal transaction hash: None
Proposal call index: None
Creation block window: 28
Duration: 1 (Block windows)

Invoked contracts:
        Contract: Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj
        Function: 4
        Data:
                Recipient: {YOUR_ADDRESS}
                Amount: 500000000 (5)
                Token: {TOKEN2}
                Spend hook: -
                User data: -
                Blind: 8e9ne7...bVGsbH

        Contract: BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o
        Function: 3
        Data: -

Votes: No votes found
Voting status: Ongoing
Current proposal outcome: Unknown
```

We can export this proposal, to share with rest DAO members.
The exported file will be encrypted using the DAO proposals view key,
so only its members can decrypt and import it.

```shell
drk> dao proposal --export {PROPOSAL_BULLA} > anon_dao_transfer_proposal.dat
```

```shell
drk> dao proposal-import < anon_dao_transfer_proposal.dat
```

Now we can create the proposal mint transaction:
```shell
drk> dao proposal --mint-proposal {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: 2149d7e3a60be12c96b6c6fc7ba009717d8b229b815dd4006bbe120c31681f38
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 2149d7e3a60be12c96b6c6fc7ba009717d8b229b815dd4006bbe120c31681f38
```

Members that didn't receive the encrypted file will receive the
proposal when they scan the corresponding block, but its plaintext
data will be missing, so they should ask the DAO for it.
Once confirmed, you should see a leaf position, a mint height and a
transaction hash when running:

```shell
drk> dao proposal {PROPOSAL_BULLA}

Proposal parameters
===================
Bulla: G9FUrWn6PLieNuPpYdzUmfd1UP9tUVMpimmu7mwMukcU
DAO Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposal leaf position: Position(0)
Proposal mint height: 12
Proposal transaction hash: 2149d7e3a60be12c96b6c6fc7ba009717d8b229b815dd4006bbe120c31681f38
Proposal call index: 0
Creation block window: 28
Duration: 1 (Block windows)

Invoked contracts:
        Contract: Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj
        Function: 4
        Data:
                Recipient: {YOUR_ADDRESS}
                Amount: 500000000 (5)
                Token: {TOKEN2}
                Spend hook: -
                User data: -
                Blind: 8e9ne7...bVGsbH

        Contract: BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o
        Function: 3
        Data: -

Votes: No votes found
Voting status: Ongoing
Current proposal outcome: Unknown
```

## Voting on a proposal

Now the DAO members are ready to cast their votes.
First lets check the `dao vote` subcommand usage.

```shell
$ ./drk help dao vote

drk-dao-vote 0.5.0
Vote on a given proposal

USAGE:
    drk dao vote <bulla> <vote> [vote-weight]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <bulla>          Bulla identifier for the proposal
    <vote>           Vote (0 for NO, 1 for YES)
    <vote-weight>    Optional vote weight (amount of governance tokens)
```

Lets use our `ANON` governance tokens to vote yes to the proposal.

```shell
drk> dao vote {PROPOSAL_BULLA} 1 | broadcast

[mark_tx_spend] Processing transaction: 060468c5676a52a8b59b464dc959906b762a2108fa6f9d0db0b88c9d200eb612
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 060468c5676a52a8b59b464dc959906b762a2108fa6f9d0db0b88c9d200eb612
```

Once confirmed and scanned, you should see votes information and
current status, assuming you hold the votes view key, by running:

```shell
drk> dao proposal {PROPOSAL_BULLA}

Proposal parameters
===================
Bulla: G9FUrWn6PLieNuPpYdzUmfd1UP9tUVMpimmu7mwMukcU
DAO Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposal leaf position: Position(0)
Proposal mint height: 12
Proposal transaction hash: 2149d7e3a60be12c96b6c6fc7ba009717d8b229b815dd4006bbe120c31681f38
Proposal call index: 0
Creation block window: 28
Duration: 1 (Block windows)

Invoked contracts:
        Contract: Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj
        Function: 4
        Data:
                Recipient: {YOUR_ADDRESS}
                Amount: 500000000 (5)
                Token: {TOKEN2}
                Spend hook: -
                User data: -
                Blind: 8e9ne7...bVGsbH

        Contract: BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o
        Function: 3
        Data: -

Votes:
 Transaction                                                      | Tokens | Vote
------------------------------------------------------------------+--------+------
 060468c5676a52a8b59b464dc959906b762a2108fa6f9d0db0b88c9d200eb612 | 40     | Yes

Total tokens votes: 40
Total tokens Yes votes: 40 (100.00%)
Total tokens No votes: 0 (0.00%)
Voting status: Ongoing
Current proposal outcome: Approved
```

## Executing the proposal

Once the block period has passed(~4h) and enough votes have been cast
that meet the required minimum (quorum), and assuming the yes:no votes
ratio is bigger than the approval ratio, then we are ready to confirm
the vote. Only DAO members with the executor key can perform this
action.

```shell
drk> dao exec {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: 808b75685d91c766574dd5a3d46206b8e145b29f3647736161d2e2b2db051444
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 3
Broadcasting transaction...
Transaction ID: 808b75685d91c766574dd5a3d46206b8e145b29f3647736161d2e2b2db051444
```

Since in our tutorial the `ANON` governance tokens we used surpass the
early execution quorum, we can execute the proposal right away, if we
hold both the DAO executor and early executor keys:

```shell
drk> dao exec --early {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: 808b75685d91c766574dd5a3d46206b8e145b29f3647736161d2e2b2db051444
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 3
Broadcasting transaction...
Transaction ID: 808b75685d91c766574dd5a3d46206b8e145b29f3647736161d2e2b2db051444
```

After the proposal has been executed on chain, we will see that the DAO
balance has been reduced by 5 `DAWN`, if we hold the DAO notes key,
while our own balance has been increased by the same amount:

```shell
drk> dao balance AnonDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN2} | DAWN    | 5
```

```shell
drk> wallet balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+-------------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 19.93153568
 {TOKEN1}                                     | ANON    | 40
 {TOKEN2}                                     | DAWN    | 15
```

## Generic proposal

DAOs can vote on off-chain operations by creating what is known as
generic proposals, meaning that no on-chain action is tied to it:

```shell
drk> dao propose-generic AnonDAO 1

Generated proposal: {PROPOSAL_BULLA}
```

```shell
drk> dao proposal --mint-proposal {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: d90f4863445e2b45b4c710e668eed6cfee18b4b513f923fbfe327022f01d4f15
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: d90f4863445e2b45b4c710e668eed6cfee18b4b513f923fbfe327022f01d4f15
```

Vote on the proposal:

```shell
drk> dao vote {PROPOSAL_BULLA} 1 | broadcast

[mark_tx_spend] Processing transaction: 47240cd8ae28eb4d1768029b488d93fe6df6c2c6847cc987ce79f75dfcd56cdc
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 47240cd8ae28eb4d1768029b488d93fe6df6c2c6847cc987ce79f75dfcd56cdc
```

And execute it, after the vote period(1 block period) has passed:

```shell
drk> dao exec {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: a9d77e2d6a64372cb1cf33ed062e0439e617b88ca6374917c83cd284d788d1ce
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: a9d77e2d6a64372cb1cf33ed062e0439e617b88ca6374917c83cd284d788d1ce
```

Or right away, since the early execution quorum has been reached:

```shell
drk> dao exec --early {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: a9d77e2d6a64372cb1cf33ed062e0439e617b88ca6374917c83cd284d788d1ce
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: a9d77e2d6a64372cb1cf33ed062e0439e617b88ca6374917c83cd284d788d1ce
```

Executing the proposal will just confirm it on-chain, without any
other actions taken.

## DAO->DAO

Let's now try some more exotic operations!

Since we hold the mint authority of the `ANON` token,
instead of transfering some to the DAO, we will mint them
directly into it:

```shell
drk> token mint ANON 20 {DAO_NOTES_PUBLIC_KEY} {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} | broadcast

[mark_tx_spend] Processing transaction: 781632eb1d0e4566582c1bb34f4a99516d62357761659d4e5e965ac9d199b581
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 2
Broadcasting transaction...
Transaction ID: 781632eb1d0e4566582c1bb34f4a99516d62357761659d4e5e965ac9d199b581
```

After confirmation we will see the dao holding its own
governance tokens in its treasury:

```shell
drk> dao balance AnonDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN1} | ANON    | 20
 {TOKEN2} | DAWN    | 5
```

Now we will create a second dao:

```shell
drk> dao create 20 10 10 0.67 DAWN | dao import DawnDAO

Importing "DawnDAO" DAO into the wallet
```

And mint it on-chain:

```shell
drk> dao mint DawnDAO | broadcast

[mark_tx_spend] Processing transaction: cfc31bee7d198d7d59e9f40f76a98e93230320ec6dd8c606af32d9bee28fcf0e
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: cfc31bee7d198d7d59e9f40f76a98e93230320ec6dd8c606af32d9bee28fcf0e
```

We propose a transfer of some of the `ANON` governance token
from the DAO treasury to the new DAO we created:

```shell
drk> dao list DawnDAO

DAO Parameters
==============
Name: DawnDAO
Bulla: EHNBPkxnDHEVbGDjJ4yWJatgQif2VM2r2sZMWNpTJB2i
Proposer limit: 20 (2000000000)
Quorum: 10 (1000000000)
Early Exec Quorum: 10 (1000000000)
Approval ratio: 0.67
Governance Token ID: {TOKEN2}
Notes Public key: 9TH2EM...6RxfhL
Notes Secret key: 56UxMM...ADqoyo
Proposer Public key: u4DuNk...xmXM6T
Proposer Secret key: 4ZidQT...HN7js7
Proposals Public key: BNCSWt...g8jFcF
Proposals Secret key: H5bs3y...6qHuZt
Votes Public key: 8V5htk...8EqSrs
Votes Secret key: 7gZ38q...McCvoR
Exec Public key: 6xzupH...3gSecA
Exec Secret key: 43Xgq6...KYt8UK
Early Exec Public key: FiepdF...G5TqVE
Early Exec Secret key: 9ZABgX...vwv1xY
Bulla blind: DCiDUE...jsCCD1
Leaf position: Position(3)
Mint height: 23
Transaction hash: cfc31bee7d198d7d59e9f40f76a98e93230320ec6dd8c606af32d9bee28fcf0e
Call index: 0
```

```shell
drk> dao propose-transfer AnonDAO 1 6.9 ANON {DAWN_DAO_NOTES_PUBLIC_KEY} {DAO_CONTRACT_SPEND_HOOK} {DAWN_DAO_BULLA}

Generated proposal: {PROPOSAL_BULLA}
```

```shell
drk> dao proposal --mint-proposal {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: ed1b365d35abb632521a68146b6678efce9cd000de0ed1dbf4b07818686a7283
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: ed1b365d35abb632521a68146b6678efce9cd000de0ed1dbf4b07818686a7283
```

Vote on the proposal:

```shell
drk> dao vote {PROPOSAL_BULLA} 1 | broadcast

[mark_tx_spend] Processing transaction: 9dd81f166115563e88262ef9ed83b15112dd72247bf48ce7b161779405830a63
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 9dd81f166115563e88262ef9ed83b15112dd72247bf48ce7b161779405830a63
```

And execute it, after the vote period(1 block period) has passed:

```shell
drk> dao exec {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: b78824d5d6c6e6fdb6a002848353dc60279e1c8800e2741062f8944c44796582
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 3
Broadcasting transaction...
Transaction ID: b78824d5d6c6e6fdb6a002848353dc60279e1c8800e2741062f8944c44796582
```

Or right away, since the early execution quorum has been reached:

```shell
drk> dao exec --early {PROPOSAL_BULLA} | broadcast

[mark_tx_spend] Processing transaction: b78824d5d6c6e6fdb6a002848353dc60279e1c8800e2741062f8944c44796582
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 3
Broadcasting transaction...
Transaction ID: b78824d5d6c6e6fdb6a002848353dc60279e1c8800e2741062f8944c44796582
```

After the proposal has been executed on chain, we will see that
the DAO governance token balance has been reduced by 6.9 `ANON`,
while the new DAO balance has been increased by the same amount:

```shell
drk> dao balance AnonDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN1} | ANON    | 13.1
 {TOKEN2} | DAWN    | 5
```

```shell
drk> dao balance DawnDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN1} | ANON    | 6.9
```

## Mining for a DAO

A DAO can deploy its own mining nodes and/or other miners can choose to
directly give their rewards towards one. To retrieve a DAO mining
configuration, execute:

```shell
drk> dao mining-config {YOUR_DAO}

DarkFi TOML configuration:
recipient = "{YOUR_DAO_NOTES_PUBLIC_KEY}"
spend_hook = "{DAO_CONTRACT_SPEND_HOOK}"
user_data = "{YOUR_DAO_BULLA}"

P2Pool wallet address to use:
{YOUR_DAO_P2POOL_WALLET_ADDRESS_CONFIGURATION}
```

Then configure a `darkfid` instance to mine for a DAO, by setting the
corresponding fields(uncomment if needed) as per retrieved
configuration:

```toml
# Put your DAO notes public key here
recipient = "{YOUR_DAO_NOTES_PUBLIC_KEY}"

# Put the DAO contract spend hook here
spend_hook = "{DAO_CONTRACT_SPEND_HOOK}"

# Put your DAO bulla here
user_data = "{YOUR_DAO_BULLA}"
```

After your miners have successfully mined confirmed blocks, you will
see the DAO `DRK` balance increasing:

```shell
drk> dao balance {YOUR_DAO}

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 40
```
