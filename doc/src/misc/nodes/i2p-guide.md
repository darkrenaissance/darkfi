# Set-up an I2p-enabled node

_To connect to I2p network we use the socks5 proxy provided by [I2pd](https://i2pd.website/)_

<u><b>Note</b></u>: This page is a general guide for `i2p` nodes in the DarkFi 
ecosystem and is applicable to other apps such as `taud` and `darkfid`. We use 
`darkirc` as our main example throughout this guide. Commands such as `./darkirc`
and configuration filenames need to be adjusted if using different apps.
If you're using another app, the network configurations remain the same except 
for the seed nodes you connect to.

## Generating configuration files

After compiling, you can start the application so it can spawn its configuration 
file. We use `darkirc` as the application example going forward.

```shell
% ./darkirc
```

`darkirc` creates a configuration file `darkirc_config.toml` by default in 
`~/.config/darkfi/`. You will review and edit this configuration file for your 
preferred network settings. 

## Configure network settings

Modify the network settings located in the `~/.config/darkfi` directory. This 
configuration allows your node to send and receive traffic only via I2p.

<u><b>Note</b></u>: As you modify the file, if you notice some settings are missing, 
simply add them. Some settings may be commented-out by default. In the example 
configurations below, you will find the a placeholder `youraddress.b32.i2p` which 
indicates you should replace them with your i2p address.

First, you must install [I2pd](https://i2pd.readthedocs.io/en/latest/user-guide/install/). It can usually be
installed with your package manager. For example on an `apt` based system we can run:

```
% sudo apt install apt-transport-https
% wget -q -O - https://repo.i2pd.xyz/.help/add_repo | sudo bash -s -
% sudo apt update
% sudo apt install i2pd
```

### Outbound node settings

These outbound node settings for your i2p node configuration is only for
connecting to the network. You will not advertise an external address.
Make sure `i2pd` is running, and it's `socks5` proxy is listening on `127.0.0.1:4447`
before running `darkirc`.

<!-- TODO: replace the i2p seed address with an official one-->

```toml
## connection settings
outbound_peer_discovery_cooloff_time = 60

## Outbound connection slots
outbound_connections = 8

## Whitelisted transports for outbound connections
active_profiles = ["i2p"]

## I2p Socks5 proxy
i2p_socks5_proxy = "socks5://127.0.0.1:4447"

[net.profiles."i2p"]
## Seed nodes to connect to
seeds = [
    "i2p://6l2rdfriixo2nh5pr5bt555lyz56qox2ikzia4kuzm4okje7gtmq.b32.i2p:5262"
]
```

### Inbound node settings

With these settings your node becomes an I2p inbound node. The `inbound` 
settings are optional, but enabling them will increase the strength and 
reliability of the network. Using I2p, we can host anonymous nodes as I2p eepsites(hidden 
services). To do this, we need to set up our I2p daemon and create a hidden service.
The following instructions should work on any Linux system.

After installing `I2pd`, Now we can set up the hidden service. 
For hosting an anonymous `darkirc` node, go to `/var/lib/i2pd/tunnels.d`
and create a file `darkirc.conf` with the following contents:

```
[darkirc]
type = server
host = 127.0.0.1
port = 25551
keys = darkirc.dat
```

Then restart i2pd:

```
% systemctl restart i2pd
```

Find the hostname of your hidden service by running the following command:

```
% curl -s http://127.0.0.1:7070/?page=i2p_tunnels | grep -Eo "[a-zA-Z0-9./?=_%:-]*" | grep "25551"
```

The above configuration saves the `i2p` hidden service key in `/var/lib/i2pd/darkirc.dat`,
you might want to back it up.

Note your `.b32.i2p` address and the ports you used while setting up the
hidden service, and add the following settings to your configuration file:

```toml
## Inbound connection slots
inbound_connections = 64

[net.profiles."i2p"]
## Addresses we want to advertise to peers
external_addrs = ["i2p://youraddress.b32.i2p:25551"]

## P2P accept addresses
inbound = ["tcp://127.0.0.1:25551"]
```

## Connect and test your node

Run `./darkirc`. Welcome to the dark forest.

You can test if your node is configured properly on the network. Use 
[Dnet](../../learn/dchat/network-tools/using-dnet.md) and the 
[ping-tool](../network-troubleshooting.md#ping-tool) to test your node 
connections. You can view if your node is making inbound and outbound connections.

## Troubleshooting

Refer to [Network troubleshooting](../network-troubleshooting.md)
for further troubleshooting resources.
