# DarkIRC

DarkIRC is DarkFi's peer-to-peer chat daemon. It exposes a local IRC server so
standard IRC clients can use it, while DarkFi's P2P network and Event Graph
synchronize messages between DarkIRC nodes.

DarkIRC implements a practical subset of IRC rather than the complete IRC
protocol. Public channel messages are public. A configured channel secret or
contact keypair encrypts the corresponding message fields with
`crypto_box::ChaChaBox`, but DarkIRC does not implement the Signal protocol.

Nicknames are included in message events and may be changed at any time. They
are not proof of a real-world identity. Optional NickServ/RLN identities are a
separate feature and RLN is disabled by default.

## Build and install

Follow the repository [build prerequisites](../../README.md#build), then build
DarkIRC from the repository root:

```shell
% make darkirc
```

The binary is written to `./darkirc`. To install it under
`$HOME/.cargo/bin`, use the DarkIRC makefile:

```shell
% make -C bin/darkirc install
```

Run that command again after building a newer version.

### Android

The Android container target builds a 64-bit ARM binary suitable for running
inside Termux:

```shell
% make -C bin/darkirc podman-android
```

The result is `bin/darkirc/darkirc.aarch64-android`. Copy it to the phone,
make it executable, and run it in Termux. Keep the process awake if Android
would otherwise suspend it, then connect an Android IRC client to
`127.0.0.1:6667` without IRC TLS. This local IRC connection is distinct from
the encrypted P2P transports used between DarkIRC nodes.

## Network privacy and message privacy

These are separate concerns:

- Direct P2P peers can observe the network address used for a connection.
- The generated configuration enables the built-in `tor` profile by default.
  See the [Tor guide](../nodes/tor-guide.md) before changing transports.
- Nym can be used as an outbound SOCKS5 transport; see the
  [Nym guide](../nodes/nym-guide.md).
- Public channels store their channel name, nickname, and message as
  plaintext Event Graph content.
- A channel is encrypted only when every participant configures the same
  channel `secret`. Contact DMs require the keypairs described in the
  [private-message guide](private_message.md).

Transport anonymity depends on the selected network and its threat model. It
does not make messages unlinkable by itself, and encrypted content can still
expose metadata through timing, participation, or the IRC client.

## Choose a node mode

DarkIRC rotates its Event Graph once per hour. The default normal node syncs
and retains 24 DAGs, a rolling full-day history window.

- [Run a normal node](normal-node.md) for chatting and ordinary P2P
  participation. Old rotating DAGs are pruned.
- [Run an archive node](archive-node.md) to retain every rotating DAG received
  after the archive starts and serve that history to peers.

`dags_count` controls startup and reconnect sync depth. `archive_mode` controls
retention. When archive mode is selected, `history_retention_dags` is ignored,
but `dags_count` still determines how many recent DAGs are requested during a
sync. The node-mode guides explain the bootstrap implications.

## First run

Start DarkIRC from the repository root:

```shell
% ./darkirc
```

On its first run it creates
`~/.config/darkfi/darkirc_config.toml` and continues starting. On macOS the
configuration directory is `~/Library/Application Support/darkfi/`.

The default runtime state is namespaced under
`~/.local/share/darkfi/darkirc/`:

| Setting | Default |
| --- | --- |
| `datastore` | `~/.local/share/darkfi/darkirc/darkirc_db` |
| `zk_key_datastore` | `~/.local/share/darkfi/darkirc/zk_keys` |
| `replay_datastore` | `~/.local/share/darkfi/darkirc/replayed_darkirc_db` |
| `net.p2p_datastore` | `~/.local/share/darkfi/darkirc/p2p` |
| `net.hostlist` | `~/.local/share/darkfi/darkirc/p2p_hostlist.tsv` |

Review the generated `[net]` settings before connecting. Stop the daemon with
`Ctrl-C`, edit the file, and restart it when changing node mode, storage, RPC,
or P2P settings. With the defaults, startup sync completes with:

```text
Event DAG synced successfully (full mode, 24 dag(s))
```

## Connect an IRC client

DarkIRC listens on `tcp://127.0.0.1:6667` by default. For WeeChat, add the
local server after starting the client:

```text
/server add darkfi localhost/6667 -notls -autoconnect
/save
/connect darkfi
```

The displayed nick list is reconstructed from messages the local node has
seen; it is not a live global presence list. Change nickname with `/nick foo`.

After editing only `autojoin`, `[channel.*]`, or `[contact.*]`, reload those
settings without restarting:

```text
/rehash
```

All other configuration changes require a daemon restart.

## Encrypted channels

Generate a shared channel secret:

```shell
% ./darkirc --gen-channel-secret
```

Configure the exact same secret on every participant's node:

```toml
[channel."#project"]
secret = "BASE58_CHANNEL_SECRET"
topic = "Private project channel"
```

Exchange this secret over an authenticated, confidential channel. Anyone with
the secret can read all encrypted messages for that channel that they obtain;
there is no per-member key rotation or forward secrecy.

## Local two-node deployment

For development, use two independent configurations and datastores. The first
node listens on `127.0.0.1:9700`:

```toml
irc_listen = "tcp://127.0.0.1:6667"
datastore = "~/.local/share/darkfi/darkirc/localnet/a/darkirc_db"

[rpc]
rpc_listen = "tcp://127.0.0.1:9705"

[net]
localnet = true
active_profiles = ["tcp"]
p2p_datastore = "~/.local/share/darkfi/darkirc/localnet/a/p2p"
hostlist = "~/.local/share/darkfi/darkirc/localnet/a/p2p_hostlist.tsv"

[net.profiles."tcp"]
inbound = ["tcp://127.0.0.1:9700"]
```

The second node connects to it manually:

```toml
irc_listen = "tcp://127.0.0.1:6668"
datastore = "~/.local/share/darkfi/darkirc/localnet/b/darkirc_db"

[rpc]
rpc_listen = "tcp://127.0.0.1:9706"

[net]
localnet = true
active_profiles = ["tcp"]
p2p_datastore = "~/.local/share/darkfi/darkirc/localnet/b/p2p"
hostlist = "~/.local/share/darkfi/darkirc/localnet/b/p2p_hostlist.tsv"

[net.profiles."tcp"]
peers = ["tcp://127.0.0.1:9700"]
```

Start both with `./darkirc --config PATH`. Connect separate IRC clients to
ports 6667 and 6668. `localnet = true` is required because loopback P2P
addresses are rejected in ordinary network mode.

## Custom networks

Keep a custom DarkIRC network isolated from the public network:

- give every instance distinct DarkIRC, P2P, hostlist, and RPC paths;
- use only custom `peers` and `seeds` under the appropriate
  `[net.profiles."..."]` table;
- use the same non-public `net.magic_bytes` on every custom node; and
- do not reuse a datastore created with different Event Graph consensus
  parameters.

Changing `magic_bytes` separates P2P handshakes; it does not change DarkIRC's
compiled Event Graph genesis or RLN commitment set.

## Operations and recovery

The DarkIRC datastore contains public Event Graph state and can also contain
local NickServ account secrets. Stop the process and back up the complete
`datastore` directory before recovery or migration. A clean resync can recover
public DAG state from suitable peers, but it cannot recover local nullifiers
and trapdoors that were never backed up or exported.

Do not hand-edit RLN counters, static DAG state, or individual sled trees. See
[Event Graph recovery](../event_graph/recovery.md) and
[security invariants](../event_graph/security_invariants.md). For connection
problems, see [network troubleshooting](../network-troubleshooting.md).
