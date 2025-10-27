# Atomic Swaps

In order to do an atomic swap with someone, you will first have to
agree on what tokens you wish to swap. For example purposes, let's say
you want to swap `40` `ANON` (which is the balance you should have left
over after doing the payment from the previous page) for your
counterparty's `20` `DAWN`. For this tutorial the counterparty is
yourself.

To protect your anonymity from the counterparty, the swap can only send
entire coins. To create a smaller coin denomination, send yourself the
amount you want to swap. Then check you have a spendable coin to swap
with. Note that the coin overview might look very different depending
on your activity:

```shell
drk> wallet coins

 Coin            | Spent | Token ID        | Aliases | Value                    | Spend Hook | User Data | Spent TX
-----------------+-------+-----------------+---------+--------------------------+------------+-----------+-----------------
 EGV6rS...pmmm6H | true  | 241vAN...KcLssb | DRK     | 2000000000 (20)          | -          | -         | fbbd7a...5f2b19
...
 H6Bc49...Zb6k8h | false | {TOKEN2}        | DAWN    | 2000000000 (20)          | -          | -         | -
 47QnyR...1T7igm | true  | {TOKEN1}        | ANON    | 4269000000 (42.69)       | -          | -         | 47b481...b07395
 5UUJbH...trdQHY | false | {TOKEN1}        | ANON    | 4000000000 (40)          | -          | -         | -
 EEneNB...m6mxTC | false | 241vAN...KcLssb | DRK     | 1999442971 (19.97253683) | -          | -         | -
```

You'll have to initiate the swap and build your half of the swap tx:

```shell
drk> otc init 40.0:20.0 ANON:DAWN > half_swap
```

Then you can send this `half_swap` file to your counterparty and they
can create the other half and sign it by running:

```shell
drk> otc join < half_swap > full_swap
```

They can now send it back to you. Finally, to make the swap transaction
valid, you need to sign it as well

```shell
drk> otc sign < full_swap > signed_swap
```

Now that the swap is signed, one of the parties (or a third one)
must attach the corresponding fee:

```shell
drk> attach-fee < signed_swap > full_swap_with_fee
```

Since a new call has been added to the transaction, both parties
must re-sign the full_swap_with_fee file, one by one.

Party A:

```shell
drk> otc sign < full_swap_with_fee > signed_full_swap_with_fee
```

Party B:

```shell
drk> otc sign < signed_full_swap_with_fee > swap.tx
```

Now the complete swap transaction can be broadcasted:

```shell
drk> broadcast < swap.tx

[mark_tx_spend] Processing transaction: d2a5e288e6ba44583ee12db9c7a0ed154c736d1aa841d70c7d3fa121c92dfc69
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: d2a5e288e6ba44583ee12db9c7a0ed154c736d1aa841d70c7d3fa121c92dfc69
```

On success, you should see a transaction ID. This transaction will now
also be in the mempool, so you should wait again until it's confirmed.

![pablo-waiting2](img/pablo2.jpg)

After a while you should see the change in balances in your wallet:

```shell
drk> wallet balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+-------------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 19.96835727
 {TOKEN1}                                     | ANON    | 40
 {TOKEN2}                                     | DAWN    | 20
```

Since in this example we did an atomic swap with ourself, the balances are
unchanged. We can confirm it actually happened successfully by checking
our coins:

```shell
drk> wallet coins

 Coin            | Spent | Token ID        | Aliases | Value                    | Spend Hook | User Data | Spent TX
-----------------+-------+-----------------+---------+--------------------------+------------+-----------+-----------------
 EGV6rS...pmmm6H | true  | 241vAN...KcLssb | DRK     | 2000000000 (20)          | -          | -         | fbbd7a...5f2b19
...
 EEneNB...m6mxTC | true  | 241vAN...KcLssb | DRK     | 1999442971 (19.97253683) | -          | -         | d2a5e2...2dfc69
 H6Bc49...Zb6k8h | true  | {TOKEN2}        | DAWN    | 2000000000 (20)          | -          | -         | d2a5e2...2dfc69
 5UUJbH...trdQHY | true  | {TOKEN1}        | ANON    | 4000000000 (40)          | -          | -         | d2a5e2...2dfc69
 4zwzZf...uMbVir | false | {TOKEN2}        | DAWN    | 2000000000 (20)          | -          | -         | -
 BrqQuk...FcwW6d | false | {TOKEN1}        | ANON    | 4000000000 (40)          | -          | -         | -
 EEneNB...m6mxTC | false | 241vAN...KcLssb | DRK     | 1999442971 (19.96834924) | -          | -         | -
```

Here you can see there are two entries for the tokens we used in the
swap: `40.00` `ANON` and `20.00` `DAWN` . The first entry shows the
`Spent` flag as `true` and the second entry shows the `Spent` flag as
`false`. This means the transaction was successful. Since we are
swapping with ourself, we successfully spent the coins in the first
half of the transaction, and re-minted to ourselves them in the second
half of the transaction.

If you're testing atomic swaps with a counterparty and you can see
their tokens, that means the swap was successful.  In case you still
see your old tokens, that could mean that the swap transaction has not
yet been confirmed.
