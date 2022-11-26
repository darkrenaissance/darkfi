Some notes
==========

Start the nodes

```
$ ../../../faucetd -c ./faucetd_config.toml -v
$ ../../../darkfid -c ./darkfid_config.toml -v
```

Wait for them to start up, then initialize wallet:

```
$ ../../../drk -e tcp://127.0.0.1:18340 wallet --initialize
$ ../../../drk -e tcp://127.0.0.1:18340 wallet --keygen
```

Subscribe to new blocks

```
$ ../../../drk -e tcp://127.0.0.1:18340 subscribe
```

Airdrop some coins

```
$ ../../../drk -e tcp://127.0.0.1:18340 airdrop 42.69 A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd
```

Wait and look at the subscription.

Check balance

```
$ ../../../drk -e tcp://127.0.0.1:18340 wallet --balance
```

Make a new key

```
$ ../../../drk -e tcp://127.0.0.1:18340 wallet --keygen
f00b4r
```

Create a tx to send some money to the new key

```
$ ../../../drk -e tcp://127.0.0.1:18340 transfer 11.11 A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd f00b4r > tx
```

Broadcast the tx

```
$ ../../../drk -e tcp://127.0.0.1:18340 broadcast < tx
```

Now watch darkfid, it gets the transaction, simulates it, and broadcasts
over p2p, but the consensus doesn't get it and so it doesn't get appended
to the mempool.
