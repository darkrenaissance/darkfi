lilith
==========

A tool to deploy multiple P2P network seed nodes for darkfi applications, in a single daemon.

## Usage

```
lilith 0.3.0
Daemon that spawns P2P seeds

USAGE:
    lilith [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               Increase verbosity (-vvv supported)

OPTIONS:
    -c, --config <config>            Configuration file to use
        --rpc-listen <rpc-listen>    JSON-RPC listen URL [default: tcp://127.0.0.1:18927]
        --urls <urls>...             Daemon published urls, common for all enabled networks (repeatable flag)
```

On first execution, daemon will create default config file ~/.config/darkfi/lilith_config.toml.
Configuration must be verified, and application networks should be configured accordingly.

Run lilith as follows:

```
% lilith
17:22:12 [INFO] Found configuration for network: darkfid_consensus
17:22:12 [INFO] Found configuration for network: darkfid_sync
17:22:12 [INFO] Found configuration for network: ircd
17:22:12 [INFO] Urls are not provided, will use: tcp://127.0.0.1
17:22:12 [INFO] Starting seed network node for darkfid_sync at: ["tcp://127.0.0.1:33032"]
17:22:12 [WARN] Skipping seed sync process since no seeds are configured.
17:22:12 [INFO] Starting seed network node for darkfid_consensus at: ["tcp://127.0.0.1:33033"]
17:22:12 [INFO] #0 starting inbound session on tcp://127.0.0.1:33032
17:22:12 [WARN] Skipping seed sync process since no seeds are configured.
17:22:12 [INFO] Starting seed network node for ircd at: ["tcp://127.0.0.1:25551"]
17:22:12 [INFO] #0 starting inbound session on tcp://127.0.0.1:33033
17:22:12 [WARN] Skipping seed sync process since no seeds are configured.
17:22:12 [INFO] Starting JSON-RPC server
17:22:12 [INFO] #0 starting inbound session on tcp://127.0.0.1:25551
17:22:12 [INFO] Starting 0 outbound connection slots.
17:22:12 [INFO] Starting 0 outbound connection slots.
17:22:12 [INFO] JSON-RPC listener bound to tcp://127.0.0.1:18927
17:22:12 [INFO] Starting 0 outbound connection slots.
```
