# Accept addr

To start receiving `JSON-RPC` requests, we'll need to configure a
`JSON-RPC` accept address.

We'll add a `rpc_listen` address to our `Args` struct. It will look
like this:

```rust
{{#include ../../../../../example/dchat/dchatd/src/main.rs:args}}
```

This encodes a default `rpc_listen` address on the port `51054`. To be
able to modify the default, we can also add `rpc_listen` to the default
config at `../dchatd_config.toml` as follows:

```toml
# dchat toml

## RPC listen address. 
rpc_listen =["tcp://127.0.0.1:51054"]

[net]
## P2P accept addresses Required for inbound nodes.
inbound=["tcp://127.0.0.1:51554"]

## P2P external addresses. Required for inbound nodes.
external_addr=["tcp://127.0.0.1:51554"]

## Seed nodes to connect to. Required for inbound and outbound nodes.
seeds=["tcp://127.0.0.1:50515"]

## Outbound connect slots. Required for outbound nodes.
outbound_connections = 5
```

Regenerate the config by deleting the previous one, rebuilding and
rerunning `dchatd`. Now the `rpc_listen` address can be modified any
time by editing the config file.
