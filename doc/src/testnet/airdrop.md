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

# Minting tokens

On the DarkFi network, we're also able to mint custom tokens with
some supply. To do this, we need to generate a mint authority keypair,
and derive a token ID from it. We can simply do this by executing the
following command:

```
$ ./drk token generate-mint
```

This will generate a new token mint authority and will tell you what
your new token ID is. For tutorial purposes, let's assume the tokens
we will be minting are `DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq`
and `AcABG4fnmBuT5vuXV8TLdEV8panhk5SdtBZxCCLqQxyL`.

You can also list your mint authorities with:

```
$ ./drk token list
```

Now let's mint some tokens to ourself. First grab your wallet address,
and then create the token mint transaction, and finally - broadcast it:

```
$ ./drk wallet --address
$ ./drk token mint DARKfZX1utGbz8ZpnvtCH6i46nSDZEEGa5fMnhoubWPq 42.69 YOUR_ADDRESS > mint_tx
$ ./drk broadcast < mint_tx

$ ./drk token mint AcABG4fnmBuT5vuXV8TLdEV8panhk5SdtBZxCCLqQxyL 20.0 YOUR_ADDRESS > mint_tx
$ ./drk broadcast < mint_tx
```

Now the transaction should be published to the network. If you have
an active block subscription (which you can do with `drk subscribe`),
then when the transaction is finalized, your wallet should have your
new tokens listed when you request to see the balance.
