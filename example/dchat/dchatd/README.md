# Dchat

A simple chat program to document DarkFi net
code. Tutorial can be found in the [DarkFi
book](https://darkrenaissance.github.io/darkfi/learn/writing-a-p2p-app.html).

## Step 1: Spin a seed node

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
must match the seed specified in Alice and Bob's settings (see: main.rs).

```toml
[network."dchat"]
port = 50515
localnet = true
```

Now run `lilith`:

```bash
./lilith
```

## Step 2: Using dchat

```shell
make BINS="dchat"
```

Run dchat as an inbound node:

```shell
./dchat a
```

Run dchat as an outbound node:

```shell
./dchat b
```

## Logging

Dchat creates a logging file for the inbound node at /tmp/alice.log.
Logging for the outbound node is found at /tmp/bob.log.

Tail them like so:

```shell
tail -f /tmp/alice.log
```

Or use multitail for colored output:

```shell
multitail -c /tmp/alice.log
```

# Example Localnet

```
./example/seed.sh
./example/node1.sh
./example/node2.sh
```
