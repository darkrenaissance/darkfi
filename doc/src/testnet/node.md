Compiling and Running a Node
=========================

Since this is still an early phase, we will not be installing any of
the software system-wide. Instead, we'll be running all the commands
from the git repository, so we're able to easily pull any necessary
updates.

Please read the whole document first before executing commands, to
understand all the steps required and how each component operates.
Unless instructed otherwise, each daemon exists/runs on its own
shell, so don't stop a running one to start another.
We also strongly suggest to first execute next guide steps on a
[local environment](#local-deployment) to become familiar with
each command, before broadcasting transactions to the actual network.

## Compiling

Refer to the main [DarkFi](../index.html) page for instructions on how
to install Rust and necessary deps.

Once you have the repository in place, and everything is installed, we
can compile the `darkfid` node and the `drk` wallet CLI:

```
$ make darkfid drk
```

This process will now compile the node and the wallet CLI tool.
When finished, we can begin using the network. Run `darkfid` and `drk`
once so their config files are spawned on your system. This config files
will be used by `darkfid` and `drk` in order to configure themselves.
The defaults are already preset for using the testnet network.

```
$ ./darkfid
Config file created in "~/.config/darkfi/darkfid_config.toml". Please review it and try again.
$ ./drk wallet --address
Config file created in "~/.config/darkfi/drk_config.toml". Please review it and try again.
```

## Running

### Wallet initialization

Now it's time to initialize your wallet. For this we use a separate
wallet CLI which is created to interface with the smart contract used
for payments and swaps.

We simply have to initialize a wallet, and create a keypair:

```
$ ./drk wallet --initialize
$ ./drk wallet --keygen
$ ./drk wallet --default-address 1
```

The second command will print out your new DarkFi address where you
can receive payments. Take note of it. Alternatively, you can always
retrieve your default address using:

```
$ ./drk wallet --address
```

### Miner

If you want to help secure the network, you can participate in the mining
process, by running the native `minerd` mining daemon. First, compile it:

```
$ make minerd
```

This process will now compile the mining daemon. When finished, run
`minerd` once so that it spawns its config file on your system. This
config file will be used by `minerd` in order to configure itself.

```
$ ./minerd
Config file created in "~/.config/darkfi/minerd_config.toml". Please review it and try again.
```

Once that's in place, you can run it again and `minerd` will start,
waiting for requests to mine blocks.

```
$ ./minerd
```

You now have to configure `darkfid` to use your wallet address as the
rewards recipient, when submitting blocks to `minerd` to mine. Open
your config file with your editor of choice (default path is
`~/.config/darkfi/darkfid_config.toml`) and find the `recipient` and
`minerd_endpoint` options under the network configuration you will operate
on (for testnet it is `[network_config."testnet"]`). Uncomment them by
removing the `#` character at the start of line, and replace the
`YOUR_WALLET_ADDRESS_HERE` string with your wallet address.

```
# Put your `minerd` endpoint here (default for testnet is in this example)
minerd_endpoint = "tcp://127.0.0.1:28467"
# Put the address from `drk wallet --address` here
recipient = "..."
```


### Darkfid

Now that `darkfid` configuration is in place, you can run it again and
`darkfid` will start, create the necessary keys for validation of blocks
and transactions, and begin syncing the blockchain. Keep it running,
and you should see a `Blockchain synced!` message after some time.

```
$ ./darkfid
```

### Wallet sync

In order to receive incoming coins, you'll need to use the `drk`
tool to subscribe on `darkfid` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In another terminal,
you can run the following commands to first scan the blockchain,
and then to subscribe to new blocks:

```
$ ./drk scan
$ ./drk subscribe
```

Now you can leave the subscriber running. In case you stop it, just
run `drk scan` again until the chain is fully scanned, and then you
should be able to subscribe again.

## Local Deployment

For development we recommend running master, and use the existing
`contrib/localnet/darkfid-single-node` folder, which provides
the corresponding configurations to operate.

First, compile `darkfid` node, `minerd` mining daemon and the `drk`
wallet CLI:

```
$ make darkfid minerd drk
```

Enter the localnet folder, and initialize a wallet:

```
$ cd contrib/localnet/darkfid-single-node/
$ ./init-wallet.sh
```

Then start `darkfid` and wait until its initialized:

```
$ ./tmux_sessions.sh
```

After some blocks have been generated we
will see some `DRK` in our test wallet.
On a different shell(or tmux pane in the session),
navigate to `contrib/localnet/darkfid-single-node`
folder again and check wallet balance

```
$ ./wallet-balance.sh
```

Don't forget that when using this local node, all operations
should be executed inside the `contrib/localnet/darkfid-single-node`
folder, and `./drk` command to be replaced by `../../../drk -c drk.toml`

## Advanced Usage

To run a node in full debug mode:

```
$ LOG_TARGETS='!sled,!rustls,!net' ./darkfid -vv | tee /tmp/darkfid.log
```

The `sled` and `net` targets are very noisy and slow down the node so
we disable those.

We can now view the log, and grep through it.

```
$ tail -n +0 -f /tmp/darkfid.log | grep -a --line-buffered -v DEBUG
```

