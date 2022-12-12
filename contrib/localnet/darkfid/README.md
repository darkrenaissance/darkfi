darkfid localnet
================

This will start three darkfid and one faucetd network. Two of the
darkfid participate in the consensus, and one is just a sync node.

The faucetd is able to airdrop arbitrary tokens into wallets.

|   Node     |     RPC endpoint       | Consensus |
|------------|------------------------|-----------|
| `darkfid0` | `tcp://127.0.0.1:8340` |  `true`   |
| `darkfid1` | `tcp://127.0.0.1:8440` |  `true`   |
| `darkfid2` | `tcp://127.0.0.1:8540` |  `false`  |
| `faucetd`  | `tcp://127.0.0.1:8640` |  `false`  |

## Initialize wallets

```
$ ./drk -e tcp://127.0.0.1:8340 wallet --initialize
$ ./drk -e tcp://127.0.0.1:8340 wallet --keygen

$ ./drk -e tcp://127.0.0.1:8440 wallet --initialize
$ ./drk -e tcp://127.0.0.1:8440 wallet --keygen

$ ./drk -e tcp://127.0.0.1:8540 wallet --initialize
$ ./drk -e tcp://127.0.0.1:8540 wallet --keygen
```

Make note of the addresses given by `keygen`.

## Airdrops

Let's airdrop some money do `darkfid1` and `darkfid2`. For this to
work, we also need to subscribe to their RPC endpoints so we can scan
incoming blocks and add them to our wallet.

```
$ ./drk -e tcp://127.0.0.1:8440 subscribe
$ ./drk -e tcp://127.0.0.1:8540 subscribe
```

And now we can execute our airdrop calls:

```
$ ./drk -e tcp://127.0.0.1:8440 airdrop -f tcp://127.0.0.1:8640 \
    15.57 A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd

$ ./drk -e tcp://127.0.0.1:8540 airdrop -f tcp://127.0.0.1:8640 \
    66.31 BNBZ9YprWvEGMYHW4dFvbLuLfHnN9Bs64zuTFQAbw9Dy
```

If successful, `drk` should give a transaction ID. Now watch the
network and wait until the blocks with these transactions get
finalized.

Then you can check the wallets' balances:

```
$ ./drk -e tcp://127.0.0.1:8440 wallet --balance
$ ./drk -e tcp://127.0.0.1:8540 wallet --balance
```

## Payments

Now let's try to send a little bit of funds from `darkfid1` to
`darkfid2`:

```
$ ./drk -e tcp://127.0.0.1:8440 transfer 1.33 \
    A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd \
    6uw3S12RWnhikrrTrtVsTJFQQdc5i9QWoVLUEfGd8B5Z \
    > transaction
```

The command will build proving keys, and create a transaction on
stdout which you should redirect into a file.

You can optionally inspect this transaction:

```
$ ./drk -e tcp://127.0.0.1:8440 inspect < transaction
```

And then we broadcast it:

```
$ ./drk -e tcp://127.0.0.1:8440 broadcast < transaction
```

On success, you should see a transaction ID.

Now wait again until the tx is finalized and scanned by your `drk`
subscribers so you'll see the balance changes.


## Swaps

We can also try to swap some coins between `darkfid1` and `darkfid2`.

First `darkfid1` will initiate the 1st half of the swap, and send it
to `darkfid2`:

```
$ ./drk -e tcp://127.0.0.1:8440 otc init \
    -v 14.24:66.41 \
    -t A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:BNBZ9YprWvEGMYHW4dFvbLuLfHnN9Bs64zuTFQAbw9Dy \
    > half_swap
```

Then `darkfid2` can create the second half and send it back to
`darkfid1`:

```
$ ./drk -e tcp://127.0.0.1:8540 otc join < half_swap > full_swap
```

And finally `darkfid1` can sign and broadcast it:

```
$ ./drk -e tcp://127.0.0.1:8440 otc sign < full_swap > signed_swap
$ ./drk -e tcp://127.0.0.1:8440 broadcast < signed_swap
```

After finalization, the balances should update.
