Compiling and Running a Node
=========================

Since this is still an early phase, we will not be installing any of
the software system-wide. Instead, we'll be running all the commands
from the git repository, so we're able to easily pull any necessary
updates.


## Compiling

Refer to the main
[README](https://github.com/darkrenaissance/darkfi/blob/master/README.md)
file for instructions on how to install Rust and necessary deps.

Once you have the repository in place, and everything is installed, we
can compile the `darkfid` node and the `drk` wallet CLI:

```
$ make darkfid drk
```

This process will now compile the node and the wallet CLI tool.
When finished, we can begin using the network. Run `darkfid` once so
that it spawns its config file on your system. This config file will
be used by `darkfid` in order to configure itself. The defaults are
already preset for using the testnet network.

```
$ ./darkfid
Config file created in "~/.config/darkfi/darkfid_config.toml". Please review it and try again.
```


## Running

Once that's in place, you can run it again and `darkfid` will start,
create necessary keys for validation of blocks and transactions, and
begin syncing the blockchain. Keep it running, and you should see a
`Blockchain is synced!` message after some time.

```
$ ./darkfid
```

Now it's time to initialize your wallet. For this we use a separate
wallet CLI which is created to interface with the smart contract used
for payments and swaps.

We simply have to initialize a wallet, and create a keypair:

```
$ ./drk wallet --initialize
$ ./drk wallet --keygen
```

The second command will print out your new DarkFi address where you
can receive payments. Take note of it. Alternatively, you can always
retrieve it using:

```
$ ./drk wallet --address
```

In order to receive incoming coins, you'll need to use the `drk`
tool to subscribe on `darkfid` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In another terminal,
you can run the following commands to first scan the blockchain,
and then to subscribe to new blocks:

```
$ ./drk scan
$ ./drk subscribe blocks
```

Now you can leave the subscriber running. In case you stop it, just
run `drk scan` again until the chain is fully scanned, and then you
should be able to subscribe again.

## Advanced Usage

To run a node in full debug mode:

```
LOG_TARGETS="\!sled,\!net" ./darkfid -v | tee /tmp/darkfid.log
```

The `sled` and `net` targets are very noisy and slow down the node so
we disable those.

We can now view the log, and grep through it.

```
tail -n +0 -f /tmp/darkfid.log | grep -a --line-buffered -v DEBUG
```

