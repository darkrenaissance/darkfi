# Dchat

A demo chat program to document DarkFi net
code. Tutorial can be found in the [DarkFi
book](https://darkrenaissance.github.io/darkfi/learn/dchat/dchat.html).

## Usage

Spin up a seed node:

```shell
cd example/dchat
cargo run
```

Run dchat as an inbound node:

```shell
cargo run a
```

Run dchat as an outbound node:

```shell
cargo run b
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


