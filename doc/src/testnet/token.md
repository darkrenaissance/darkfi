# Native token

Now that you have your wallet set up, you will need some native `DRK`
tokens in order to be able to perform transactions, since that token
is used to pay the transaction fees. You can obtain `DRK` either by
successfully mining a block that gets confirmed or by asking for some
by the community on `darkirc` and/or your comrades. Don't forget to
tell them to add the `--half-split` flag when they create the transfer
transaction, so you get more than one coins to play with.

After you request some `DRK` and the other party submitted a
transaction to the network, it should be in the consensus' mempool,
waiting for inclusion in the next block(s). Depending on your network
configuration, confirmation of the blocks could take some time. You'll
have to wait for this to happen. If your `drk subscribe` is running,
then after some time your new balance should be in your wallet.

![pablo-waiting0](img/pablo0.jpg)

You can check your wallet balance using `drk`:

```shell
$ ./drk wallet --balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 20
```

# Creating tokens

On the DarkFi network, we're able to mint custom tokens with some
supply. To do this, we need to generate a mint authority keypair,
and derive a token ID from it. The tokens shown in the outputs are
placeholders for the ones that will be generated from your. We can
simply create our own tokens by executing the following command:

```shell
$ ./drk token generate-mint

Successfully imported mint authority for token ID: {TOKEN1}
```

This will generate a new token mint authority and will tell you what
your new token ID is.

You can list your mint authorities with:

```shell
$ ./drk token list

 Token ID | Aliases | Mint Authority          | Token Blind    | Frozen
----------+---------+-------------------------+----------------+--------
 {TOKEN1} | -       | {TOKEN1_MINT_AUTHORITY} | {TOKEN1_BLIND} | false

```

For this tutorial we will need two tokens so execute the command again
to generate another one.

```shell
$ ./drk token generate-mint

Successfully imported mint authority for token ID: {TOKEN2}
```

Verify you have two tokens by running:

```shell
$ ./drk token list

 Token ID | Aliases | Mint Authority          | Token Blind    | Frozen
----------+---------+-------------------------+----------------+--------
 {TOKEN1} | -       | {TOKEN1_MINT_AUTHORITY} | {TOKEN1_BLIND} | false
 {TOKEN2} | -       | {TOKEN2_MINT_AUTHORITY} | {TOKEN2_BLIND} | false

```

## Aliases

To make our life easier, we can create token ID aliases, so when we
are performing transactions with them, we can use that instead of the
full token ID. Multiple aliases per token ID are supported.

The native token alias `DRK` should already exist, and we can use that
to refer to the native token when executing transactions using it.

We can also list all our aliases using:

```shell
$ ./drk alias show

 Alias | Token ID
-------+----------------------------------------------
 DRK   | 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb
```

Note: these aliases are only local to your machine. When exchanging
with other users, always verify that your aliases' token IDs match.

Now let's add the two token IDs generated earlier to our aliases:

```shell
$ ./drk alias add ANON {TOKEN1}

Generating alias ANON for Token: {TOKEN1}
```

```shell
$ ./drk alias add DAWN {TOKEN2}

Generating alias ANON for Token: {TOKEN2}
```

```shell
$ ./drk alias show

 Alias | Token ID
-------+----------------------------------------------
 ANON  | {TOKEN1}
 DAWN  | {TOKEN2}
 DRK   | 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLss
```

## Mint transaction

Now let's mint some tokens for ourselves. First grab your wallet address,
and then create the token mint transaction, and finally - broadcast it:

```shell
$ ./drk wallet --address

{YOUR_ADDRESS}
```

```shell
$ ./drk token mint ANON 42.69 {YOUR_ADDRESS} > mint.tx
```

```shell
$ ./drk broadcast < mint.tx

[mark_tx_spend] Processing transaction: e9ded45928f2e2dbcb4f8365653220a8e2346987dd8b75fe1ffdc401ce0362c2
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 2
Broadcasting transaction...
Transaction ID: e9ded45928f2e2dbcb4f8365653220a8e2346987dd8b75fe1ffdc401ce0362c2
```

```shell
$ ./drk token mint DAWN 20.0 {YOUR_ADDRESS} > mint.tx
```

```shell
$ ./drk broadcast < mint.tx

[mark_tx_spend] Processing transaction: e404241902ba0a8825cf199b3083bff81cd518ca30928ca1267d5e0008f32277
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 2
Broadcasting transaction...
Transaction ID: e404241902ba0a8825cf199b3083bff81cd518ca30928ca1267d5e0008f32277
```

Now the transaction should be published to the network. If you have
an active block subscription (which you can do with `drk subscribe`),
then when the transaction is confirmed, your wallet should have your
new tokens listed when you run:

```shell
$ ./drk wallet --balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+----------------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 19.98451279
 {TOKEN1}                                     | ANON    | 42.69
 {TOKEN2}                                     | DAWN    | 20
```
