fud
=======

File-sharing Utility Daemon, using DHT for records discovery.

## Usage

```
fud 0.3.0
File-sharing Utility Daemon, using DHT for records discovery.

USAGE:
    fud [FLAGS] [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               Increase verbosity (-vvv supported)

OPTIONS:
    -c, --config <config>                Configuration file to use
        --folder <folder>                Path to the contents directory [default: ~/.config/darkfi/fud]
        --p2p-accept <p2p-accept>        P2P accept address
        --p2p-external <p2p-external>    P2P external address
        --p2p-peer <p2p-peer>...         Connect to peer (repeatable flag)
        --p2p-seed <p2p-seed>...         Connect to seed (repeatable flag)
        --rpc-listen <rpc-listen>        JSON-RPC listen URL [default: tcp://127.0.0.1:9540]
        --slots <slots>                  Connection slots [default: 8]
```

On first execution, daemon will create default config file ~/.config/darkfi/fud_config.toml.
Configuration must be verified and application should be configured accordingly.
Additionaly, default content folder will be created at ~/.config/darkfi/fud.

Run fud as follows:

```
% fud
13:23:04 [INFO] Starting JSON-RPC server
13:23:04 [INFO] Starting sync P2P network
13:23:04 [WARN] Skipping seed sync process since no seeds are configured.
13:23:04 [INFO] Initializing fud dht state for folder: "/home/x/.config/darkfi/fud"
13:23:04 [INFO] Not configured for accepting incoming connections.
13:23:04 [INFO] JSON-RPC listener bound to tcp://127.0.0.1:9540
13:23:04 [INFO] Entry: seedd_config.toml
13:23:04 [INFO] Starting 8 outbound connection slots.
13:23:04 [INFO] Entry: lt.py
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #0
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #1
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #2
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #3
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #6
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #4
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #5
13:23:04 [WARN] Hosts address pool is empty. Retrying connect slot #7
13:23:07 [INFO] Caught termination signal, cleaning up and exiting...
```

fu
=======

Command-line client for fud.

## Usage

```
fu 0.3.0
Daemon that spawns P2P seeds

USAGE:
    fu [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -e, --endpoint <ENDPOINT>    fud JSON-RPC endpoint [default: tcp://127.0.0.1:9540]
    -h, --help                   Print help information
    -v                           Increase verbosity (-vvv supported)
    -V, --version                Print version information

SUBCOMMANDS:
    get     Retrieve provided file name from the fud network
    help    Print this message or the help of the given subcommand(s)
    list    List fud folder contents
    sync    Sync fud folder contents and signal network for record changes
```

Execution examples:

```
% fu list
13:25:14 [INFO] ----------Content-------------
13:25:14 [INFO] 	seedd_config.toml
13:25:14 [INFO] 	lt.py
13:25:14 [INFO] ------------------------------
13:25:14 [INFO] ----------New files-----------
13:25:14 [INFO] No new files to import.
13:25:14 [INFO] ------------------------------
13:25:14 [INFO] ----------Removed keys--------
13:25:14 [INFO] No keys were removed.
13:25:14 [INFO] ------------------------------

% fu sync
13:25:46 [INFO] Daemon synced successfully!

% fu get -f lt.py
13:26:23 [INFO] File waits you at: /home/x/.config/darkfi/fud/lt.py

% fu get -f sdsd
Error: JsonRpcError("\"Did not find key\"")
```
