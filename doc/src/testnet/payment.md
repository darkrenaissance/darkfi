# Payments

Using the tokens we minted, we can make payments to other addresses.
We will use a dummy recepient, but you can also test this with friends.
Let's try to send some `ANON` tokens to
`8sRwB7AwBTKEkyTW6oMyRoJWZhJwtqGTf7nyHwuJ74pj`:

```shell
$ ./drk transfer 2.69 ANON 8sRwB7AwBTKEkyTW6oMyRoJWZhJwtqGTf7nyHwuJ74pj > payment.tx
```

The above command will create a transfer transaction and place it into
the file called `payment.tx`. Then we can broadcast this transaction to
the network:

```shell
$ ./drk broadcast < payment.tx

[mark_tx_spend] Processing transaction: 47b4818caec22470427922f506d72788233001a79113907fd1a93b7756b07395
[mark_tx_spend] Found Money contract in call 0
[mark_tx_spend] Found Money contract in call 1
Broadcasting transaction...
Transaction ID: 47b4818caec22470427922f506d72788233001a79113907fd1a93b7756b07395
```

On success we'll see a transaction ID. Now again the same confirmation
process has to occur and `8sRwB7AwBTKEkyTW6oMyRoJWZhJwtqGTf7nyHwuJ74pj`
will receive the tokens you've sent.

![pablo-waiting1](img/pablo1.jpg)

We can see the spent coin in our wallet.

```shell
$ ./drk wallet coins

 Coin            | Spent | Token ID        | Aliases | Value                    | Spend Hook | User Data | Spent TX
-----------------+-------+-----------------+---------+--------------------------+------------+-----------+-----------------
 EGV6rS...pmmm6H | true  | 241vAN...KcLssb | DRK     | 2000000000 (20)          | -          | -         | fbbd7a...5f2b19
...
 47QnyR...1T7igm | true  | {TOKEN1}        | ANON    | 4269000000 (42.69)       | -          | -         | 47b481...b07395
 5UUJbH...trdQHY | false | {TOKEN1}        | ANON    | 4000000000 (40)          | -          | -         | -
 EEneNB...m6mxTC | false | 241vAN...KcLssb | DRK     | 1999442971 (19.97253683) | -          | -         | -
```

We have to wait until the next block to see our change balance reappear
in our wallet.

```shell
$ ./drk wallet balance

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+-------------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 19.97253683
 {TOKEN1}                                     | ANON    | 40
 {TOKEN2}                                     | DAWN    | 20
```
