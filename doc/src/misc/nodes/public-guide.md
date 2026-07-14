# Run a public DarkIRC node

A public node accepts inbound P2P connections and advertises a reachable
address. A clearnet address exposes the node's IP or hostname to peers. Use the
[Tor guide](tor-guide.md) if that is not acceptable.

First follow the [normal-node](../darkirc/normal-node.md) or
[archive-node](../darkirc/archive-node.md) guide. Stop DarkIRC before editing
`~/.config/darkfi/darkirc_config.toml`.

## Clearnet `tcp+tls`

This example listens on the conventional DarkIRC P2P port. Replace the
advertised hostname, open TCP port 9600 in the firewall, and configure router
port forwarding if necessary.

```toml
[net]
active_profiles = ["tcp+tls"]
outbound_connections = 8
inbound_connections = 64

[net.profiles."tcp+tls"]
seeds = ["tcp+tls://lilith0.dark.fi:9600", "tcp+tls://lilith1.dark.fi:9600"]
inbound = ["tcp+tls://0.0.0.0:9600", "tcp+tls://[::]:9600"]
external_addrs = ["tcp+tls://chat.example:9600"]
```

Remove the IPv6 listener if the host has no working IPv6 route. An
`external_addrs` value must resolve to this host and be reachable from outside
its LAN. Do not advertise `0.0.0.0`, `[::]`, loopback, or a private address.

The complete clearnet example is
`bin/darkirc/config/darkirc-clearnet.toml`.

## Multi-transport bridge

A bridge can accept both clearnet and Tor connections. Each inbound transport
needs a reachable external address:

```toml
[net]
active_profiles = ["tcp+tls", "tor"]
outbound_connections = 8
inbound_connections = 64

[net.profiles."tcp+tls"]
seeds = ["tcp+tls://lilith0.dark.fi:9600", "tcp+tls://lilith1.dark.fi:9600"]
inbound = ["tcp+tls://0.0.0.0:9600"]
external_addrs = ["tcp+tls://chat.example:9600"]

[net.profiles."tor"]
seeds = [
    "tor://g7fxelebievvpr27w7gt24lflptpw3jeeuvafovgliq5utdst6xyruyd.onion:9600",
    "tor://yvklzjnfmwxhyodhrkpomawjcdvcaushsj6torjz2gyd7e25f3gfunyd.onion:9600",
]
inbound = ["tor://127.0.0.1:9601"]
```

The `tor://` inbound form asks the built-in Arti transport to create an
ephemeral onion service. Its onion address changes when the service is
recreated and does not need a manual `external_addrs` entry. For a stable onion
address, use the [static Tor setup](tor-guide.md#static-onion-service).

Set `mixed_profiles` only when deliberately routing one endpoint scheme through
another active transport. It is not required merely to enable two profiles.

## UPnP IGD

UPnP can request a router port mapping and discover the external address. It
increases trust in the local router and must be enabled at compile time. Build
with all features:

```shell
% cargo build --release --all-features --package darkirc --bin darkirc
```

This writes `target/release/darkirc`; run or install that binary so the
feature-enabled build is the one actually used.

Then add the query to a clearnet listener:

```toml
[net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:9600?upnp_igd=true"]
```

Optional URL parameters include `upnp_igd_lease_duration`,
`upnp_igd_timeout`, `upnp_igd_description`, and
`upnp_igd_ext_addr_refresh`. Verify the mapping from another network; a log
message or local router entry alone does not prove reachability.

## Verify the node

Restart DarkIRC, wait for Event Graph sync, and inspect sessions with `dnet`
after enabling `p2p.get_info` as described in the
[dnet guide](../../learn/dchat/network-tools/using-dnet.md).
Test every advertised URL from a separate network using the
[ping utility](../network-troubleshooting.md#test-an-endpoint). An archive is
useful to peers only while it remains reachable and retains complete message
bodies (`fast_mode = false`).
