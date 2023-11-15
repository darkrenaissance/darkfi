# Set-up a Tor-enabled darkirc node

## Step 1: Install Tor and Launch a Hidden Service
[Install Tor](https://darkrenaissance.github.io/darkfi/clients/tor_inbound.html).

Note your `.onion` address and the ports you used while setting up the
Hidden Service.

## Step 2: Build and run darkirc

In the main repository: `make BINS="darkirc"`.

The binary will create a configuration file at `~/.config/darkirc/darkirc_config.toml`.

## Step 3: Configure Network Settings

Tor requires specific timeout settings in order to maintain a stable
connection.

Ensure the config file contains the following:

_If you notice some settings are not present in the file, simply add them._

```toml
# connection settings
outbound_connect_timeout = 60
channel_handshake_timeout = 55
channel_heartbeat_interval = 90
hosts_quarantine_limit = 10
outbound_peer_discovery_cooloff_time = 60

allowed_transports = ["tor", "tor+tls"]
external_addr = ["tor://youraddress.onion:your-port"]

# seeds
seeds = [
    "tor://rwjgdy7bs4e3eamgltccea7p5yzz3alfi2vps2xefnihurbmpd3b7hqd.onion:5262",
    "tor://f5mldz3utfrj5esn7vy7osa6itusotix6nsjhv4uirshkcvgglb3xdqd.onion:5262",
]

# inbound settings
inbound = ["tcp://127.0.0.1:your-port"]
inbound_connections = 8
```

This configuration allows your node to send and receive traffic only via Tor.

The settings under `inbound settings`, but enabling them will increase the strength and 
reliability of the network.

## Step 4: Connect

Run `./darkirc`. Welcome to the dark forest.

## Troubleshooting

Run `darkirc -vv` for verbose debugging. This will show detailed errors including
tor connection issues.

### DagSync spam

If you see a many rapid `EventReq` messages in the log, it is possible that there is
an incompatibility with your local `darkirc` database and the state of the network.

This can be resolved by deleting `~/.local/darkfi/darkirc_db/`

This is a known bug and we are working on a fix.
