# Set-up a Tor-enabled darkirc node

_To connect to Tor, we use [Arti](https://gitlab.torproject.org/tpo/core/arti). 
This is an experimental project with incomplete security features. See Arti's 
[roadmap](https://gitlab.torproject.org/tpo/core/arti#roadmap) for more 
information._

Using Tor, we can host anonymous nodes as Tor hidden services. To do
this, we need to set up our Tor daemon and create a hidden service.
The following instructions should work on any Linux system.

## Step 1: Install Tor and Launch a Hidden Service

Tor can usually be installed with your package manager. For example
on an `apt` based system we can run:

```
# apt install tor
```

This will install it. Now in `/etc/tor/torrc` we can set up the hidden
service. For hosting an anonymous `darkirc` node, set up the following
lines in the file:

```
HiddenServiceDir /var/lib/tor/darkfi_darkirc
HiddenServicePort 25551 127.0.0.1:25551
```

Then restart Tor:

```
# /etc/init.d/tor restart
```

You can grab the hostname of your hidden service from the directory:

```
# cat /var/lib/tor/darkfi_darkirc/hostname
```

Note your `.onion` address and the ports you used while setting up the
Hidden Service.

## Step 2: Build and run darkirc

In the main repository: `make BINS="darkirc"`. Then run `./darkirc`. This will create 
a configuration file at `~/.config/darkirc/darkirc_config.toml`.

## Step 3: Configure Network Settings

Change the following settings in the configuration file created in Step 2.

As you modify the file, if you notice some settings are missing, simply add them.
Some settings may be commented-out by default.

```toml
# connection settings
outbound_connect_timeout = 60
channel_handshake_timeout = 55
channel_heartbeat_interval = 90
outbound_peer_discovery_cooloff_time = 60

allowed_transports = ["tor", "tor+tls"]
external_addr = ["tor://youraddress.onion:your-port"]

# seeds
seeds = [
    # These addresses are commented-out by default
    "tor://rwjgdy7bs4e3eamgltccea7p5yzz3alfi2vps2xefnihurbmpd3b7hqd.onion:5262",
    "tor://f5mldz3utfrj5esn7vy7osa6itusotix6nsjhv4uirshkcvgglb3xdqd.onion:5262",
]

# inbound settings
inbound = ["tcp://127.0.0.1:your-port"]
inbound_connections = 8
```

This configuration allows your node to send and receive traffic only via Tor.

The settings under `inbound settings` are optional, but enabling them will 
increase the strength and reliability of the network.

## Step 4: Connect

Run `./darkirc`. Welcome to the dark forest.

## Troubleshooting

Run `darkirc -vv` for verbose debugging. This will show detailed errors including
tor connection issues. Refer to 
[Network troubleshooting](../misc/network-troubleshooting.html)
for further troubleshooting resources.

### Connection issues

Review the configuration file for any mistakes. Check for duplicate variables.

### DagSync spam

If you see a many rapid `EventReq` messages in the log, it is possible that there is
an incompatibility with your local `darkirc` database and the state of the network.

This can be resolved by deleting `~/.local/darkfi/darkirc_db/`

This is a known bug and we are working on a fix.
