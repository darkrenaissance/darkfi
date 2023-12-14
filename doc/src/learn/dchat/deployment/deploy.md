# Deploying a local network

Let's start by running 2 nodes: a `dchatd` full node and our seed node. To
run the seed node, go to the `lilith` directory and spawn a new config
file by running it once:

```bash
cd darkfi
make BINS=lilith
./lilith
```

You should see the following output:

```
Config file created in '"/home/USER/.config/darkfi/lilith_config.toml"'. Please review it and try again.
 ```

Add dchat to the config as follows, keeping in mind that the port number
must match the seed we specified earlier in the TOML.

```toml
[network."dchat"]
accept_addrs = ["tcp://127.0.0.1:50515"]
seeds = []
peers = []
version = "0.4.2"
```

Now run `lilith`:

```bash
./lilith
```

Here's what the debug output should look like:

```
[INFO] Found configuration for network: dchat
[INFO] Starting seed network node for dchat at: tcp://127.0.0.1:50515
[WARN] Skipping seed sync process since no seeds are configured.
[INFO] Starting inbound session on tcp://127.0.0.1:50515
[INFO] Starting 0 outbound connection slots.
```

Next we'll run `dchatd` outbound and inbound node using the default
settings we specified earlier.

```bash
./dchatd
```

```
[INFO] Connected seed #0 [tcp://127.0.0.1:50515]
```

That shows we have connected connected to the seed node. Here's some
more interesting output:

```
[DEBUG] (1) net: Attached ProtocolPing
[DEBUG] (1) net: Attached ProtocolSeed
[DEBUG] (1) net: ProtocolVersion::run() [START]
[DEBUG] (1) net: ProtocolVersion::exchange_versions() [START]
```

We have created a local deployment of the p2p network. 

This raises an interesting question- what are these protocols? We'll
deal with that in more detail soon. For now it's worth noting that every
node on the p2p network performs several protocols when it connects to
another node.

