# Deploying a local network

Get ready to spin up a bunch of different terminals. We are going to
run 3 nodes: Alice and Bob and our seed node. To run the seed node,
go to the lilith directory and run it by passing dchat as an argument:

```
cargo run -- --dchat
```

Here's what the debug output should look like:

```
[DEBUG] (1) net: P2p::start() [BEGIN]
[DEBUG] (1) net: SeedSession::start() [START]
[WARN] Skipping seed sync process since no seeds are configured.
[DEBUG] (1) net: P2p::start() [END]
[DEBUG] (1) net: P2p::run() [BEGIN]
[INFO] Starting inbound session on tcp://127.0.0.1:55555
[DEBUG] (1) net: tcp transport: listening on 127.0.0.1:55555
[INFO] Starting 0 outbound connection slots.
```

Next we'll run Alice.

```
cargo run a
```

You can `cat` or `tail` the log file created in /tmp/. I recommend using
multitail for colored debug output, like so:

`multitail -c /tmp/alice.log`

Check out that debug output! Keep an eye out for this line:

```
[INFO] Connected seed #0 [tcp://127.0.0.1:55555]
```

That shows Alice has connected to the seed node. Here's some more
interesting output:

```
08:54:59 [DEBUG] (1) net: Attached ProtocolPing
08:54:59 [DEBUG] (1) net: Attached ProtocolSeed
08:54:59 [DEBUG] (1) net: ProtocolVersion::run() [START]
08:54:59 [DEBUG] (1) net: ProtocolVersion::exchange_versions() [START]
```

This raises an interesting question- what are these protocols? We'll deal
with that in more detail in a subsequent section. For now it's worth
noting that every node on the p2p network performs several protocols
when it connects to another node.

Keep Alice and the seed node running. Now let's run Bob.

```
cargo run b
```

And track his debug output:

```
multitail -c /tmp/bob.log
```

Success! All going well, Alice and Bob are now connected to each
other. We should be able to watch Ping and Pong messages being sent
across by tracking their debug output.

We have created a local deployment of the p2p network.

