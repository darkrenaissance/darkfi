seedd
==========

A tool to deploy multiple P2P network seed nodes for darkfi applications, in a single daemon.

## Usage

```
seedd 0.3.0
Defines the network specific settings

USAGE:
    seedd [FLAGS] [OPTIONS]

FLAGS:
        --darkfid    Darkfid activation flag
    -h, --help       Prints help information
        --ircd       Ircd activation flag
        --taud       Taud activation flag
    -V, --version    Prints version information
    -v               Increase verbosity (-vvv supported)

OPTIONS:
    -c, --config <config>    Configuration file to use
        --url <url>          Daemon published url, common for all enabled networks [default: tcp://127.0.0.1]
```

On first execution, daemon will create default config file ~/.config/darkfi/seedd_config.toml.
Configuration must be verified, and applications should be configured accordingly.

Run seedd as follows:

```
% cargo run -- --darkfid --taud --ircd
17:00:19 [INFO] Starting seed network node for darkfid at: tcp://127.0.0.1:7650
17:00:19 [WARN] Skipping seed sync process since no seeds are configured.
17:00:19 [INFO] Starting seed network node for ircd at: tcp://127.0.0.1:8760
17:00:19 [INFO] Starting inbound session on tcp://127.0.0.1:7650
17:00:19 [WARN] Skipping seed sync process since no seeds are configured.
17:00:19 [INFO] Starting seed network node for taud at: tcp://127.0.0.1:9870
17:00:19 [INFO] Starting inbound session on tcp://127.0.0.1:8760
17:00:19 [WARN] Skipping seed sync process since no seeds are configured.
17:00:19 [INFO] Starting inbound session on tcp://127.0.0.1:9870
17:00:19 [INFO] Starting 0 outbound connection slots.
17:00:19 [INFO] Starting 0 outbound connection slots.
17:00:19 [INFO] Starting 0 outbound connection slots
```
