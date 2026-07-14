# Run a Normal DarkIRC Node

A normal node is the recommended setup for chatting. It performs a full sync
of the latest 24 hourly DAGs and keeps a rolling 24-DAG window. When a new DAG
is created, the oldest DAG and its message data are deleted from the local
datastore.

## Build and create the configuration

From the repository root, build DarkIRC:

```shell
% make darkirc
```

Run it once to create `~/.config/darkfi/darkirc_config.toml`:

```shell
% ./darkirc
```

The node continues running after creating the file. Stop it with `Ctrl-C` if
you want to review the configuration before connecting. On macOS, the default
configuration directory is `~/Library/Application Support/darkfi/`.

## Configure the history window

The generated configuration already uses these defaults. Uncomment the
settings if you want them recorded explicitly:

```toml
# Fetch complete message data rather than headers only.
fast_mode = false

# Fetch the latest 24 hourly DAGs at startup.
dags_count = 24

# Prune old DAGs, keeping a rolling 24-hour window.
archive_mode = false
history_retention_dags = 24
```

`dags_count` is the number of recent DAGs fetched during startup and after a
network reconnection. `history_retention_dags` is the number kept locally. In
normal mode, `dags_count` cannot be greater than
`history_retention_dags`. Each DAG represents one hour with DarkIRC's current
rotation settings.

Review the `[net]` section before starting, especially `active_profiles`, seed
addresses, proxy settings, and any inbound address you intend to expose. See
[network and message privacy](darkirc.md#network-privacy-and-message-privacy) and the
[node configuration guides](../nodes/node-configurations.md) for transport and
public-listener setups.

## Start and connect

Start the node from the repository root:

```shell
% ./darkirc
```

To use a configuration in another location, pass it explicitly:

```shell
% ./darkirc --config /path/to/darkirc_config.toml
```

Wait for the full startup sync to finish:

```text
Event DAG synced successfully (full mode, 24 dag(s))
```

Then connect an IRC client to `127.0.0.1:6667`, unless you changed
`irc_listen`. The [IRC client instructions](darkirc.md#connect-an-irc-client)
provide a WeeChat example.

Stop the node with `Ctrl-C` so the datastores are flushed cleanly. Changes to
history, datastore, and network settings take effect only after a restart;
`/rehash` is only for reloadable IRC settings such as channels and contacts.
