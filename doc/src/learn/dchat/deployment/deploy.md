# Deploying a local network

Get ready to spin up a bunch of different terminals. We are going to
run 3 nodes: Alice and Bob and our seed node. To run the seed node,
go to the `lilith` directory and spawn a new config file by running it once:

```bash
cd darkfi
make BINS=lilith
./lilith
```

You should see the following output:

```
Config file created in '"/home/USER/.config/darkfi/lilith_config.toml"'. Please review it and try again.
 ```

Add dchat to the config as follows, keeping in mind that the port number must match the seed we specified
earlier in Alice and Bob's settings.

```toml
[network."dchat"]
port = 50515
localnet = true
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

Next we'll head back to `dchat` and run Alice. 

```bash
cargo run a
```

You can `cat` or `tail` the log file created in /tmp/. I recommend using
multitail for colored debug output, like so:

```bash
multitail -c /tmp/alice.log
```

Check out that debug output! Keep an eye out for this line:

```
[INFO] Connected seed #0 [tcp://127.0.0.1:55555]
```

That shows Alice has connected to the seed node. Here's some more
interesting output:

```
[DEBUG] (1) net: Attached ProtocolPing
[DEBUG] (1) net: Attached ProtocolSeed
[DEBUG] (1) net: ProtocolVersion::run() [START]
[DEBUG] (1) net: ProtocolVersion::exchange_versions() [START]
```

This raises an interesting question- what are these protocols? We'll deal
with that in more detail in a subsequent section. For now it's worth
noting that every node on the p2p network performs several protocols
when it connects to another node.

Keep Alice and the seed node running. Now let's run Bob.

```bash
cargo run b
```

And track his debug output:

```bash
multitail -c /tmp/bob.log
```

Success! All going well, Alice and Bob are now connected to each
other. We should be able to watch `ping` and `pong` messages being sent
across by tracking their debug output.

We have created a local deployment of the p2p network.

