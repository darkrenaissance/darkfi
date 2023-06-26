lilith
======

A tool to deploy multiple P2P network seed nodes for DarkFi
applications with a single daemon.

## Usage

```
lilith 0.4.1
Daemon that spawns P2P seeds


USAGE:
    lilith [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               Increase verbosity (-vvv supported)

OPTIONS:
        --accept-addrs <accept-addrs>...    Accept addresses (URL without port)
    -c, --config <config>                   Configuration file to use
        --hosts-file <hosts-file>           Hosts .tsv file to use [default: ~/.config/darkfi/lilith_hosts.tsv]
        --rpc-listen <rpc-listen>           JSON-RPC listen URL [default: tcp://127.0.0.1:18927]
```

On first execution, daemon will create default config file ~/.config/darkfi/lilith_config.toml.
Configuration must be verified, and application networks should be configured accordingly.

Run lilith as follows:

```
$ lilith
[INFO] Found configuration for network: foo_network
[INFO] Starting seed network node for "foo_network" on ["tcp://0.0.0.0:18911"]
[INFO] [P2P] Seeding P2P subsystem
[WARN] [P2P] Skipping seed sync process since no seeds are configured.
[INFO] [P2P] Running P2P subsystem
[INFO] [P2P] Starting Inbound session #0 on tcp://0.0.0.0:18911
[INFO] [P2P] Starting 0 outbound connection slots.
[INFO] [P2P] P2P subsystem started
[INFO] Starting periodic host purge task for "foo_network"
```
