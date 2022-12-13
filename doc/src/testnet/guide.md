DarkFi Testnet User Guide
=========================

This document presents a short user guide for the initial DarkFi
testnet. In it, we cover basic setup of the `darkfid` node daemon,
initializing a wallet, and interacting with the _money contract_,
which provides infrastructure for payments and atomic swaps.

Since this is still an early phase, we will not be installing any of
the software system-wide. Instead, we'll be running all the commands
from the git repository, so we're able to easily pull any necessary
updates.


## Compiling

Refer to the main
[README](https://github.com/darkrenaissance/darkfi/blob/master/README.md)
file for instructions on how to install Rust and necessary deps.

Once you have the repository in place, and everything is installed, we
can compile the `darkfid` node and the `drk` wallet CLI:

```
$ make darkfid drk
```

This process will now compile the node and the wallet CLI tool.
When finished, we can begin using the network. Run `darkfid` once so
that it spawns its config file on your system. This config file will
be used by `darkfid` in order to configure itself. The defaults are
already preset for using the testnet network.

```
$ ./darkfid
Config file created in "~.config/darkfi/darkfid_config.toml". Please review it and try again.
```


## Running

Once that's in place, you can run it again and `darkfid` will start,
create necessary keys for validation of blocks and transactions, and
begin syncing the blockchain. Keep it running, and you should see a
`Blockchain is synced!` message after some time.

```
$ ./darkfid
```

Now it's time to initialize your wallet. For this we use a separate
wallet CLI which is created to interface with the smart contract used
for payments and swaps.

We simply have to initialize a wallet, and create a keypair:

```
$ ./drk wallet --initialize
$ ./drk wallet --keygen
```

The second command will print out your new DarkFi address where you
can receive payments. Take note of it. Alternatively, you can always
retrieve it using:

```
$ ./drk wallet --address
```

In order to receive incoming coins, you'll need to use the `drk`
tool to subscribe on `darkfid` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In another terminal,
you can run the following commands to first scan the blockchain,
and then to subscribe to new blocks:

```
$ ./drk scan
$ ./drk subscribe
```

Now you can leave the subscriber running. In case you stop it, just
run `drk scan` again until the chain is fully scanned, and then you
should be able to subscribe again.


## Airdrops

Now you have your wallet set up. Let's proceed with getting some
tokens from the faucet. The testnet has a running faucet which is
able to airdrop arbitrary tokens. For example purposes, we'll use
the tokens called `DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq` and
`BobvfQrDaf32VNhVtX6Adyi3WGfPpPYZPJBn6rnrxHKm`

So let's airdrop some of these into our wallet:

```
$ ./drk airdrop 42.69 DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq
$ ./drk airdrop 13.37 BobvfQrDaf32VNhVtX6Adyi3WGfPpPYZPJBn6rnrxHKm
```

On success, you should see a transaction ID. If successful,
the airdrop transactions will how be in the consensus' mempool,
waiting for inclusion in the next block. Depending on the network,
finalization of the blocks could take some time. You'll have to wait
for this to happen.  If your `drk subscribe` is running, then after
some time your balance should be in your wallet.

![pablo-waiting0](pablo0.jpg)

You can check your wallet balance using `drk`:

```
$ ./drk wallet --balance
```

## Payments

Using the tokens we got, we can make payments to other addresses. Let's
try to send some `DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq` tokens
to `8sRwB7AwBTKEkyTW6oMyRoJWZhJwtqGTf7nyHwuJ74pj`:

```
$ ./drk transfer 2.69 DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq \
    8sRwB7AwBTKEkyTW6oMyRoJWZhJwtqGTf7nyHwuJ74pj > payment_tx
```

The above command will create a transfer transaction and place it into
the file called `payment_tx`. Then we can broadcast this transaction
to the network:

```
$ ./drk broadcast < payment_tx
```

On success we'll see a transaction ID. Now again the same finalization
process has to occur and `8sRwB7AwBTKEkyTW6oMyRoJWZhJwtqGTf7nyHwuJ74pj`
will receive the tokens you've sent.

![pablo-waiting1](pablo1.jpg)

We have to wait until the next block to see our change balance reappear
in our wallet.

```
$ ./drk wallet --balance
```

We can see the spent coin in our wallet.

```
$ ./drk wallet --coins
```

## Atomic Swaps

In order to do an atomic swap with someone, you will
first have to come to consensus on what tokens you wish to
swap. For example purposes, let's say you want to swap `40`
`DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq` (which is the balance
you should have left over after doing the above payment) for your
counterparty's `20` `AcABG4fnmBuT5vuXV8TLdEV8panhk5SdtBZxCCLqQxyL`.

You'll have to initiate the swap and build your half of the swap tx:

```
$ ./drk otc init -v 40.0:20.0 \
    -t DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq:AcABG4fnmBuT5vuXV8TLdEV8panhk5SdtBZxCCLqQxyL \
    > half_swap
```

Then you can send this `half_swap` file to your counterparty and they
can create the other half by running:

```
$ ./drk otc join < half_swap > full_swap
```

They will sign the full_swap file and send it back to you. Finally,
to make the swap transaction valid, you need so sign it as well,
and broadcast it:

```
$ ./drk otc sign < full_swap > signed_swap
$ ./drk broadcast < signed_swap
```

On success, you should see a transaction ID. This transaction will now
also be in the mempool, so you should wait again until it's finalized.

![pablo-waiting2](pablo2.jpg)

After a while you should see the change in balances in your wallet:

```
$ ./drk wallet --balance
```

If you see your counterparty's tokens, that means the swap was
successful.  In case you still see your old tokens, that could mean
that the swap transaction has not yet been finalized.
