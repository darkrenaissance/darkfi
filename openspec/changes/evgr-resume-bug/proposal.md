# Proposal: evgr-resume-bug

## Why

A mobile client (bin/app darkirc plugin) that suspends its P2P stack (screen off →
`P2P_OUTBOUND_SLEEP`) and later resumes receives only a holey subset of the chat
history created while it was away. Verified from a production logcat capture: after a
~13 minute suspension, the app received a single ~110-event burst containing
contiguous runs with permanent gaps (~156 seconds of conversation never arrived, plus
several smaller gaps), and because no reconciliation mechanism ever runs again, those
gaps persist for the lifetime of the process. Users experience silently missing
messages in the middle of conversations. The same failure has also been observed
with laptop users closing the lid and later returning: the process is frozen by
OS suspend rather than a mobile sleep hook, so the defect is not mobile-specific —
any long-lived client whose connectivity drops out for a period (screen sleep,
lid close, OS suspend, network churn) can accumulate permanent gaps, and we
cannot assume the machine stays running and connected for the duration of the
session. This is not the hourly DAG rotation
(`hours_rotation: 1`, `max_dags: 24`) — the entire incident occurred inside a single
rotation slot; the root cause is that DAG sync runs exactly once per process
lifetime, and post-resume catch-up relies solely on best-effort gossip ancestry
walks (`fetch_parents`) which cover only the lineages they happen to descend.

Full forensic evidence (sanitized log excerpts, parent-chain analysis, code path
walkthrough) is captured in `design.md`.

## What Changes

- Add resume-triggered reconciliation: when connectivity is re-established after a
  suspension (an explicit wake signal where one exists, or outbound peers
  transitioning 0 → N), the app re-runs a DAG sync of the current rotation slot
  (`EventGraph::dag_sync`, the existing quorum-tips → header sync → body fetch
  path) so that any events missed during the outage are fetched.
- Add a periodic background reconciliation loop that re-syncs the current slot at a
  slow cadence, repairing gaps from failed best-effort walks even when no explicit
  wake event is observed (e.g. laptop lid-close/resume where the process is frozen
  with no in-app signal, or connection churn without screen state change).
- Guard the resync so it cannot run concurrently with an in-flight initial sync or
  with itself, and so it observes (does not reset) the `synced` flag semantics that
  gate live `EventPut` ingestion.
- Instrument the failure mode: when a `fetch_parents` walk fails or is truncated,
  and when a resume resync commits previously-missing events, emit an info-level
  log line so the repair is observable in the field.
- Non-goals (explicitly out of scope, candidate follow-ups): scrollback/pagination
  UI via `RangeReq`/`fetch_page`; changing `fetch_parents` multi-peer fallback or
  its drop-on-failure semantics inside `src/event_graph/proto.rs` (security-critical
  shared subsystem; would be its own change with review); any modification to the
  rotation or retention configuration.

## Capabilities

### New Capabilities
- `event-graph-resume-sync`: Behavior contract for repairing missed rotating-DAG
  events after a connectivity interruption on long-lived clients: triggers
  (wake / peer-count recovery / periodic), the reconciliation mechanism
  (re-running a slot sync), concurrency and idempotence requirements, and
  observability requirements for repairs.

### Modified Capabilities
<!-- None: no existing specs to modify (openspec/specs/ is empty). -->

## Impact

- `bin/app/src/plugin/darkirc.rs` — the `dag_sync` task lifecycle (currently
  run-once-then-park), the `screen_changed` / `darkirc_start` handlers, and
  `catch_up_sync`; new resume-reconciliation task and periodic loop.
- `src/event_graph/mod.rs` — only if a public API seam is needed to re-enter
  `sync_impl` safely (e.g. exposing whether a slot sync is in flight); the
  existing `dag_sync`/`sync_selected` entry points are expected to suffice.
  Any edit here is in a security-critical subsystem and gets explicit review.
- No changes to consensus serialization, ZK circuits, RLN logic, the wasm host
  ACL, or the p2p wire protocol.
- Risk surface: resync traffic volume on wake (bounded by `MAX_*` page/request
  limits already enforced by the protocol), and interaction between resync and
  live gossip ingestion (both terminate in `dag_insert_with_blobs`, which is
  already idempotent for known events).
