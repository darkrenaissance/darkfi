# Settings

To create an inbound and outbound node, we
will need to configure them using `net` type called
[Settings](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/settings.rs).
This type consists of several settings that allow you to configure nodes
in different ways.

To do this, we'll create a default `dchatd_config.toml` file at the
place specified in `CONFIG_FILE_CONTENTS`.

```toml
# dchatd toml
[net]
## Outbound connect slots. Required for outbound nodes.
outbound_connections = 5

# Whitelisted network transports for outbound connections
active_profiles = ["tcp"]

[net.profiles."tcp"]
## P2P accept addresses Required for inbound nodes.
inbound=["tcp://127.0.0.1:51554"]

## P2P external addresses. Required for inbound nodes.
external_addr=["tcp://127.0.0.1:51554"]

## Seed nodes to connect to. Required for inbound and outbound nodes.
seeds=["tcp://127.0.0.1:50515"]
```

Inbound nodes specify an external address and an inbound address: this is
where it will receive connections. Outbound nodes specify the number of
outbound connection slots, which is the number of outbound connections
the node will try to make, and seed addresses from which it can receive
information about other nodes it can connect to. If all of these settings
are enabled, the node is both inbound and outbound, i.e. a full node.

Next, we add `SettingsOpt` to our `Args` struct. This will allow us to
read the fields specified in TOML as the darkfi `net` type, `Settings`.

```rust
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dchat", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    /// P2P network settings
    #[structopt(flatten)]
    net: SettingsOpt,
}
```
