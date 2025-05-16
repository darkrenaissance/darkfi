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

You can use the tokens we created earlier to create new tokens. Return to
the definition of each parameter by running the `help` command like this:

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
$ ./drk dao create 20 10 10 0.67 ANON > anon_dao.toml
```

And view it:

```shell
$ ./drk dao view < anon_dao.toml

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
$ ./drk dao import AnonDAO < anon_dao.toml

Importing "AnonDAO" DAO into the walle
```

```shell
$ ./drk dao list

0. AnonDAO
```

```shell
$ ./drk dao list AnonDAO

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
$ ./drk dao mint AnonDAO > anon_dao_mint.tx
```

```shell
$ ./drk broadcast < dao_anon_mint_tx

[mark_tx_spend] Processing transaction: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
```

Now the transaction is broadcasted to the network. Wait for it to
confirm, and if your `drk` is subscribed, after confirmation you
should see a leaf position and a transaction hash when running:

```shell
$ ./drk dao list AnonDAO

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
Transaction hash: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
Call index: 0
```

## Sending money to the treasury

Let's send some tokens to the DAO's treasury so we're able to make
a proposal to send those somewhere. First, find the DAO bulla, the
dao contract spend hook and the DAO notes public key.

Then create a transfer transaction as follows:

```shell
$ ./drk dao spend-hook

6iW9nywZYvyhcM7P1iLwYkh92rvYtREDsC8hgqf2GLuT
```

```shell
$ ./drk dao list AnonDAO

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
Transaction hash: 2e7931f200c1485ea7752076e199708b011a504d71e69d60ed606817c5ff4bd5
Call index: 0
```

```shell
$ ./drk transfer 10 DAWN {DAO_NOTES_PUBLIC_KEY} {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} > anon_dao_transfer.tx
```

```shell
$ ./drk broadcast < anon_dao_transfer.tx

[mark_tx_spend] Processing transaction: a4db439f75de88457cadd849131394ae37723c943ea5c088b218d6dc0f7982f1
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: a4db439f75de88457cadd849131394ae37723c943ea5c088b218d6dc0f7982f1
```

Wait for it to confirm. If `drk` is subscribed and you hold the DAO
notes key, you can view the balance like so:

```shell
$ ./drk dao balance AnonDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN2} | DAWN    | 10
```

## Creating a proposal

Now that the DAO has something in its treasury, we can generate a
transfer proposal to send it somewhere, that will be up to vote
for 1 block period, if we hold the DAO proposer key. Let's propose
to send 5 of the 10 tokens to our address (we can find that with
`drk wallet --address`):

```shell
$ ./drk dao propose-transfer AnonDAO 1 5 DAWN {YOUR_ADDRESS}

Generated proposal: {PROPOSAL_BULLA}
```

After command was executed, it will output the generated proposal
bulla, which we will use to view the proposal full information:

```shell
$ ./drk dao proposal {PROPOSAL_BULLA}

Proposal parameters
===================
Bulla: {PROPOSAL_BULLA}
DAO Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposal leaf position: None
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
$ ./drk dao proposal {PROPOSAL_BULLA} --export > anon_dao_transfer_proposal.dat
```

```shell
$ ./drk dao proposal-import < anon_dao_transfer_proposal.dat
```

Now we can create the proposal mint transaction:
```shell
$ ./drk dao proposal {PROPOSAL_BULLA} --mint-proposal > anon_dao_transfer_proposal_mint.tx
```

And broadcast it
```shell
$ ./drk broadcast < anon_dao_transfer_proposal_mint.tx

[mark_tx_spend] Processing transaction: 2149d7e3a60be12c96b6c6fc7ba009717d8b229b815dd4006bbe120c31681f38
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 2149d7e3a60be12c96b6c6fc7ba009717d8b229b815dd4006bbe120c31681f38
```

Members that didn't receive the encrypted file will receive the
proposal when they scan the corresponding block, but its plaintext
data will be missing, so they should ask the DAO for it.
Once confirmed and scanned, you should see a leaf position and a
transaction hash when running:

```shell
$ ./drk dao proposal {PROPOSAL_BULLA}

Proposal parameters
===================
Bulla: G9FUrWn6PLieNuPpYdzUmfd1UP9tUVMpimmu7mwMukcU
DAO Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposal leaf position: Position(0)
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
$ ./drk dao vote {PROPOSAL_BULLA} 1 > anon_dao_transfer_proposal_vote.tx
```

And broadcast our vote:

```shell
$ ./drk broadcast < anon_dao_transfer_proposal_vote.tx

[mark_tx_spend] Processing transaction: 060468c5676a52a8b59b464dc959906b762a2108fa6f9d0db0b88c9d200eb612
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 060468c5676a52a8b59b464dc959906b762a2108fa6f9d0db0b88c9d200eb612
```

Once confirmed and scanned, you should see votes information and
current status, assuming you hold the votes view key, by running:

```shell
$ ./drk dao proposal {PROPOSAL_BULLA}

Proposal parameters
===================
Bulla: G9FUrWn6PLieNuPpYdzUmfd1UP9tUVMpimmu7mwMukcU
DAO Bulla: AWnAra8wXPxKfJ6qBqXt3Kto83RLCrC32wWZCZUMfwgy
Proposal leaf position: Position(0)
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
$ ./drk dao exec {PROPOSAL_BULLA} > anon_dao_transfer_proposal_exec.tx
```

```shell
$ ./drk broadcast < anon_dao_transfer_proposal_exec.tx

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
$ ./drk dao exec --early {PROPOSAL_BULLA} > anon_dao_transfer_proposal_exec.tx
```

```shell
$ ./drk broadcast < anon_dao_transfer_proposal_exec.tx

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
$ ./drk dao balance AnonDAO

 Token ID | Aliases | Balance
----------+---------+---------
 {TOKEN2} | DAWN    | 5
```

```shell
$ ./drk wallet --balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+-------------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 19.93153568
 {TOKEN1}                                     | ANON    | 40
 {TOKEN2}                                     | DAWN    | 15
```

## Generic proposal

DAOs can vote on off-chain operations by creating what is known as generic
proposals, meaning that no on-chain action is tied to it:

```
$ ./drk dao propose-generic AnonDAO 1
$ ./drk dao proposal {PROPOSAL_BULLA} --mint-proposal > dao_anon_generic_proposal_mint_tx
$ ./drk broadcast < dao_anon_generic_proposal_mint_tx
```

Vote on the proposal:

```
$ ./drk dao vote {PROPOSAL_BULLA} 1 > dao_anon_generic_proposal_vote_tx
$ ./drk broadcast < dao_anon_generic_proposal_vote_tx
```

And execute it, after the vote period(1 block period) has passed:

```
$ ./drk dao exec {PROPOSAL_BULLA} > dao_anon_generic_proposal_exec_tx
$ ./drk broadcast < dao_anon_generic_proposal_exec_tx
```

Or right away, since the early execution quorum has been reached:

```
$ ./drk dao exec --early {PROPOSAL_BULLA} > dao_anon_generic_proposal_exec_tx
$ ./drk broadcast < dao_anon_generic_proposal_exec_tx
```

Executing the proposal will just confirm it on-chain, without any
other actions taken.

## DAO->DAO

Let's now try some more exotic operations!

Since we hold the mint authority of the `MLDY` token,
instead of transfering some to the DAO, we will mint them
directly into it:

```
$ ./drk token mint MLDY 20 {DAO_NOTES_PUBLIC_KEY} \
    {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} > mint_dao_anon_tx
$ ./drk broadcast < mint_dao_anon_tx
```

After confirmation we will see the dao holding its own
governance tokens in its treasury:

```
$ ./drk dao balance AnonDAO
```

Now we will create a second dao:

```
$ ./drk dao create 20 10 10 0.67 WCKD > dao_fren.toml
$ ./drk dao import FrenDAO < dao_fren.toml
$ ./drk dao mint FrenDAO > dao_fren_mint_tx
$ ./drk broadcast < dao_fren_mint_tx
```

We propose a transfer of some of the `MLDY` governance token
from the DAO treasury to the new DAO we created:

```
$ ./drk dao list FrenDAO
$ ./drk dao propose-transfer AnonDAO 1 6.9 MLDY {FREN_DAO_NOTES_PUBLIC_KEY} \
    {DAO_CONTRACT_SPEND_HOOK} {FREN_DAO_BULLA}
$ ./drk dao proposal {PROPOSAL_BULLA} --mint-proposal > dao_anon_transfer_proposal_fren_mint_tx
$ ./drk broadcast < dao_anon_transfer_proposal_fren_mint_tx
```

Vote on the proposal:

```
$ ./drk dao vote {PROPOSAL_BULLA} 1 > dao_anon_transfer_proposal_fren_vote_tx
$ ./drk broadcast < dao_anon_transfer_proposal_fren_vote_tx
```

And execute it, after the vote period(1 block period) has passed:

```
$ ./drk dao exec {PROPOSAL_BULLA} > dao_anon_transfer_proposal_fren_exec_tx
$ ./drk broadcast < dao_anon_transfer_proposal_fren_exec_tx
```

Or right away, since the early execution quorum has been reached:

```
$ ./drk dao exec --early {PROPOSAL_BULLA} > dao_anon_transfer_proposal_fren_exec_tx
$ ./drk broadcast < dao_anon_transfer_proposal_fren_exec_tx
```

After the proposal has been executed on chain, we will see that
the DAO governance token balance has been reduced by 6.9 `MLDY`,
while the new DAO balance has been increased by the same amount:

```
$ ./drk dao balance AnonDAO
$ ./drk dao balance FrenDAO
```
