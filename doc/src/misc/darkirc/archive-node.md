# Run an Archive DarkIRC Node

An archive node keeps every rotating DAG in its datastore instead of deleting
old history. Run it continuously if you want to preserve and serve DarkIRC
message history. Disk use and startup work grow over time, so use persistent
storage, monitor its free space, and back it up.

## Build and create a dedicated configuration

From the repository root, build and run DarkIRC once:

```shell
% make darkirc
% ./darkirc
```

The first run creates `~/.config/darkfi/darkirc_config.toml` and continues
starting the node. Stop it with `Ctrl-C` before editing the file.

An archive should use a dedicated DarkIRC datastore. Do not point two running
processes at the same datastore:

```toml
# Keep message bodies; header-only mode is not suitable for a complete archive.
fast_mode = false

# Sync the latest full day when starting or reconnecting.
dags_count = 24

# Never prune rotating DAGs. history_retention_dags is ignored in this mode.
archive_mode = true

datastore = "~/.local/share/darkfi/darkirc/archive/darkirc_db"

[net]
p2p_datastore = "~/.local/share/darkfi/darkirc/archive/p2p"
hostlist = "~/.local/share/darkfi/darkirc/archive/p2p_hostlist.tsv"
```

Keep the remaining `[net]` and `[net.profiles.*]` tables from the generated
configuration. Configure at least one working outbound profile so the node can
sync. A reachable inbound address and matching `external_addrs` entry let more
peers discover the archive and request its history; follow the
[public node guide](../nodes/public-guide.md) for that transport setup.

## Understand archive bootstrap

`archive_mode` prevents future pruning and reloads all DAG trees already in the
datastore. It does not recreate history that was pruned before archive mode was
enabled. A fresh archive initially syncs the `dags_count` recent DAGs—24 hours
with the configuration above—and then retains every new hourly DAG.

To operate an archive containing history from before it was started, bootstrap
it from a known-good archive datastore while both nodes are stopped. Copy the
entire DarkIRC datastore, not individual sled trees. Be aware that this
datastore can also contain local NickServ account secrets; a purpose-built
archive should not be used for personal IRC accounts. See the
[operations and recovery notes](darkirc.md#operations-and-recovery) before copying or
restoring it.

## Start and verify

Start the archive with its configuration:

```shell
% ./darkirc --config ~/.config/darkfi/darkirc_config.toml
```

At startup, the logs should state that archive mode is enabled and then report
the recent full sync:

```text
Archive mode enabled; retaining all local DAGs and syncing 24 recent DAG(s) at startup
Event DAG synced successfully (full mode, 24 dag(s))
```

Keep the process running across hourly rotations. Stop it with `Ctrl-C` for a
clean datastore flush before backups, upgrades, or datastore transfers.
`archive_mode`, sync, datastore, and network changes require a restart; they
cannot be applied with IRC `/rehash`.
