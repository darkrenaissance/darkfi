# DarkFi

![Build Status](https://github.com/darkrenaissance/darkfi/actions/workflows/rust-build.yml/badge.svg)

## Building

```
$ make
```

## Configuration

The daemons can be configured using TOML files. Find examples in
this repo: [example/config](example/config) and copy them over to
`~/.config/darkfi`. The defaults should be safe to use for demo
purposes.

The cashier daemon must have enough testnet coins to cover fees.
Testnet SOL can be acquired by calling `solana airdrop` on devnet.
Testnet BTC can be acquired from a faucet.

## Usage

For demo purposes we have to run three daemons. It is best practice to
run them in three different terminals, and use the fourth to interact
with them using the provided `drk` command line tool.

1. Run `gatewayd`:

```
$ ./target/release/gatewayd -v
```

2. Run `cashierd`:

```
$ ./target/release/cashierd -v
```

3. Run `darkfid`:

```
$ ./target/release/darkfid -v
```

Now using the command line interface to the `darkfid` daemon, we can
make use of the system:

```
$ ./target/release/drk help
```

## Go Dark

Let's liberate people from the claws of big tech and create the
democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
