# DAO

On the testnet, we are also able to create an anonymous DAO. Using
the `drk` CLI tool, we have a `dao` subcommand that can perform the
necessary operations.

DarkFi DAOs have several configurable parameters, including:

**Proposer_limit**: the minimum amount of governance tokens needed to
open a proposal.
**Quorum**: The minimal threshold of participating total tokens needed
for a proposal to pass (expressed as an absolute value).
**Early execution quorum**: The minimal threshold of participating total
tokens needed for a proposal to be considered as strongly supported,
enabling early execution. Must be greater or equal to normal quorum.
**Approval_ratio**: The ratio of winning/total votes needed for a proposal
to pass.
**Governance token**: The DAO's governance token ID.

Let's create a DAO with the following parameters:

* Proposer limit: `10`
* Quorum: `5`
* Early execution quorum: `5`
* Approval ratio: `0.67`
* Governance token: `MLDY`

You can use the tokens we created earlier to create new tokens. Return to
the definition of each parameter by running the `help` command like this:

```
$ ./drk help dao create
```

Now let's create our DAO. 

```
$ ./drk dao create 5 10 10 0.67 MLDY > dao_anon.toml
```

```
$ ./drk dao view < dao_anon.toml
```

Since its a normal `toml` file, you may open it with you favourite
editor, modify the keys configuration and/or maintain different config
versions for different DAO members. By default all keys are different,
so its up to the DAO founders to chose what configuration they are going
to use.  After configuring the file(s) properly, it can be shared among
DAO members, so they hold the generated DAO information and keys.
The view command will show us the parameters. If everything looks fine,
we can now import it into our wallet:

```
$ ./drk dao import AnonDAO < dao_anon.toml
```

```
$ ./drk dao list
```

```
$ ./drk dao list AnonDAO
```

## Minting

If parameters are shown, this means the DAO was successfully imported
into our wallet. We use the DAO name to reference it. Now we can create
a transaction that will mint the DAO on-chain, if we hold all its keys,
and broadcast it:

```
$ ./drk dao mint AnonDAO > dao_anon_mint_tx
```

```
$ ./drk broadcast < dao_anon_mint_tx
```

Now the transaction is broadcasted to the network. Wait for it to
confirm, and if your `drk` is subscribed, after confirmation you
should see a leaf position and a transaction hash when running
`dao list AnonDAO`.

## Sending money to the treasury

Let's send some tokens to the DAO's treasury so we're able to make
a proposal to send those somewhere. First, find the DAO bulla, the
dao contract spend hook and the DAO notes public key.

Then create a transfer transaction as follows:

```
$ ./drk dao spend-hook
```

```
$ ./drk dao list AnonDAO
```

```
$ ./drk transfer 10 WCKD {DAO_NOTES_PUBLIC_KEY} \
    {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} > dao_anon_transfer_tx
```

```
$ ./drk broadcast < dao_anon_transfer_tx
```

Wait for it to confirm. If `drk` is subscribed and you hold the DAO
notes key, you can view the balance like so:

```
$ ./drk dao balance AnonDAO
```

## Creating a proposal

Now that the DAO has something in its treasury, we can generate a
transfer proposal to send it somewhere, that will be up to vote
for 1 block period, if we hold the DAO proposer key. Let's propose
to send 5 of the 10 tokens to our address (we can find that with
`drk wallet --address`):

```
$ ./drk dao propose-transfer AnonDAO 1 5 WCKD {YOUR_ADDRESS}
```

After command was executed, it will output the generated proposal
bulla, which we will use to view the proposal full information:

```
$ ./drk dao proposal {PROPOSAL_BULLA}
```

We can export this proposal, to share with rest DAO members.
The exported file will be encrypted using the DAO proposals view key,
so only its members can decrypt and import it.

```
$ ./drk dao proposal {PROPOSAL_BULLA} --export > dao_anon_transfer_proposal.dat
$ ./drk dao proposal-import < dao_anon_transfer_proposal.dat
```

Now we can create the proposal mint transaction and broadcast it:
```
$ ./drk dao proposal {PROPOSAL_BULLA} --mint-proposal > dao_anon_transfer_proposal_mint_tx
$ ./drk broadcast < dao_anon_transfer_proposal_mint_tx
```

Members that didn't receive the encrypted file will receive the
proposal when they scan the corresponding block, but its plaintext
data will be missing, so they should ask the DAO for it.
Once confirmed and scanned, you should see a leaf position and a
transaction hash when running `dao proposal {PROPOSAL_BULLA}`.

## Voting on a proposal

Now the DAO members are ready to cast their votes.
First lets check the `dao vote` subcommand usage.

```
$ ./drk help dao vote
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

Lets use our `MLDY` governance tokens to vote yes to the proposal.

```
$ ./drk dao vote {PROPOSAL_BULLA} 1 > dao_anon_transfer_proposal_vote_tx
$ ./drk broadcast < dao_anon_transfer_proposal_vote_tx
```

Once confirmed and scanned, you should see votes information and
current status when running `dao proposal {PROPOSAL_BULLA}`,
assuming you hold the votes view key.

## Executing the proposal

Once the block period has passed(~4h) and enough votes have been cast that
meet the required minimum (quorum), and assuming the yes:no votes ratio
ratio is bigger than the approval ratio, then we are ready to confirm
the vote. Only DAO members with the executor key can perform this action.

```
$ ./drk dao exec {PROPOSAL_BULLA} > dao_anon_transfer_proposal_exec_tx
$ ./drk broadcast < dao_anon_transfer_proposal_exec_tx
```

Since in our tutorial the `MLDY` governance tokens we used surpass the
early execution quorum, we can execute the proposal right away, if we hold
both the DAO executor and early executor keys:

```
$ ./drk dao exec --early {PROPOSAL_BULLA} > dao_anon_transfer_proposal_exec_tx
$ ./drk broadcast < dao_anon_transfer_proposal_exec_tx
```

After the proposal has been executed on chain, we will see that
the DAO balance has been reduced by 5 `WCKD`, if we hold the DAO notes key,
while our own balance has been increased by the same amount:

```
$ ./drk dao balance AnonDAO
$ ./drk wallet --balance
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
