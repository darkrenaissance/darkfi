DarkFi x Monero Merge Mining using p2pool and xmrig
===================================================

**DISCLAIMER: This is a work in progress and not available on the
current deployed testnet as of 22-May-2025.**

This document provides a way to set up a Monero testnet that is
able to merge-mine DarkFi using p2pool.

## Build binaries from source

We can build Monero, p2pool, and xmrig from their respective source
code repositories.

For Monero, we'll take the latest release tag:

```
$ git clone --recursive https://github.com/monero-project/monero
$ git checkout v0.18.4.0
$ make -j$(nproc)
```

Same for p2pool:

```
$ git clone --recursive https://github.com/SChernykh/p2pool
$ git checkout v4.6
$ mkdir build && cd build && cmake .. && make -j$(nproc)
```

And xmrig:

```
$ git clone --recursive https://github.com/xmrig/xmrig
$ git checkout v6.22.2
$ mkdir build && cd build && cmake .. && make -j$(nproc)
```

Copy the resulting binaries into a working directory where we'll be
doing the network setup. We'll need `monerod`, `monero-wallet-cli`,
`p2pool`, and `xmrig`.


## Monero setup

We should first sync the Monero Testnet locally. We can simply do this
by starting up `monerod` and waiting for the sync to finish:

```
$ ./monerod --testnet --no-igd --data-dir bitmonero --log-level 0 --hide-my-port --add-peer 125.229.105.12:28081 --add-peer 37.187.74.171:28089 --fast-block-sync=1

2025-05-22 13:04:16.492 I Synced 3601/2754128 (0%, 2750527 left)
2025-05-22 13:04:27.315 I Synced 5801/2754128 (0%, 2748327 left)
...
2025-05-22 13:04:38.705 I Synced 8101/2754128 (0%, 2746027 left)
2025-05-22 13:04:44.676 I Synced 9301/2754128 (0%, 2744827 left)
2025-05-22 13:04:47.174 I Synced 9801/2754128 (0%, 2744327 left)
```

After the sync is finished, we will take the node offline and continue
our work locally. So quit the `monerod` node, and restart it offline
with fixed difficulty that will make our mining process faster:

```
$ ./monerod --testnet --no-igd --data-dir bitmonero --log-level 1 --hide-my-port --fixed-difficulty 20000 --disable-rpc-ban --offline --zmq-pub tcp://127.0.0.1:28083
```

Now we should also create a Monero wallet. Run `monero-wallet-cli` and
follow the wizard to create a wallet:

```
$ ./monero-wallet-cli --testnet --trusted-daemon

Generated new wallet: 9zMUzh73iWm5pXha95quaQjW1BnL5w2kBA8np1RqNsaSKoK7nA3ZPg1VPmtpHjhDV1WHd6sVyuePPGdaWiQqyQTcN6RuQA4
View key: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
**********************************************************************
Your wallet has been generated!
```

Now we have our Monero address `9zMUzh73iWm5pXha95quaQjW1BnL5w2kBA8np1RqNsaSKoK7nA3ZPg1VPmtpHjhDV1WHd6sVyuePPGdaWiQqyQTcN6RuQA4`
that we can use with p2pool to receive mining rewards.


## p2pool setup (without merge-mining)

First we'll start p2pool without merge-mining to make sure everything
works in order. After we get xmrig set up, we'll restart p2pool with
merge-mining enabled.

p2pool connects to `monerod`'s JSONRPC and ZMQ Pub ports in order to
retrieve necessary mining data. It also provides a Stratum mining
endpoint that `xmrig` is able to connect to in order to receive
mining jobs and actually mine the proposed blocks.

We can start p2pool with the following command:

```
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet 9zMUzh73iWm5pXha95quaQjW1BnL5w2kBA8np1RqNsaSKoK7nA3ZPg1VPmtpHjhDV1WHd6sVyuePPGdaWiQqyQTcN6RuQA4 --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd
```

Once started, it should connect to `monerod` and retrieve the latest
blockchain info. Now we can proceed with `xmrig` to try and mine some
blocks.

## xmrig setup

xmrig is pretty simple. Just start it with a chosen number of threads
and point it to p2pool's Stratum port. `-u x+1 20000` is defined by the
`--fixed-difficulty` setting we started `monerod` with. `-t 1` is the
number of CPU threads to use for mining. With a low difficulty, one
thread should be enough.

```
$ ./xmrig -u x+1 20000 -o 127.0.0.1:3333 -t 1
```

Now we should see blocks being mined in p2pool and submitted to our
Monero testnet. To stop mining you can `^C` xmrig anytime to quit it
or press `p` to pause mining.

## p2pool setup (with merge-mining)

Now that everything is in order, we can use p2pool with merge-mining
enabled in order to merge mine DarkFi. For receiving mining rewards
on DarkFi, we'll need a DarkFi wallet address. In this example we'll
use the following:

```
GCP5e1aGWPTy347WzAbn4uA5yT8mzQ25GV3gpp3MBihS
```

We will also need `darkfid` running. Make sure you enable the RPC
endpoint that will be used by p2pool in darkfid's config:

```
[network_config."testnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:8341"
```

Then start `darkfid` as usual.

Stop p2pool if it's running, and re-run it with the merge-mining
parameters appended:

```
$ ./p2pool --host 127.0.0.1 --rpc-port 28081 --zmq-port 28083 --wallet 9zMUzh73iWm5pXha95quaQjW1BnL5w2kBA8np1RqNsaSKoK7nA3ZPg1VPmtpHjhDV1WHd6sVyuePPGdaWiQqyQTcN6RuQA4 --stratum 127.0.0.1:3333 --data-dir ./p2pool-data --no-igd --merge-mine 127.0.0.1:8341 GCP5e1aGWPTy347WzAbn4uA5yT8mzQ25GV3gpp3MBihS
```

Now p2pool should communicate with both `monerod` and `darkfid` in
order to pick up Monero blocktemplates and inject them with DarkFi
data necessary for merge-mining verification on the DarkFi side.
Re-run xmrig and now we should be mining blocks again. Once blocks
are found, they will be submitted to both `monerod` and `darkfid` and
`darkfid` should verify them and release block rewards to the address
provided to p2pool's merge-mine parameters.

Happy mining!
