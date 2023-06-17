# DAO

On the testnet, we are also able to create an anonymous DAO. Using
the `drk` CLI tool, we have a `dao` subcommand that can perform the
necessary operations.

You can find a script in
`contrib/localnet/darkfid-single-node/run-dao-test.sh` which
automatically does all the commands in this tutorial. Just be sure
to read the comment at the top of the file first.

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
$ ./drk dao create 20 10 0.67 MLDY > dao.dat
$ ./drk dao view < dao.dat
```

The view command will show us the parameters. If everything looks fine,
we can now import it into our wallet:

```
./drk dao import MiladyMakerDAO < dao.dat
./drk dao list
./drk dao list MiladyMakerDAO
```

## Minting

If parameters are shown, this means the DAO was successfully imported
into our wallet. The DAO's index in our wallet is `1`, so we'll use
that to reference it. Now we can create a transaction that will mint
the DAO on-chain, and broadcast it:

```
./drk dao mint MiladyMakerDAO > dao_mint_tx
./drk broadcast < dao_mint_tx
```

Now the transaction is broadcasted to the network. Wait for it to
finalize, and if your `drk` is subscribed, after finalization you
should see a `leaf_position` and a transaction ID when running
`dao list MiladyMakerDAO`.

## Sending money to the treasury

Let's send some tokens to the DAO's treasury so we're able to make
a proposal to send those somewhere. First find the DAO bulla and the
DAO public key with `dao list` and then create a transfer transaction:

```
$ ./drk dao list MiladyMakerDAO
$ ./drk transfer 10 WCKD {DAO_PUBLIC_KEY} \
    --dao {DAO_BULLA} > dao_transfer
$ ./drk broadcast < dao_transfer
```

Wait for it to finalize, and if subscribed, you should see the DAO
receive the funds:

```
$ ./drk dao balance MiladyMakerDAO
```

## Creating a proposal

Now that the DAO has something in its treasury, we can create a
proposal to send it somewhere. Let's send 5 of the 10 tokens to our
address (we can find that with `drk wallet --address`):

```
$ ./drk dao propose MiladyMakerDAO {YOUR_ADDRESS} 5 WCKD > proposal_tx
$ ./drk broadcast < proposal_tx
```

Once finalized and scanned, the proposal should be viewable in the
wallet. We can see this with the `proposal` subcommands:

```
$ ./drk dao proposals MiladyMakerDAO
$ ./drk dao proposal MiladyMakerDAO 1
```

## Voting on a proposal

Now the DAO members are ready to cast their votes.
First lets check the `dao vote` subcommand usage.

```
$ drk help dao vote
Vote on a given proposal

Usage: drk dao vote <DAO_ALIAS> <PROPOSAL_ID> <VOTE> <VOTE_WEIGHT>

Arguments:
  <DAO_ALIAS>    Name or numeric identifier for the DAO
  <PROPOSAL_ID>  Numeric identifier for the proposal
  <VOTE>         Vote (0 for NO, 1 for YES)
  <VOTE_WEIGHT>  Vote weight (amount of governance tokens)
```

Lets use our 20 MLDY to vote yes to proposal 1.

```
$ drk dao vote MiladyMakerDAO 1 1 20 > /tmp/dao-vote.tx
$ drk broadcast < /tmp/dao-vote.tx
```

## Executing the proposal

Once enough votes have been cast that meet the required minimum (quorum)
and assuming the yes:no votes ratio is bigger than the approval ratio,
then we are ready to finalize the vote. Any DAO member can perform this
action.

```
$ drk dao exec MiladyMakerDAO 1 > /tmp/dao-exec.tx
$ drk broadcast < /tmp/dao-exec.tx
```

