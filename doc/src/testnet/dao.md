# DAO

On the testnet, we are also able to create an anonymous DAO. Using
the `drk` CLI tool, we have a `dao` subcommand that can perform the
necessary operations. Let's create a DAO with the following parameters:

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
./drk dao list 1
```

## Minting

If parameters are shown, this means the DAO was successfully imported
into our wallet. The DAO's index in our wallet is `1`, so we'll use
that to reference it. Now we can create a transaction that will mint
the DAO on-chain, and broadcast it:

```
./drk dao mint 1 > dao_mint_tx
./drk broadcast < dao_mint_tx
```

Now the transaction is broadcasted to the network. Wait for it to
finalize, and if your `drk` is subscribed, after finalization you
should see a `leaf_position` and a transaction ID when running
`dao list 1`.

## Sending money to the treasury

Let's send some tokens to the DAO's treasury so we're able to make
a proposal to send those somewhere. First find the DAO bulla and the
DAO public key with `dao list` and then create a transfer transaction:

```
$ ./drk dao list 1
$ ./drk transfer 10 WCKD {DAO_PUBLIC_KEY} \
    --dao {DAO_BULLA} > dao_transfer
$ ./drk broadcast < dao_transfer
```

Wait for it to finalize, and if subscribed, you should see the DAO
receive the funds:

```
$ ./drk dao balance 1
```

## Creating a proposal

Now that the DAO has something in its treasury, we can create a
proposal to send it somewhere. Let's send 5 of the 10 tokens to our
address (we can find that with `drk wallet --address`):

```
$ ./drk dao propose 1 {YOUR_ADDRESS} 5 WCKD > proposal_tx
$ ./drk broadcast < proposal_tx
```

Once finalized and scanned, the proposal should be viewable in the
wallet. We can see this with the `proposal` subcommands:

```
$ ./drk dao proposals 1
$ ./drk dao proposal 1 1
```

NOTE: vote & exec is todo, check src/contract/dao/ for code.

