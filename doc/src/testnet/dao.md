# DAO

On the testnet, we are also able to create an anonymous DAO. Using
the `drk` CLI tool, we have a `dao` subcommand that can perform the
necessary operations.

Let's create a DAO with the following parameters:

* Proposer limit: `20`
* Quorum: `10`
* Approval ratio: `0.67`
* Governance token: `MLDY`

You can see what these parameters mean with the `help` command.

```
$ ./drk help dao create
```

Let's create our DAO.

```
$ ./drk dao create 20 10 0.67 MLDY > dao_mldy.dat
$ ./drk dao view < dao_mldy.dat
```

Now this file can be shared amongst all dao members, so they
hold the generated DAO information and keys. The view command
will show us the parameters. If everything looks fine, we can
now import it into our wallet:

```
$ ./drk dao import MiladyMakerDAO < dao_mldy.dat
$ ./drk dao list
$ ./drk dao list MiladyMakerDAO
```

## Minting

If parameters are shown, this means the DAO was successfully imported
into our wallet. We use the DAO name to reference it. Now we can create
a transaction that will mint the DAO on-chain, and broadcast it:

```
$ ./drk dao mint MiladyMakerDAO > dao_mldy_mint_tx
$ ./drk broadcast < dao_mldy_mint_tx
```

Now the transaction is broadcasted to the network. Wait for it to
finalize, and if your `drk` is subscribed, after finalization you
should see a leaf position and a transaction hash when running
`dao list MiladyMakerDAO`.

## Sending money to the treasury

Let's send some tokens to the DAO's treasury so we're able to make
a proposal to send those somewhere. First, find the DAO bulla, the
dao contract spend hook and the DAO public key a d then create a
transfer transaction:

```
$ ./drk dao spend-hook
$ ./drk dao list MiladyMakerDAO
$ ./drk transfer 10 WCKD {DAO_PUBLIC_KEY} \
    {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} > dao_mldy_transfer_tx
$ ./drk broadcast < dao_mldy_transfer_tx
```

Wait for it to finalize, and if subscribed, you should see the DAO
receive the funds:

```
$ ./drk dao balance MiladyMakerDAO
```

## Creating a proposal

Now that the DAO has something in its treasury, we can generate a
transfer proposal to send it somewhere, that will be up to vote
for 30 block periods. Let's propose to send 5 of the 10 tokens to
our address (we can find that with `drk wallet --address`):

```
$ ./drk dao propose-transfer MiladyMakerDAO 30 5 WCKD {YOUR_ADDRESS}
```

After command was executed, it will output the generated proposal
bulla, which we will use to full the proposal information:

```
$ ./drk dao proposal {PROPOSAL_BULLA}
```

We can export this proposal, to share with rest DAO members.
The exported file will be encrypted using the DAO keys, so only
its members can decrypt and import it.

```
$ ./drk dao proposal {PROPOSAL_BULLA} --export > dao_mldy_transfer_proposal.dat
$ ./drk dao proposal-import < dao_mldy_transfer_proposal.dat
```

Now we can create the proposal mint transaction and broadcast it:
```
$ ./drk dao proposal {PROPOSAL_BULLA} --mint-proposal > dao_mldy_transfer_proposal_mint_tx
$ ./drk broadcast < dao_mldy_transfer_proposal_mint_tx
```

Members that didn't receive the encrypted file will receive the
proposal when they scan the corresponding block, but its plaintext
data will be missing, so they should ask the DAO for it.
Once finalized and scanned, you should see a leaf position and a
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
$ ./drk dao vote {PROPOSAL_BULLA} 1 > dao_mldy_transfer_proposal_vote_tx
$ ./drk broadcast < dao_mldy_transfer_proposal_vote_tx
```

Once finalized and scanned, you should see votes information and
current status when running `dao proposal {PROPOSAL_BULLA}`.

## Executing the proposal

Once enough votes have been cast that meet the required minimum (quorum)
and assuming the yes:no votes ratio is bigger than the approval ratio,
then we are ready to finalize the vote. Any DAO member can perform this
action.

Since in our tutorial the `MLDY` governance tokens we used surpass the
quorum, we can execute the proposal right away:

```
$ ./drk dao exec {PROPOSAL_BULLA} > dao_mldy_transfer_proposal_exec_tx
$ ./drk broadcast < dao_mldy_transfer_proposal_exec_tx
```

After the proposal has been executed on chain, we will see that
the DAO balance has been reduced by 5 `WCKD`, while our own balance
has been increased by the same amount:

```
$ ./drk dao balance MiladyMakerDAO
$ ./drk wallet --balance
```

## DAO->DAO

Let's now try some more exotic operations!

Since we hold the mint authority of the `MLDY` token,
instead of transfering some to the DAO, we will mint them
directly into it:

```
$ ./drk token mint MLDY 20 {DAO_PUBLIC_KEY} \
    {DAO_CONTRACT_SPEND_HOOK} {DAO_BULLA} > mint_dao_mldy_tx
$ ./drk broadcast < mint_dao_mldy_tx
```

After finalization we will see the dao holding its own
governance tokens in its treasury:

```
$ ./drk dao balance MiladyMakerDAO
```

Now we will create a second dao:

```
$ ./drk dao create 20 10 0.67 WCKD > dao_wckd.dat
$ ./drk dao import WickedDAO < dao_wckd.dat
$ ./drk dao mint WickedDAO > dao_wckd_mint_tx
$ ./drk broadcast < dao_wckd_mint_tx
```

We propose a transfer of some of the `MLDY` governance token
from the DAO treasury to the new DAO we created:

```
$ ./drk dao list WickedDAO
$ ./drk dao propose-transfer MiladyMakerDAO 30 6.9 MLDY {WICKED_DAO_PUBLIC_KEY} \
    {DAO_CONTRACT_SPEND_HOOK} {WICKED_DAO_BULLA}
$ ./drk dao proposal {PROPOSAL_BULLA} --mint-proposal > dao_mldy_transfer_proposal_wckd_mint_tx
$ ./drk broadcast < dao_mldy_transfer_proposal_wckd_mint_tx
```

Vote on the proposal:

```
$ ./drk dao vote {PROPOSAL_BULLA} 1 > dao_mldy_transfer_proposal_wckd_vote_tx
$ ./drk broadcast < dao_mldy_transfer_proposal_wckd_vote_tx
```

And execute it:

```
$ ./drk dao exec {PROPOSAL_BULLA} > dao_mldy_transfer_proposal_wckd_exec_tx
$ ./drk broadcast < dao_mldy_transfer_proposal_wckd_exec_tx
```

After the proposal has been executed on chain, we will see that
the DAO governance token balance has been reduced by 6.9 `MLDY`,
while the new DAO balance has been increased by the same amount:

```
$ ./drk dao balance MiladyMakerDAO
$ ./drk dao balance WickedDAO
```
