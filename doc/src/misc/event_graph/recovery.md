# Event Graph Recovery

This page is for operators running an application that stores data in
`EventGraph`, for example `darkirc`. The safest recovery model is:
stop the process, preserve the datastore, restart once to let automatic
repairs run, and only then decide whether a restore or clean resync is
needed.

## Data layout

`EventGraph` stores several sled trees in the application's datastore.
For DarkIRC this is the `--datastore` path, defaulting to
`~/.local/share/darkfi/darkirc/darkirc_db`. This is not the same as the P2P
`p2p_datastore` path.

Important trees are:

| Tree | Meaning |
| ---- | ------- |
| `<timestamp>` | Rotating DAG event bodies for one rotation slot. |
| `headers_<timestamp>` | Header DAG for the same slot. |
| `dag-blobs` | RLN signal blobs for rotating DAG events. |
| `static-dag` | Persistent static DAG for RLN registrations and slashes. |
| `static-dag-blobs` | Original RLN blobs for static DAG events. |
| `rln-identity-leaves` | Derived RLN identity-tree leaves. |
| `rln-historical-roots-ordered` | Derived historical root index in canonical static-event order. |
| `rln-historical-roots-by-value` | Reverse lookup for historical roots. |

DarkIRC also stores local account secrets in the same sled database under
`darkirc_account_*` and `darkirc_account_default`. Treat the datastore as
secret material. A clean resync can rebuild network DAG state, but it will
not recover local account nullifiers and trapdoors unless they were backed
up or exported with `NickServ INFO <account_name>`.

## First response

1. Stop the application. Do not edit or copy sled files while the process
   is running.
2. Copy the whole datastore directory and the config file before changing
   anything.
3. Keep the exact binary, config, and logs that produced the failure.
4. Start the node once with the same config and watch the event graph logs.

Startup already performs important recovery work. If it succeeds and logs
a completed rebuild, prefer keeping the repaired datastore over manual
surgery.

## Automatic repairs

The static DAG is authoritative for RLN identity state. At startup,
`EventGraph` scans `static-dag`, sorts static events by `(layer, event_id)`,
and compares the result with:

- `rln-identity-leaves`
- `rln-historical-roots-ordered`
- `rln-historical-roots-by-value`

If these side tables are stale or incomplete, startup clears and rebuilds
them from `static-dag`. Expected log lines include:

```text
[EVENTGRAPH] RLN state audit: ... consistent=false
[EVENTGRAPH] Rebuilding RLN state: ...
[EVENTGRAPH] RLN state rebuild complete
```

Startup also audits `static-dag-blobs`. Missing pregenerated registration
blobs are deterministic and are repaired with the genesis guard blob. Slash
blobs and future staked-registration blobs are not reconstructible and must
not be fabricated.

## Common failures

### Corrupted RLN roots or identity leaves

Symptoms:

- `RLN state audit` reports `consistent=false`.
- Signal verification rejects roots that should be historical.
- Startup logs `RLN identity leaf audit failed`.

Safe response:

- Restart with the same config and let the automatic rebuild complete.
- Do not delete `static-dag`; it is the source used to rebuild the RLN side
  tables.
- If rebuild fails because `static-dag` itself is unreadable, restore the
  datastore from backup or perform a clean resync from healthy peers.

### Missing static blobs

Symptoms:

- `static blob audit` reports `repaired > 0`.
- `static blob audit` reports `unrecoverable > 0`.
- Logs mention `static event ... is missing its RLN blob and cannot be reconstructed`.

Safe response:

- If only `repaired` is nonzero, startup restored missing pregenerated guard
  blobs and no manual action is needed.
- If `unrecoverable` is nonzero, restore a backup that still has
  `static-dag-blobs`, or rebuild the node from peers that can serve complete
  static events and blobs.
- Do not invent slash or staked-registration blobs. A node that cannot serve
  original static blobs cannot safely help late joiners verify that history.

### Missing rotating DAG blobs

Symptoms:

- Peers cannot sync current rotating events from this node.
- Logs mention declining to serve an event because the blob is missing.
- A local non-genesis event exists in a `<timestamp>` tree but its RLN signal
  blob is absent from `dag-blobs`.

Safe response:

- Restore from a datastore backup made before the blob loss.
- If the lost slot is disposable, start from a clean datastore with the same
  config and sync from healthy peers.
- Do not copy only `dag-blobs` from another network or config. RLN signal
  proofs are bound to the event and application domain.

### Corrupted rotating DAG or header trees

Symptoms:

- Startup fails while opening DAG slots.
- History ordering or content sync fails on corrupt event/header records.
- Sync repeatedly returns `DagSyncFailed` for one slot.

Safe response:

- For current history, restore a full datastore backup if local account data
  matters.
- For disposable rotating history, move the datastore aside and let the node
  create a fresh store, then sync from peers.
- Do not edit individual sled tree files in place. The `<timestamp>` and
  `headers_<timestamp>` trees must agree with each other, and event blobs must
  agree with the event bodies.

### Config mismatch

Symptoms:

- Layer-1 headers are rejected as referencing a foreign genesis.
- Peers return headers that fail `HeaderIsInvalid`.
- Static or rotating sync fails across otherwise reachable peers.
- RLN signals from peers never verify.

Safe response:

- Confirm every node in the network uses the same event graph consensus
  parameters: `initial_genesis`, `hours_rotation`, `genesis_contents`, and the
  app-provided pregenerated RLN commitment set.
- For DarkIRC these constants are compiled into `bin/darkirc/src/main.rs` and
  `bin/darkirc/src/genesis_commits.rs`.
- Do not reuse a datastore across networks with different consensus
  parameters. Fix the binary/config first, then start with a datastore created
  under the same parameters.

## Clean resync

Use clean resync only after preserving the original datastore.

1. Stop the node.
2. Back up the datastore and config.
3. If local DarkIRC accounts matter, export each account with
   `NickServ INFO <account_name>` before discarding the datastore.
4. Start the node with an empty datastore and the same event graph consensus
   configuration as the network.
5. Let static sync and rotating DAG sync complete from healthy peers.

A clean resync is safe for public network state only if enough peers still
serve complete events and blobs. It is not a replacement for backing up local
account secrets.
