# Event Graph Security Invariants

This page records the invariants that applications must preserve when they
integrate `EventGraph`. It is written for developers and operators reviewing
changes around sync, RLN admission, and DarkIRC account handling.

## Event admission

Non-genesis rotating DAG events are not just event bodies. They are the tuple:

- header and content
- a stored body in the rotating DAG tree
- an aligned RLN signal blob in `dag-blobs`

The safe public insertion path is `insert_signal_with_blob()` for local events
and `dag_insert_with_blobs()` for sync or peer-supplied events. Do not insert a
non-genesis body with `dag_insert()` unless the caller is a test or another
explicitly proofless path. A body without its blob cannot be served honestly to
late joiners because peers must re-verify the signal proof during sync.

Rotating event admission has two parent-closure checks:

1. Before RLN verification, every non-genesis candidate must have parent bodies
   already committed locally or accepted earlier in the same batch.
2. Before the final body write, the same condition is checked again under the
   DAG write lock.

These checks are separate on purpose. Header sync can learn structure before
body sync, but body admission must never create a body DAG whose parent body is
missing.

## Static DAG admission

The static DAG stores RLN registrations and slashes. It is authoritative for
identity-tree state and for the historical-root tables. Static events must be
committed through `commit_verified_static_event()` after `rln_verify_static_event()`
accepts the RLN node and blob.

Static events also require parent closure. During static sync, a node must not
commit a static event until the parent chain is known locally or accepted earlier
in the same sync batch. This matters because historical roots are indexed by
canonical static-event order, using `(layer, event_id)`, and every accepted
static event creates a historical-root entry even if the SMT mutation is a
soft no-op.

Static event blobs are durable consensus evidence:

- pregenerated registrations may use the deterministic `GENESIS_BLOB_GUARD`
- slash blobs are not reconstructible
- future staked-registration blobs are not reconstructible

Operators must not fabricate missing static blobs. If `static-dag-blobs` loses a
slash or future staked-registration blob, the node must restore from backup or
resync from peers that still have the original blob.

## RLN roots

A live RLN signal may verify against the current identity-tree root. A
historical signal may verify against a historical root only if that root was
valid at the signal timestamp, within the event time-drift window.

This distinction prevents stale-root replay after slashing. Recent roots kept in
memory are not enough by themselves: any non-current root must pass the
historical timestamp-window lookup in `is_root_valid_at()`.

## Slashing

Slash evidence is static DAG state. A valid slash removes the commitment from
the identity tree, records a new historical root, and keeps the slash visible to
future syncers. A slashed identity must not be allowed to re-register through the
pregenerated path or the future staked path.

Slash replay is a soft no-op for the identity tree, but it must not be treated
as a fresh registration or a way to resurrect an identity. Duplicate or
repackaged slash events still participate in static-DAG ordering only after
they pass verification. Invalid slash blobs are rejected and must not mutate
identity state or historical-root indexes.

## Range sync

`RangeReq` is the lazy body-fetch API. It exists so clients can sync headers
first, render the newest messages, and then request older bodies while the user
scrolls backward.

The request is scoped to one rotating DAG and uses an exclusive `(timestamp,
event_id)` cursor. The responder serves events and aligned blobs from its local
time index, up to `MAX_RANGE_PAGE_SIZE`. Callers must treat every returned event
as untrusted: the same `dag_insert_with_blobs()` body, parent, and RLN checks
still apply.

Range sync must not become a cross-DAG disclosure oracle. Keep requests scoped
to a DAG name, keep cursors exclusive, and keep page sizes bounded. A node that
is not synced does not serve range pages.

## P2P limits

Event graph protocol messages have item-count limits for expensive vectors:
`EventReq`, `EventRep`, `HeaderReq`, `HeaderRep`, `TipRep`, and `RangeReq` /
`RangeRep`. These limits are part of the DoS boundary and should not be raised
without a matching review of memory use and verification cost.

All event graph messages also pass through the network metering layer. Dedicated
per-message byte caps for event graph payloads are still future hardening work;
do not assume count limits alone bound encoded byte size for arbitrary future
payload fields.

## DarkIRC RLN counters

DarkIRC stores local RLN identities under `darkirc_account_*` and mirrors the
active identity under `darkirc_account_default`. Before creating a signal proof,
DarkIRC reserves the next per-epoch `message_id` with
`IrcServer::reserve_rln_message_id()`. The reservation is persisted before proof
creation.

This write order can burn a message slot if the process crashes after
reservation, but it prevents a restart from reusing a message ID and
self-slashing the identity. Do not move proof creation before counter
persistence. Do not copy `darkirc_account_default` without the matching account
tree unless the operator understands they are copying counter state and secrets.
