# Set-up a Tor-enabled node

_To connect to Tor, we use [Arti](https://gitlab.torproject.org/tpo/core/arti). 
Arti is an experimental project with incomplete security features. See Arti's 
[roadmap](https://gitlab.torproject.org/tpo/core/arti#roadmap) for more 
information._

<u><b>Note</b></u>: This page is a general guide for `tor` nodes in the DarkFi 
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
configuration allows your node to send and receive traffic only via Tor.

<u><b>Note</b></u>: As you modify the file, if you notice some settings are missing, 
simply add them. Some settings may be commented-out by default. In the example 
configurations below, you will find the a placeholder `youraddress.onion` which 
indicates you should replace them with your onion address.

### Outbound node settings

These outbound node settings for your `tor` node configuration is only for
connecting to the network. You will not advertise an external address.

```toml
## connection settings
outbound_peer_discovery_cooloff_time = 60

## Outbound connection slots
outbound_connections = 8

## Whitelisted transports for outbound connections
active_profiles = ["tor"]

## Transports to be mixed
mixed_profiles = []

[net.profiles."tor"]
## Seed nodes to connect to
seeds = [
    "tor://g7fxelebievvpr27w7gt24lflptpw3jeeuvafovgliq5utdst6xyruyd.onion:25552",
    "tor://yvklzjnfmwxhyodhrkpomawjcdvcaushsj6torjz2gyd7e25f3gfunyd.onion:25552",
]
```

#### Socks5 proxy node settings
If we want to route all our connections through the `socks5` proxy provided by Tor,
we can add the `socks5` and `socks5+tls` profiles to `active_profiles` and enable
transport mixing by adding `tor` and `tcp+tls` to `mixed_profiles`. Enabling
transport mixing helps us to connect to `tor` and `tcp+tls` endpoints through
our socks5 proxy.

When using `Whonix`, this configuration helps prevent the `Tor over Tor` issue.
Ensure that the `tor_socks5_proxy` field is correctly set.

<u><b>Note</b></u>: With this setup, our node will connect to both Tor and clearnet
nodes through the Socks5 proxy.
```toml
## Whitelisted transports for outbound connections
active_profiles = ["socks5", "socks5+tls", "tcp+tls", "tor"]
## Transports to be mixed
mixed_profiles = ["tor", "tcp+tls"]
## Tor Socks5 proxy
tor_socks5_proxy = "socks5://127.0.0.1:9050"
```
If you prefer to connect only to `tor` nodes, modify the above config like below.
```toml
## Whitelisted transports for outbound connections
active_profiles = ["socks5", "tor"]
## Transports to be mixed
mixed_profiles = ["tor"]
```

### Inbound node settings

With these settings your node becomes a Tor inbound node. The `inbound` 
settings are optional, but enabling them will increase the strength and 
reliability of the network.

There are currently two methods of doing this, both documented below.
The Arti method allows you to create ephemeral onions that will
change each time you restart your node. Alternatively you can make a
non-ephemeral service using the torrc method. In this case the address
always stays the same, which is useful for nodes such as seed nodes that
need to be found on the same onion adddress.

#### Using Arti

We can use Arti to create an ephemeral onion on each startup that we
will receive Inbound connections on. Set this in your config file with
a port number of your choice:

```
inbound = ["tor://127.0.0.1:25551"]
```

On running your node, you should get a message like this:

```
[INFO] [P2P] Starting Inbound session #0 on tor://127.0.0.1:25551/
```

This means your ephemeral onion is active and awaiting connections. 

#### Using torrc

Alternatively, we can set up a static Tor daemon and create a hidden
service.  The following instructions should work on any Linux system.

First, you must install [Tor](https://www.torproject.org/). It can usually be 
installed with your package manager. For example on an `apt` based system we can run:

```
% apt install tor
```

This will install Tor. Now in `/etc/tor/torrc` we can set up the hidden
service. For hosting an anonymous `darkirc` node, set up the following
lines in the file:

```
HiddenServiceDir /var/lib/tor/darkfi_darkirc
HiddenServicePort 25551 127.0.0.1:25551
```

Then restart Tor:

```
% /etc/init.d/tor restart
```

Find the hostname of your hidden service from the directory:

```
% cat /var/lib/tor/darkfi_darkirc/hostname
```

Note your `.onion` address and the ports you used while setting up the
hidden service, and add the following settings to your configuration file:

```toml
## Inbound connection slots
inbound_connections = 64

[net.profiles."tor"]
## Addresses we want to advertise to peers
external_addrs = ["tor://youraddress.onion:25551"]

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
