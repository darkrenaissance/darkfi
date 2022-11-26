Some notes
==========

Start the nodes

```
$ ../../../faucetd -c ./faucetd.config -v
$ ../../../darkfid -c ./darkfid.config -v
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

Wait and look at the subscription. It gets the block correctly, but
it seems to see the transaction again in new incoming blocks. What's
happening?
