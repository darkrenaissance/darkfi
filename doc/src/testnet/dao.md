# DAO

On the testnet, we are also able to create an anonymous DAO. Using
the `drk` CLI tool, we have a `dao` subcommand that can perform the
necessary operations. Let's create a DAO with the following parameters:

* Proposer limit: `90`
* Quorum: `10`
* Approval ratio: `0.67`
* Governance token: `A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd`

You can see what these parameters mean with the `help` command.

```
$ ./drk help dao create
```

Lets create our DAO.

```
$ ./drk dao create 90 10 0.67 A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd > dao.dat
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
$ ./drk transfer 10 BNBZ9YprWvEGMYHW4dFvbLuLfHnN9Bs64zuTFQAbw9Dy \
    6BwyxvNut6jrPaQ5YPcMVgM3nhwNxvdFhVT4CwQHwJgN \
    --dao CvnfwNHGLEL42mpjETtWknt7ZEQhoJCACGZ9rVFscFw4 > dao_transfer
$ ./drk broadcast < dao_transfer
```

Wait for it to finalize, and if subscribed, you should see the DAO
receive the funds:

```
$ ./drk dao balance 1
```

## Creating a proposal

Now that the DAO has something in their treasury, we can create a
proposal to send it somewhere. Let's send 5 of the 10 tokens to our
address (we can find that with `drk wallet --address`).

Since we chose `90` as the governance token proposal limit, let's
just airdrop that into our wallet first:

```
$ ./drk airdrop 90 A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd
```

Then create the proposal:

```
$ ./drk dao propose 1 ChgNSmpp6pCstPsvYNNT1686fuj1PPobo1C4qWXubr3r \
    5 BNBZ9YprWvEGMYHW4dFvbLuLfHnN9Bs64zuTFQAbw9Dy > proposal_tx
$ ./drk broadcast < proposal_tx
```

Once finalized and scanned, the proposal should be viewable in the
wallet. We can see this with the `proposal` subcommands:

```
$ ./drk dao proposals 1
$ ./drk dao proposal 1 1
```

NOTE: vote & exec is todo, check src/contract/dao/ for code.

