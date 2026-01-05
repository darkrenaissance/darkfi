Compiling and Running a Node
=========================

**DISCLAIMER: This is a work in progress and functionalities may not be
available on the current deployed testnet as of 22-May-2025.**

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

* `darkfid` is the DarkFi fullnode. It validates blockchain
transactions and stays connected to the p2p network.
* `drk` is a CLI wallet. It provides an interface to smart contracts
such as Money and DAO, manages our keys and coins, and scans the
blockchain to update our balances.
* `xmrig` is the mining daemon used in DarkFi. Connects to `darkfid`
over its `Stratum` RPC, and requests new block headers to mine.

The config files for `darkfid` and `drk` are sectioned into three
parts, each marked `[network_config]`. The sections look like this:

* `[network_config."testnet"]`
* `[network_config."mainnet"]`
* `[network_config."localnet"]`

At the top of each daemon config file, we can modify the network being
used by changing the following line:

```toml
# Blockchain network to use
network = "testnet"
```

This enables us to configure the daemons for different contexts, namely
mainnet, testnet and localnet. Mainnet is not active yet. Localnet can
be setup by following the instructions [here](#local-deployment). The
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
$ ./drk interactive

Config file created in "~/.config/darkfi/drk_config.toml". Please review it and try again.
```

## Running

### Using Tor

DarkFi supports Tor for network-level anonymity. To use the testnet over
Tor, you'll need to make some modifications to the `darkfid` config
file.

For detailed instructions and configuration options on how to do this,
follow the [Tor Guide](../misc/nodes/tor-guide.md#configure-network-settings).

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
{YOUR_DARKFI_WALLET_ADDRESS}
```

```shell
$ ./drk wallet default-address 1
```

The second command will print out your new DarkFi address where you
can receive payments. Take note of it. Alternatively, you can always
retrieve your default address using:

```shell
$ ./drk wallet address

{YOUR_DARKFI_WALLET_ADDRESS}
```

### Darkfid

Now that `darkfid` configuration is in place, you can run it again and
`darkfid` will start, create the necessary keys for validation of blocks
and transactions, and begin syncing the blockchain.

```shell
$ ./darkfid

[INFO] Initializing DarkFi node...
[INFO] Node is configured to run with fixed PoW difficulty: 1
[INFO] Initializing a Darkfi daemon...
[INFO] Initializing Validator
[INFO] Initializing Blockchain
[INFO] Deploying native WASM contracts
[INFO] Deploying Money Contract with ContractID BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o
[INFO] Successfully deployed Money Contract
[INFO] Deploying DAO Contract with ContractID Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj
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

### Miner

It's not necessary for broadcasting transactions or proceeding with the
rest of the tutorial (`darkfid` and `drk` handle this), but if you want
to help secure the network, you can participate in the mining process
by running an `xmrig` mining daemon. In this example we will build
`xmrig` from its respective source code repository. Make sure you are
not in the DarkFi repository folder as we are going to retrieve
external repos.

First, install its [dependencies][1], retrieve its repo and checkout
the latest release tag:

```shell
$ git clone --recursive https://github.com/xmrig/xmrig
$ cd xmrig
$ git checkout $(git describe --tags "$(git rev-list --tags --max-count=1)")
```

Now we can build it:

```shell
$ mkdir build
```

If you have already build `xmrig` above command will fail as folder
already exists, so just continue to next ones:

```shell
$ cd build
$ cmake ..
$ make -j$(nproc)
```

The binary now exists in the current directory. Make sure you enable
the `Stratum` RPC endpoint that will be used by `xmrig` in `darkfid`
config:

```toml
[network_config."testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:18347"
```

> Note:
>
> If you are not on the same network as the `darkfid` instance you
> are using, you must configure and use `tcp+tls` for the RPC
> endpoints, so your traffic is not plaintext, as it contains your
> wallet address used for the block rewards.

To mine on DarkFI we need to add a recipient to `xmrig` that specifies
where the mining rewards will be minted to. You now have to configure
`xmrig` to use your wallet address as the rewards recipient, when it
retrieves blocks from `darkfid` to mine. Make sure you have
[initialized](#wallet-initialization) your wallet and grab your default
address:

```shell
./drk wallet address

{YOUR_DARKFI_WALLET_ADDRESS}
```

Refer to [xmrig optimizations guide][2] to fully configure your system
for maximum mining performance. Start `darkfid` as usual and then start
`xmrig`, specifying retries setup, how many threads to mine and for
which wallet:

```shell
$ ./xmrig -u x+1 -r 1000 -R 20 -o 127.0.0.1:18347 -t {XMRIG_THREADS} -u {YOUR_DARKFI_WALLET_ADDRESS}
```

In `darkfid`, you should see a notification like this:

```shell
...
[INFO] [RPC-STRATUM] Got login from {YOUR_DARKFI_WALLET_ADDRESS} ({AGENT_INFO})
...
```

This means that `darkfid` and `xmr` are connected over the `Stratum`
RPC and `xmrig` can start mining. You will see log messages like these:

```shell
...
[INFO] Created new block template for wallet: address=DZns...rKkf, spend_hook=-, spend_hook=-, user_data=-
[INFO] [RPC-STRATUM] Created new mining job for client 091e...9d71: 26d6...8a3c
[INFO] [RPC-STRATUM] Got solution submission from client 091e...9d71 for job: 26d6...8a3c
[INFO] Appended proposal 6188...e623c
[INFO] Proposing new block to network
...
```

To stop mining you can `^C` `xmrig` anytime to quit it or press `p` to
pause mining.

### Wallet sync

From this point forward in the guide we will use `drk` in `interactive`
mode for all our wallet operations. In another terminal, run the
following command:

```shell
$ ./drk interactive

drk>
```

In order to receive incoming coins, you'll need to use the `drk`
tool to subscribe on `darkfid` so you can receive notifications for
incoming blocks. The blocks have to be scanned for transactions,
and to find coins that are intended for you. In the interactive shell,
run the following command to subscribe to new blocks:

```shell
drk> subscribe

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
Subscribing to receive notifications of incoming blocks
Detached subscription to background
All is good. Waiting for block notifications...
```

## Local Deployment

For local (non-testnet) development we recommend running master, and
use the existing `contrib/localnet/darkfid-single-node` folder, which
provides the corresponding configurations to operate. Some outputs are
emitted since they are identical to previous steps.

First, compile `darkfid` node and the `drk` wallet CLI:

```shell
$ make darkfid drk
```

> Note:
>
> Make sure you have properly setup `xmrig` [miner](#miner) as its
> required.

Enter the localnet folder, and initialize a wallet:

```shell
$ cd contrib/localnet/darkfid-single-node/
$ ./init-wallet.sh
```

Then configure your `xmrig` mining daemon path in `tmux_sessions.sh`
script, start the daemons and wait until `darkfid` is initialized:

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
folder, and `./drk` command to be replaced by
`../../../drk -c drk.toml`. All paths should be relative to this one.

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

[1]: https://xmrig.com/docs/miner/build
[2]: https://xmrig.com/docs/miner/randomx-optimization-guide
