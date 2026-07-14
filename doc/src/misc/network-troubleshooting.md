# Network troubleshooting

This page uses DarkIRC examples. Other DarkFi daemons use the same P2P profile
schema but have their own configuration files, seed endpoints, and ports.

For DarkIRC, the authoritative template is
`bin/darkirc/darkirc_config.toml`. The generated configuration is
`~/.config/darkfi/darkirc_config.toml` on Linux and
`~/Library/Application Support/darkfi/darkirc_config.toml` on macOS.

## Start with debug logs

Run DarkIRC with additional verbosity:

```shell
% ./darkirc -vv
```

Or configure a file log:

```toml
log = "/tmp/darkirc.log"
verbose = 2
```

Do not publish the complete configuration or datastore: they can contain IRC
passwords, channel secrets, contact keys, and NickServ/RLN account secrets.

## DAG sync fails

`DagSyncFailed` during initial static or rotating sync commonly means there
are no usable peer channels, although incompatible or incomplete peer data can
also cause it. Check the log in this order:

1. Confirm the selected `[net].active_profiles` are defined by matching
   `[net.profiles."..."]` tables.
2. Confirm the profile has current `seeds` or `peers` entries.
3. For proxy transports, confirm the local proxy is listening and the proxy
   URL is correct.
4. Check the host clock. Event Graph rotation uses timestamps; a materially
   incorrect clock can select the wrong hourly DAG slots.
5. Confirm the binary and Event Graph consensus parameters match the network.

A manual peer belongs inside its transport profile, for example:

```toml
[net]
active_profiles = ["tcp+tls"]

[net.profiles."tcp+tls"]
seeds = ["tcp+tls://lilith0.dark.fi:9600", "tcp+tls://lilith1.dark.fi:9600"]
peers = ["tcp+tls://known-peer.example:9600"]
```

Use an endpoint supplied by an operator you trust; the example peer hostname
above is only a placeholder.

## Inspect the hostlist

DarkIRC's default hostlist is:

```text
~/.local/share/darkfi/darkirc/p2p_hostlist.tsv
```

Its path is configured in `[net]`:

```toml
[net]
hostlist = "~/.local/share/darkfi/darkirc/p2p_hostlist.tsv"
```

An empty hostlist on a first start is expected until the node learns peers. If
seeds are unreachable and the hostlist has no usable entries, add a known
manual peer to the appropriate profile.

## Test an endpoint

The repository includes a transport-level ping utility. From
`script/ping`, test a complete endpoint URL:

```shell
% cargo run --release --all-features -- tcp+tls://known-peer.example:9600
```

A successful transport handshake prints `Connected!` and version information.
Run the same test from a separate network when checking whether your advertised
inbound endpoint is reachable. For example:

```shell
% cargo run --release --all-features -- tor://youraddress.onion:9600
```

This tests reachability, not whether the remote node has compatible DarkIRC
history.

## Inbound nodes

An inbound listener and its advertised address must describe the same reachable
service. They are configured under the same profile:

```toml
[net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:9600"]
external_addrs = ["tcp+tls://chat.example:9600"]
```

Open or forward the port in the host firewall and router. Test the
`external_addrs` URL from another device. Do not advertise loopback or a
private LAN address to the public network.

For Tor static and ephemeral inbound configurations, see the
[Tor guide](nodes/tor-guide.md#inbound-tor-node).

## Tor connections

The generated DarkIRC configuration uses the built-in `tor` profile, which is
implemented with Arti and does not require a separately configured Tor SOCKS5
daemon. If using the explicit `socks5` profile instead, the proxy in the URL
must be running.

When onion connections fail, distinguish between:

- no local internet route or blocked Tor bootstrap;
- an unavailable onion service;
- a stale or corrupted Arti state directory; and
- a profile mismatch, such as enabling `tor` without a
  `[net.profiles."tor"]` table.

Preserve logs before removing any cache. If resetting Arti state is necessary,
stop every process using it first and move the state directory aside instead
of deleting it immediately.

## Connected but no messages arrive

Wait for this startup message before testing IRC history or sending chat:

```text
Event DAG synced successfully (full mode, 24 dag(s))
```

If it never appears, investigate sync rather than the IRC client. If it does
appear, confirm:

- the IRC client is connected to the configured `irc_listen` address;
- the client joined the intended channel;
- encrypted-channel participants use the same channel secret; and
- the contact label and keys match when testing a DM.

`fast_mode = true` fetches headers without message bodies, so it is not
suitable when the local client needs complete history.

## Repeated or corrupt DAG sync

Do not immediately delete the DarkIRC datastore. Its default path is
`~/.local/share/darkfi/darkirc/darkirc_db`, and it can contain local account
secrets as well as public history.

Stop DarkIRC, preserve the entire datastore and configuration, and follow the
[Event Graph recovery guide](event_graph/recovery.md). If a clean resync is
appropriate, move the old directory aside and start with a new empty path.
Never copy individual sled trees between datastores.

## Network viewers

`dnet` displays inbound, outbound, manual, and seed sessions through the
daemon's JSON-RPC interface. See [Using dnet](../learn/dchat/network-tools/using-dnet.md).
The default DarkIRC RPC listener is `tcp://127.0.0.1:9605`; it is local-only
unless explicitly changed. The generated config disables `p2p.get_info`; remove
that method from `rpc_disabled_methods` and restart DarkIRC before using dnet.

When reporting an issue, include the DarkIRC version, selected profiles,
redacted logs, operating system, and whether the endpoint ping succeeded.
