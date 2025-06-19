Compiling and Running a Node
=========================

Please read the whole document first before executing commands, to
understand all the steps required and how each component operates.
Unless instructed otherwise, each daemon runs on its own shell, so don't
stop a running one to start another.

Each command to execute will be inside a codeblock, on its first line,
marked by the user `$` symbol, followed by the expected output. For
longer command outputs, some lines will be emmited to keep the guide
simple.

We also strongly suggest to first execute next guide steps on a
[local environment](#local-deployment) to become familiar with
each command, before broadcasting transactions to the actual network.

## Overview

This tutorial will cover the three DarkFi blockchain components and
their current features. The components covered are:

* `darkfid` is the DarkFi fullnode. It validates blockchain transactions
and stays connected to the p2p network. 
* `drk` is a CLI wallet. It provides an interface to smart contracts such
as Money and DAO, manages our keys and coins, and scans the blockchain
to update our balances.
* `minerd` is the DarkFi mining daemon. `darkfid` connects to it over
RPC, and triggers commands to mine blocks.

The config files for `darkfid` and `drk` are sectioned into three parts,
each marked `[network_config]`. The sections look like this:

* `[network_config."testnet"]`
* `[network_config."mainnet"]`
* `[network_config."localnet"]`

At the top of the `darkfid` and `drk` config file, we can modify the
network being used by changing the following line:

```toml
# Blockchain network to use
network = "testnet"
```

This enables us to configure `darkfid` and `drk` for different contexts,
namely mainnet, testnet and localnet. Mainnet is not active yet. Localnet
can be setup by following the instructions [here](#local-deployment). The
rest of this tutorial assumes we are setting up a testnet node.

## Compiling

Since this is still an early phase, we will not be installing any of
the software system-wide. Instead, we'll be running all the commands
from the git repository, so we're able to easily pull any necessary
updates.

Refer to the main [DarkFi](../index.html#build) page for instructions
on how to install Rust and necessary deps. Skip last step of the build
process, as you don't need to compile all binaries of the project.

Once you have the repository in place, and everything is installed, we
can compile the `darkfid` node and the `drk` wallet CLI:

```shell
$ make darkfid drk

...
make -C bin/darkfid \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/darkfi/bin/darkfid'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package darkfid
...
   Compiling darkfid v0.5.0 (/home/anon/darkfi/bin/darkfid)
    Finished `release` profile [optimized] target(s) in 4m 19s
cp -f ../../target/x86_64-unknown-linux-gnu/release/darkfid darkfid
cp -f ../../target/x86_64-unknown-linux-gnu/release/darkfid ../../darkfid
make[1]: Leaving directory '/home/anon/darkfi/bin/darkfid'
make -C bin/drk \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/darkfi/bin/drk'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package drk
...
   Compiling drk v0.5.0 (/home/anon/darkfi/bin/drk)
    Finished `release` profile [optimized] target(s) in 2m 16s
cp -f ../../target/x86_64-unknown-linux-gnu/release/drk drk
cp -f ../../target/x86_64-unknown-linux-gnu/release/drk ../../drk
make[1]: Leaving directory '/home/anon/darkfi/bin/drk'
```

This process will now compile the node and the wallet CLI tool.
When finished, we can begin using the network. Run `darkfid` and `drk`
once so their config files are spawned on your system. These config files
will be used to `darkfid` and `drk`.

Please note that the exact paths may differ depending on your local setup.

```shell
$ ./darkfid

Config file created in "~/.config/darkfi/darkfid_config.toml". Please review it and try again.
```

```shell
$ ./drk wallet address

Config file created in "~/.config/darkfi/drk_config.toml". Please review it and try again.
```

## Running

### Using Tor

DarkFi supports Tor for network-level anonymity. To use the testnet over
Tor, you'll need to make some modifications to the `darkfid` config
file.

For detailed instructions and configuration options on how to do this,
follow the [Tor Guide](../misc/nodes/tor-guide.md#configure-network-settings).
The guide is using `darkirc` port `25552` for seeds and `25551` for
`torrc` configuration, so in your actual configuration replace them
with `darkfid` ones, where seeds use port `8343` and `torrc` should
use port `8342`.

### Wallet initialization

Now it's time to initialize your wallet. For this we use `drk`, a separate
wallet CLI which is created to interface with the smart contract used
for payments and swaps.

First, you need to change the password in the `drk` config. Open
your config file in a text editor (the default path is
`~/.config/darkfi/drk_config.toml`). Look for the section marked
`[network_config."testnet"]` and change this line:

```toml
# Password for the wallet database
wallet_pass = "changeme"
```

Once you've changed the default password for your testnet wallet, we
can proceed with the wallet initialization. We simply have to
initialize a wallet, and create a keypair. The wallet address shown in
the outputs is examplatory and will different from the one you get.

```shell
$ ./drk wallet initialize

Initializing Money Merkle tree
Successfully initialized Merkle tree for the Money contract
Generating alias DRK for Token: 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb
Initializing DAO Merkle trees
Successfully initialized Merkle trees for the DAO contract
```

```shell
$ ./drk wallet keygen

Generating a new keypair
New address:
CbaqFqGTgn86Zh9AjUeMw3DJyVCshaPSPFtmj6Cyd5yU
```

```shell
$ ./drk wallet default-address 1
```

The second command will print out your new DarkFi address where you
can receive payments. Take note of it. Alternatively, you can always
retrieve your default address using:

```shell
$ ./drk wallet address

CbaqFqGTgn86Zh9AjUeMw3DJyVCshaPSPFtmj6Cyd5yU
```

### Miner

It's not necessary for broadcasting transactions or proceeding with the
rest of the tutorial (`darkfid` and `drk` handle this), but if you want
to help secure the network, you can participate in the mining process
by running the native `minerd` mining daemon.

To mine on DarkFI we need to expose the `minerd` RPC to the `darkfid`
full node, which will initiate the mining process. We'll also need to
add a recipient to `darkfid` that specifies where the mining rewards
will be minted to. 

First, compile it:

```shell
$ make minerd

...
make -C bin/minerd \
        PREFIX="/home/anon/.cargo" \
        CARGO="cargo" \
        RUST_TARGET="x86_64-unknown-linux-gnu" \
        RUSTFLAGS=""
make[1]: Entering directory '/home/anon/darkfi/bin/minerd'
RUSTFLAGS="" cargo build --target=x86_64-unknown-linux-gnu --release --package minerd
...
  Compiling minerd v0.5.0 (/home/anon/darkfi/bin/minerd)
    Finished `release` profile [optimized] target(s) in 1m 25s
cp -f ../../target/x86_64-unknown-linux-gnu/release/minerd minerd
cp -f ../../target/x86_64-unknown-linux-gnu/release/minerd ../../minerd
make[1]: Leaving directory '/home/anon/darkfi/bin/minerd'
```

This process will now compile the mining daemon. When finished, run
`minerd` once so that it spawns its config file on your system. This
config file is used to configure `minerd`. You can define how many
threads will be used for mining. RandomX can use up to 2080 MiB per
thread so configure it to not consume all your system available memory.

```shell
$ ./minerd

Config file created in "~/.config/darkfi/minerd_config.toml". Please review it and try again.
```

Once that's in place, you can run it again and `minerd` will start,
waiting for requests to mine blocks.

```shell
$ ./minerd

14:20:06 [INFO] Starting DarkFi Mining Daemon...
14:20:06 [INFO] Initializing a new mining daemon...
14:20:06 [INFO] Mining daemon initialized successfully!
14:20:06 [INFO] Starting mining daemon...
14:20:06 [INFO] Mining daemon started successfully!
```

You now have to expose `minerd` RPC to `darkfid`, and configure it
to use your wallet address as the rewards recipient, when submitting
blocks to `minerd` to mine.

Open your `darkfid` config file with a text editor (the default path
is `~/.config/darkfi/darkfid_config.toml`). Find the `recipient` and
`minerd_endpoint` options under `[network_config."testnet"]`, and
uncomment them by removing the `#` character at the start of line,
like this:

```toml
# Put your `minerd` endpoint here (default for testnet is in this example)
minerd_endpoint = "tcp://127.0.0.1:28467"
# Put the address from `drk wallet address` here
recipient = "YOUR_WALLET_ADDRESS_HERE"
```

Now ensure that `minerd_endpoint` is set to the same value as the
`rpc_listen` address in your `minerd` config (the default path
is `~/.config/darkfi/minerd_config.toml`). Finally, replace the
`YOUR_WALLET_ADDRESS_HERE` string with your `drk` wallet address that
you can retrieve as follows:

```shell
$ ./drk wallet address

CbaqFqGTgn86Zh9AjUeMw3DJyVCshaPSPFtmj6Cyd5yU
```

Note: when modifying the `darkfid` config file to use with the
testnet, be sure to change the values under the section marked
`[network_config."testnet"]` (not localnet or mainnet!).

### Darkfid

Now that `darkfid` configuration is in place, you can run it again and
`darkfid` will start, create the necessary keys for validation of blocks
and transactions, and begin syncing the blockchain. 

```shell
$ ./darkfid

14:23:23 [INFO] Initializing DarkFi node...
14:23:23 [INFO] Node is configured to run with fixed PoW difficulty: 1
14:23:23 [INFO] Initializing a Darkfi daemon...
14:23:23 [INFO] Initializing Validator
14:23:23 [INFO] Initializing Blockchain
14:23:23 [INFO] Deploying native WASM contracts
14:23:23 [INFO] Deploying Money Contract with ContractID BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o
14:23:29 [INFO] Successfully deployed Money Contract
14:23:29 [INFO] Deploying DAO Contract with ContractID Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj
...
```

As its syncing, you'll see periodic messages like this:

```shell
...
[INFO] Blocks received: 4020/4763
...
```

This will give you an indication of the current progress. Keep it running,
and you should see a `Blockchain synced!` message after some time.

If you're running `minerd`, you should see a notification from the
`minerd` terminal like this:

```shell
...
[INFO] [RPC] Server accepted conn from tcp://127.0.0.1:44974/
...
```

This means that `darkfid` and `minerd` are connected over RPC and
`minerd` can start mining. You will see log messages like these:

```shell
...
14:23:56 [INFO] Mining block 4abc760a1f1c7198837e91c24d8e045e9fc9cb9fdf3a5fd45e184c25b03b0b51 for target:
115792089237316195423570985008687907853269984665640564039457584007913129639935
14:24:04 [INFO] Mined block 4abc760a1f1c7198837e91c24d8e045e9fc9cb9fdf3a5fd45e184c25b03b0b51 with nonce: 2
14:24:06 [INFO] Received request to mine block 17e7428ecb3d911477f8452170d0822c831c6912027abb120e4b4c4cf01d6020 for target:
115792089237316195423570985008687907853269984665640564039457584007913129639935
14:24:06 [INFO] Checking if a pending request is being processed...
...
```

When `darkfid` and `minerd` are correctly connected and you get an
error like this:

```shell
...
[ERROR] minerd::rpc: Failed mining block f6b4a0f0c8f90905da271ec0add2e856939ef3b0d6cd5b28964d9c2b6d0a0fa9 with error:
Miner task stopped
...
```

That's expected behavior. It means your setup is correct and you are
mining blocks. `Failed mining block` happens when a new block was
received by `darkfid`, extending the current best fork, so it sends an
interuption message to `minerd` to stop mining the current block and
start mining the next height one.

Otherwise, you'll see a notification like this:

```shell
...
[INFO] Mined block b6c7bd3545daa81d0e2e56ee780363beef6eb5b54579f54dca0cdd2a59989b76 with nonce: 266292
...
```

Which means the current height block has been mined succesfully by
`minerd` and propagated to `darkfid` so it can broadcast it to the
network.

### Wallet sync

In order to receive incoming coins, you'll need to use the `drk`
tool to subscribe on `darkfid` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In another terminal,
you can run the following commands to first scan the blockchain,
and then to subscribe to new blocks:

```shell
$ ./drk scan

...
Requested to scan from block number: 0
Last confirmed block reported by darkfid: 1 - da4455f461df6833a68b659d1770f58e44b6bc4abdd934cb22d084c24333255f
Requesting block 0...
Block 0 received! Scanning block...
=======================================
Header {
        Hash: b967812a860e8bf43deb03dd4f7cf69258f7719ddb7f2183d4e4fa3559b9f39d
        Version: 1
        Previous: 86bbac430a4b3a182f125b37a486e9c486bbfa34d84ef4a66b4a23e5f0c625b1
        Height: 0
        Timestamp: 2025-05-12T13:00:24
        Nonce: 0
        Transactions Root: 0x081361c364feba0d28a418e2e20c216ce442d5127036e3491ceaf1996fdb3c3b
        State Root: afc1694dd6b290d8b92c33d3fc746707da9bed857eb9e90f11683d2e243b8047
        Proof of Work data: Darkfi
}
=======================================
[scan_block] Iterating over 1 transactions
[scan_block] Processing transaction: 91525ff00a3755a8df93c626b59f6e36cf021d85ebccecdedc38f3f1890a15fc
Requesting block 1...
Block 1 received! Scanning block...
...
Requested to scan from block number: 2
Last confirmed block reported by darkfid: 1 - da4455f461df6833a68b659d1770f58e44b6bc4abdd934cb22d084c24333255f
Finished scanning blockchain
```

```shell
$ ./drk subscribe

Subscribing to receive notifications of incoming blocks
Detached subscription to background
All is good. Waiting for block notifications...
```

Now you can leave the subscriber running. In case you stop it, just
run `drk scan` again until the chain is fully scanned, and then you
should be able to subscribe again.

## Local Deployment

For local (non-testnet) development we recommend running master, and
use the existing `contrib/localnet/darkfid-single-node` folder, which
provides the corresponding configurations to operate. Some outputs are
emitted since they are identical to previous steps.

First, compile `darkfid` node, `minerd` mining daemon and the `drk`
wallet CLI:

```shell
$ make darkfid minerd drk
```

Enter the localnet folder, and initialize a wallet:

```shell
$ cd contrib/localnet/darkfid-single-node/
$ ./init-wallet.sh
```

Then start `darkfid` and wait until its initialized:

```shell
$ ./tmux_sessions.sh
```

After some blocks have been generated we
will see some `DRK` in our test wallet.
On a different shell(or tmux pane in the session),
navigate to `contrib/localnet/darkfid-single-node`
folder again and check wallet balance

```shell
$ ./wallet-balance.sh

 Token ID                                     | Aliases | Balance
----------------------------------------------+---------+---------
 241vANigf1Cy3ytjM1KHXiVECxgxdK4yApddL8KcLssb | DRK     | 20
```

Don't forget that when using this local node, all operations
should be executed inside the `contrib/localnet/darkfid-single-node`
folder, and `./drk` command to be replaced by `../../../drk -c drk.toml`

## Advanced Usage

To run a node in full debug mode:

```shell
$ LOG_TARGETS='!sled,!rustls,!net' ./darkfid -vv | tee /tmp/darkfid.log
```

The `sled` and `net` targets are very noisy and slow down the node so
we disable those.

We can now view the log, and grep through it.

```shell
$ tail -n +0 -f /tmp/darkfid.log | grep -a --line-buffered -v DEBUG
```
