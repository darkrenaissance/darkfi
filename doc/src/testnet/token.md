# Native token

Now that you have your wallet set up, you will need some native `DRK`
in order to be able to perform transactions, since `DRK` is used to pay
the transaction fees. You can obtain `DRK` either by successfully
mining a block that gets confirmed or by asking for some from the
community on `darkirc`.

If the latter, don't forget to tell them to add the `--half-split` flag
when they create the transfer, so you get more than one coin to play
with. Once your friend has submitted a transaction to the network, it
should be in the consensus' mempool, waiting for inclusion in the next
block(s). Depending on your network configuration, confirmation of the
blocks could take some time. You'll have to wait for this to happen. If
your `drk` subscription is running, then after some time your new
balance should be in your wallet.

![pablo-waiting0](img/pablo0.jpg)

You can check your wallet balance using `drk`:

```shell
drk> wallet balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 20
```

# Creating tokens

On the DarkFi network, we can mint custom tokens with a given supply.
To do this, we first need to generate a mint authority keypair, and
derive a token ID from it.

> Note: The token data shown in the outputs (Token ID, Mint Authority,
> Token Blind) are placeholders for the data that will be created by
> you. For the rest of the guide, replace the placeholder token data
> with the data that you generate.

For this tutorial we will be minting two sets of tokens. For each token
we will create the mint authority keypair that sets the token ID and
authorizes you to mint the token supply.

We do this by simply executing the following command:

```shell
drk> token generate-mint

Successfully imported mint authority for token ID: {TOKEN1}
```

You can list your mint authorities with:

```shell
drk> token list

 Token ID | Aliases | Mint Authority          | Token Blind    | Frozen | Freeze Height
----------+---------+-------------------------+----------------+--------+---------------
 {TOKEN1} | -       | {TOKEN1_MINT_AUTHORITY} | {TOKEN1_BLIND} | false  | -

```

Now execute the command again to generate the mint authority for a
second set of tokens.

```shell
drk> token generate-mint

Successfully imported mint authority for token ID: {TOKEN2}
```

Verify you have two token mint authorities by running:

```shell
drk> token list

 Token ID | Aliases | Mint Authority          | Token Blind    | Frozen | Freeze Height
----------+---------+-------------------------+----------------+--------+---------------
 {TOKEN1} | -       | {TOKEN1_MINT_AUTHORITY} | {TOKEN1_BLIND} | false  | -
 {TOKEN2} | -       | {TOKEN2_MINT_AUTHORITY} | {TOKEN2_BLIND} | false  | -

```

## Aliases

To make our life easier, we can create token ID aliases, which we can
use instead of the token ID when performing transactions. Multiple
aliases per token ID are supported.

The native token alias `DRK` should already exist, and we can use that
to refer to `DRK` when executing transactions using it.

We can also list all our aliases using:

```shell
drk> alias show

 Alias | Token ID
-------+----------------------------------------------
 DRK   | 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb
```

> Note: these aliases are local to your machine. When exchanging with
> other users, always verify that your aliases' token IDs match.

Now let's create aliases for the two token IDs generated earlier:

```shell
drk> alias add ANON {TOKEN1}

Generating alias ANON for Token: {TOKEN1}
```

```shell
drk> alias add DAWN {TOKEN2}

Generating alias DAWN for Token: {TOKEN2}
```

```shell
drk> alias show

 Alias | Token ID
-------+---------------------------------------------
 ANON  | {TOKEN1}
 DAWN  | {TOKEN2}
 DRK   | 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLss
```

## Mint transaction

Now let's mint some tokens for ourselves. First grab your wallet
address, then create the token mint transaction,
and finally - broadcast it:

```shell
drk> wallet address

{YOUR_ADDRESS}
```

By default the transaction will be printed in the terminal. Interactive
mode supports `UNIX` style pipes and exporting/importing to/from files.
We can either export a transaction to a file by appending
`> {tx_file_name}.tx`, or broadcast it right away by appending
`| broadcast`. We will broadcast all transactions in the guide, for
simplicity.

```shell
drk> token mint ANON 42.69 {YOUR_ADDRESS} | broadcast

[mark_tx_spend] Processing transaction: e9ded45928f2e2dbcb4f8365653220a8e2346987dd8b75fe1ffdc401ce0362c2
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 2
Broadcasting transaction...
Transaction ID: e9ded45928f2e2dbcb4f8365653220a8e2346987dd8b75fe1ffdc401ce0362c2
```

Now the transaction should be published to the network, waiting to be
included in a block. After the transaction is confirmed, perform the
next one:

```shell
drk> token mint DAWN 20.0 {YOUR_ADDRESS} | broadcast

[mark_tx_spend] Processing transaction: e404241902ba0a8825cf199b3083bff81cd518ca30928ca1267d5e0008f32277
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
[mark_tx_spend] Found Money contract in call 2
Broadcasting transaction...
Transaction ID: e404241902ba0a8825cf199b3083bff81cd518ca30928ca1267d5e0008f32277
```

The transaction is now published to the network. When the transaction
is confirmed, your wallet should have your new tokens listed when you
run:

```shell
drk> wallet balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+-------------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 19.98451279
 {TOKEN1}                                     | ANON    | 42.69
 {TOKEN2}                                     | DAWN    | 20
```

## Freeze transaction

We can lock a token's supply and disallow further mints by executing:


```shell
drk> token freeze DAWN | broadcast

[mark_tx_spend] Processing transaction: 138274448ac3af26f253e0a40d0964dc125b99b3c826ba321bcb989cabfb6df6
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 138274448ac3af26f253e0a40d0964dc125b99b3c826ba321bcb989cabfb6df6
```

After the transaction has been confirmed, we will see the token freeze
flag set to `true`, along with the block height it was frozen on:

```shell
drk> token list

 Token ID | Aliases | Mint Authority          | Token Blind    | Frozen | Freeze Height
----------+---------+-------------------------+----------------+--------+---------------
 {TOKEN1} | ANON    | {TOKEN1_MINT_AUTHORITY} | {TOKEN1_BLIND} | false  | -
 {TOKEN2} | DAWN    | {TOKEN2_MINT_AUTHORITY} | {TOKEN2_BLIND} | true   | 4

```
