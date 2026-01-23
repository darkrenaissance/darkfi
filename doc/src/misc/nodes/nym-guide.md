# Set-up a Nym-enabled node

_To connect to through `nym` network we use the `socks5` proxy provided
by [Nym](https://github.com/nymtech/nym)_

<u><b>Note</b></u>: This page is a general guide for `nym` nodes in the
DarkFi ecosystem and is applicable to other apps such as `taud` and
`darkfid`. We use `darkirc` as our main example throughout this guide.
Commands such as `./darkirc` and configuration filenames need to be
adjusted if using different apps. If you're using another app, the
network configurations remain the same except for the seed nodes you
connect to.

<u><b>Note</b></u>: Currently, we can set up only outbound nodes with
`Nym`.

## Generating configuration files

For configuration file generation, see [Common Setup Steps](public-guide.md#generating-configuration-files).

## Configure network settings

Modify the network settings located in the `~/.config/darkfi`
directory. This configuration allows your node to send and receive
traffic only via `Nym`.

<u><b>Note</b></u>: As you modify the file, if you notice some settings
are missing, simply add them. Some settings may be commented-out by
default.

First, download
[nym-socks5-client](https://github.com/nymtech/nym/releases).
Then initialize it:

```
./nym-socks5-client init --id [YOUR_ID] --provider [YOUR_PROVIDER]
```

Replace `[YOUR_ID]` with your own preferred id and `[YOUR_PROVIDER]`
with a `Nym network requester` provider address. You can find a list of
network requester providers from
[Nym Harbour Master](https://harbourmaster.nymtech.net/). Make sure the
provider you select is able to connect to non-standard ports used by
darkirc nodes.

After you initialize it, run the socks5 client:

```
./nym-socks5-client run  --id [YOUR_ID]
```

### Outbound node settings

These outbound node settings for your `nym` node configuration is only
for connecting to the network. You will not advertise an external
address. Make sure `nym-socks5-client` is running, and it's `socks5`
proxy is listening on `127.0.0.1:1080` before running `darkirc`.
To be able to route our darkirc connections through the `socks5` proxy
provided by `Nym`, we need to enable the socks5 proxy transport in our
settings:

```toml
## connection settings
outbound_peer_discovery_cooloff_time = 60

## Outbound connection slots
outbound_connections = 8

## Nym Socks5 proxy
nym_socks5_proxy = "socks5://127.0.0.1:1080"

[net.profiles."socks5+tls"]
## Seed nodes to connect to
seeds = [
    "socks5+tls://127.0.0.1:1080/lilith1.dark.fi:9603"
]
```

## Connect and test your node

See [Common Setup Steps → Connect and test your node](public-guide.md#connect-and-test-your-node).

## Troubleshooting

See [Common Setup Steps → Troubleshooting](public-guide.md#troubleshooting).

## Running a Nym Network Requester
_You can run a `Nym Network Requester` to support the Darkfi P2P
network_

A `Nym` network requester serves as an exit gateway in the `Nym`
network, an equivalent of an exit node in the Tor network. You can
provide your `Nym` network requester address to others so that they
can use your node as an exit gateway. To set it up you need a VPS with
a good internet connection.

First, download
[nym-network-requester](https://github.com/nymtech/nym/releases).
Then initialize it:

```
./nym-network-requester init --id [YOUR_ID]
```

You will see some configuration output along with the address of your
`Nym Network Requester`, make sure to record it. It is the address you
will share with others.

With it's default configuration the `Nym Network Requester` will not
allow connections to pass through non-standard ports, like the one used
by darkirc nodes. By default it uses this
[exit policy](https://nymtech.net/.wellknown/network-requester/exit-policy.txt).
To override this behavior you need to modify the config file found in
`~/.nym/service-providers/network-requester/testing/config/config.toml`.
Go to the line with `upstream_exit_policy` and set it to your exit
policy url:

```
upstream_exit_policy_url = 'http://localhost/YOUR_EXIT_POLICY.txt'
```

Run `nginx` or a `simple http server` to host your exit policy.

This is a sample `exit policy` that whitelists a node with the ip
address `NODE_IP_ADDRESS` and rejects any other connections. Note that
the `exit policy` requires an ip address to be set, it doesn't accept a
domain name:

```
ExitPolicy accept [NODE_IP_ADDRESS]:*
# reject everything else not covered by any of the previous rules
ExitPolicy reject *:*
```
