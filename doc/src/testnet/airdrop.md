# Airdrops

Now you have your wallet set up. Let's proceed with getting some
tokens from the faucet. The testnet has a running faucet which is
able to airdrop native network tokens. 

So let's airdrop some of these into our wallet:

```
$ ./drk airdrop 42.69
```

There is a limit of 100 for testnet airdrops currently.

Note: you have wait some minutes between airdrops since they're
rate-limited.

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

# Aliases

To make our life easier, we can create token ID aliases, so when we
are performing transactions with them, we can use that instead of the
full token ID. Multiple aliases per token ID is supported.

Example addition:

```
$ ./drk alias add {ALIAS} {TOKEN}
```

So lets add the native token as `DARK` by executing:

```
$ ./drk alias add DARK 12ea8e3KVuBhmSnr29iV34Zd2RsD1MEeGk9xJhcipUqx
```

From now on, we can use `DARK` to refer to the native token when
executing transactions using it.

We can also list all our aliases using:

```
$ ./drk alias show
```

Note: this aliases are only local to your machine. When exchanging
with other users, always verify that your aliases token IDs match.

# Minting tokens

On the DarkFi network, we're also able to mint custom tokens with
some supply. To do this, we need to generate a mint authority keypair,
and derive a token ID from it. We can simply do this by executing the
following command:

```
$ ./drk token generate-mint
```

This will generate a new token mint authority and will tell you what
your new token ID is. For this tautorial we will need two tokens so
execute the command again to generate another one.

You can list your mint authorities with:

```
$ ./drk token list
```

Now lets add those two token IDs to our aliases:

```
$ ./drk alias add WCKD {TOKEN1}
$ ./drk alias add MLDY {TOKEN2}
```

Now let's mint some tokens to ourself. First grab your wallet address,
and then create the token mint transaction, and finally - broadcast it:

```
$ ./drk wallet --address
$ ./drk token mint WCKD 42.69 {YOUR_ADDRESS} > mint_tx
$ ./drk broadcast < mint_tx

$ ./drk token mint MLDY 20.0 {YOUR_ADDRESS} > mint_tx
$ ./drk broadcast < mint_tx
```

Now the transaction should be published to the network. If you have
an active block subscription (which you can do with `drk subscribe`),
then when the transaction is finalized, your wallet should have your
new tokens listed when you request to see the balance.
