# Airdrops

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

