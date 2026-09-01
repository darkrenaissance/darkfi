# Design: evgr-resume-bug

## Context

The rotating event graph (`src/event_graph/`) keeps one DAG per hourly slot
(`hours_rotation: 1`) with a 24-slot retention window (`max_dags: 24`). Three
mechanisms populate a node's DAG:

1. **Initial sync** — `EventGraph::sync_impl` (via `dag_sync`): quorum-agreed
   tips → `HeaderReq(our_tips)` → `header_dag_insert` →
   `fetch_missing_events` (batched `EventReq` bodies). Strict: fails the round
   if any requested body fails to commit.
2. **Live gossip** — `ProtocolEventGraph::handle_event_put` (`src/event_graph/proto.rs`):
   validates one arriving event, then resolves its unknown ancestry with
   `fetch_parents`.
3. **Paginated range** — `RangeReq`/`fetch_page_with_blobs`. Exists, is served
   by peers, and is unused by `bin/app`.

The mobile app (`bin/app/src/plugin/darkirc.rs`) drives these as follows:

- The `dag_sync` plugin task performs the initial sync **once per process
  lifetime**, then parks forever in a loop that only re-notifies the UI of
  peer-count changes.
- `catch_up_sync` walks the remaining retention-window slots once, then `break`s.
  After it exits, **no task ever syncs a slot again** — including slots that
  rotate in later (`dag_prune_task` rotates `current_genesis` hourly, but
  nothing reconciles the new slot beyond gossip).
- Screen off/on toggles `P2P_OUTBOUND_SLEEP`/`P2P_OUTBOUND_ACTIVE` only.

The bug this change fixes: after a suspension, catch-up relies exclusively on
`fetch_parents` walks, which are best-effort, single-peer, and cover only the
lineages they descend. Anything not in a walked lineage is silently missing
forever (no reconciliation exists), and the user sees permanent holes in
conversation history.

This is not mobile-only. The same permanent-gap symptom has been observed with
laptop users who close the lid and later return: the OS freezes the whole
process (no mobile-style sleep hook, and desktop builds have no
`screen_changed` signal at all), sockets die, and on resume the client is in
exactly the state described above — current tips eventually arrive via
gossip, but mid-history branches are never fetched. Consequence for this
design: an explicit wake signal is a useful *additional* trigger, but repair
correctness cannot depend on one existing or firing. The peer-recovery and
periodic triggers below are therefore mandatory parts of the mechanism, not
hardening.

## Forensic evidence

All log excerpts below are from an Android logcat capture of the app
(taken 2026-08-31), sanitized: nicknames, channel names, message bodies, and
seed addresses are replaced with placeholders. Event ids are truncated
blake3 prefixes retained where needed to show DAG topology. Local times are
UTC+2; event timestamps are UTC.

### A. The suspension and resume

```
08-31 16:42:03.942 [10685] I net::refinery: No connections for 747s. GreylistRefinery paused.
08-31 16:42:03.957 [10685] D net::seedsync_session: SeedSyncSession::start_seed() [START]
08-31 16:42:03.957 [10685] I net::connector::connect: [P2P] Connecting peer [<seed-0>] via route [<seed-0>]
08-31 16:42:03.957 [10685] I net::connector::connect: [P2P] Connecting peer [<seed-1>] via route [<seed-1>]
08-31 16:42:09.268 [10685] I net::seedsync_session: [P2P] Connected seed [<seed-0>]
08-31 16:42:09.273 [10685] I net::seedsync_session: [P2P] Connected seed [<seed-1>]
08-31 16:42:09.405 [10685] I net::seedsync_session: [P2P] Disconnecting from seed [<seed-0>]
08-31 16:42:09.429 [10685] I net::seedsync_session: [P2P] Disconnecting from seed [<seed-1>]
```

747s without connections ⇒ last traffic at ~14:29:36 UTC. Notably, the entire
rest of the capture contains **zero** `plugin::darkirc2` INFO lines — the
plugin's `dag_sync` task logs `"Syncing newest event DAG..."`, `"Newest event
DAG synced successfully"`, etc. at info level on every sync attempt. Their
absence proves no sync path ran at resume; it had completed at cold start
(before the capture window) and was parked.

### B. The holey burst

At 16:42:16 (7 seconds after the seeds reconnected — consistent with ~100
sequential ancestry round-trips at ~70ms), ~110 events were committed in a
56ms burst and relayed to the UI. Received layers:

```
165 166 167 168 169 170 171 172 173 174 175 176 177 178 179 180 181 182
183 184 185 186 187 188 189 190 191 192
----------------------------------------- GAP (17 layers missing) -----
210 211 212
--- GAP (8) --- 221 --- 222-223 missing --- 224 --- 225 missing ---
226 227
----------------------------------- GAP (18 layers missing) ----------
246 --- 247 missing --- 248 249 250 ... 262
```

Message timestamps show the gap is real conversation time:

```
layer=192 t=1788186785188  ->  14:33:05 UTC   (received)
layer=210 t=1788186941479  ->  14:35:41 UTC   (received)
                                 ~156 seconds of conversation never arrived
```

From layer 263 onward, events arrive one at a time via live gossip
(`16:42:17` … `16:58`), i.e. the network path itself was healthy after
reconnect.

The burst shape (single 56ms commit, layer-ascending) matches
`fetch_parents` exactly: it buffers fetched events in a
`BTreeMap<u64 /*layer*/, Vec<…>>` and inserts them flattened in layer order
after the walk completes. One completed walk = one burst.

### C. Parent-chain analysis (why the holes are where they are)

Parsing `ev_id` + `parents[0]` from every relayed event and checking whether
each parent was itself ever relayed:

```
t=1788186785060  ev=6210eaa4  parent=0bbb2fc6   ok (received)
t=1788186785187  ev=ea7bc14a  parent=6210eaa4   ok (received)
t=1788186785188  ev=550bdc68  parent=ea7bc14a   ok (received)   <- L192
t=1788186941479  ev=4713d021  parent=5143546a   <<< NEVER RELAYED
t=1788186941573  ev=bab13f11  parent=4713d021   ok
t=1788186996457  ev=6545d7ff  parent=45dae6ba   <<< NEVER RELAYED
t=1788187005626  ev=79f7506f  parent=b715e06f   <<< NEVER RELAYED
t=1788187024199  ev=31d0461b  parent=44810e02   <<< NEVER RELAYED
t=1788187180594  ev=21522ad9  parent=97b2d5d6   <<< NEVER RELAYED
t=1788187203763  ev=baa051fd  parent=9ef0a12c   <<< NEVER RELAYED
...
```

The received set is **closed under parents only via events that were either
received or already local**; the "never relayed" bridging parents are
events whose bodies must be in `main_tree` (otherwise the children could not
have committed — see `dag_insert_inner`'s parent-body closure below), but
which produced no relay line. They are foreign-channel/DM traffic: darkirc
uses one shared DAG across all channels, and the app's `relay_events`
silently drops undecryptable privmsgs. The walked lineage therefore weaves
through other channels' events, and the missing conversation messages are
events on branches **no completed walk ever descended**.

### D. Ruled-out alternatives

- **Hourly rotation prune**: boundary at 14:00 UTC; the whole incident
  (14:29–14:42) is inside the 14:00–15:00 slot; `max_dags: 24` retention.
  Also `handle_event_put`'s pre-genesis cut (`event.header.timestamp <
  genesis_ts → continue`) never triggers inside a slot.
- **Publisher eviction**: `Publisher::notify` uses `force_send`, which evicts
  the *oldest* queued notification on overflow — but per-subscriber capacity
  is 1024 (`PUBLISHER_QUEUE_CAPACITY`) vs a ~110-event burst, and the loop
  was draining concurrently. Cannot produce 17 consecutive mid-burst drops.
- **UI-side dedup/deserialization drops**: zero `"Skipping duplicate seen
  message"` and zero `"Failed deserializing incoming Privmsg"` lines in the
  capture. The target channel is plaintext, so a message present in
  `main_tree` would have relayed. Hence the missing messages are genuinely
  absent from the app's DAG.
- **Local replay (`rescan`)**: `rescan_channel_history` iterates
  `order_events()` over the local `main_tree` only; it cannot fetch anything.

## Code path walkthrough

### 1. The parked sync task (`bin/app/src/plugin/darkirc.rs`, `dag_sync`)

```rust
loop {
    // ... wait for peers, run static_sync, then:
    let latest_ts = self.event_graph.current_genesis.read().await.header.timestamp;
    i!("Syncing newest event DAG ({latest_ts}) (attempt #{sync_attempt})");
    let sync_result = self.sync_dag_slot(latest_ts, fast_mode).await;
    match sync_result {
        Ok(()) => { newest_synced = true; break }   // <-- leaves the loop forever
        Err(e) => { e!("Failed syncing newest DAG ({e}), retrying..."); }
    }
}
// ...
loop {
    // Parked forever: only notifies the UI of connection changes.
    if let Err(err) = channel_sub.receive().await { continue }
    let peers_count = self.p2p.peers_count();
    self.notify_connect(peers_count, self.event_graph.is_synced()).await;
}
```

### 2. Wake only toggles outbound slots (`bin/app/src/plugin/darkirc.rs`)

```rust
let screen_changed_task = ex.spawn(async move {
    while let Ok(data) = screen_changed_recv.recv().await {
        // ...
        if screen_on {
            self_.set_outbound_connections(P2P_OUTBOUND_ACTIVE).await;
        } else {
            self_.set_outbound_connections(P2P_OUTBOUND_SLEEP).await;
        }
    }
});
```

No sync call anywhere on this path.

### 3. `catch_up_sync` exits permanently (`bin/app/src/plugin/darkirc.rs`)

```rust
pending = still_pending;
if pending.is_empty() {
    i!("Background catch-up complete; all older DAGs synced");
    break          // <- nothing ever syncs the *next* rotated-in slot
}
```

### 4. The only post-sync catch-up: gossip ancestry walks
(`src/event_graph/proto.rs`, `fetch_parents`)

```rust
// Only ever asks the single peer that sent the EventPut:
if self.channel.send(&EventReq(requested.clone())).await.is_err() { return false }
let Ok(rep) = self.ev_rep_sub.receive_with_timeout(timeout).await else {
    self.channel.stop().await;
    return false          // <- whole walk discarded, triggering event dropped
};
```

Single-peer, drop-on-failure, and — per `MAX_PARENT_FETCH_DEPTH`'s own doc
comment — explicitly not the intended mechanism for a node that is far
behind: "A node that's 1000+ layers behind should be using `dag_sync` rather
than relying on `EventPut` catch-up." The app never does.

### 5. Why a naive re-run of `dag_sync` is insufficient (`sync_impl`)

```rust
let missing: HashSet<blake3::Hash> = accepted
    .iter()
    .filter(|h| !slot.main_tree.contains_key(h.as_bytes()).unwrap_or(true))
    .cloned()
    .collect();
if missing.is_empty() {
    return Ok(())          // <- early return when we already hold all tips
}
```

At resume-before-gossip this is fine (our tips are stale, peers' tips are
missing). But if reconciliation fires after gossip has already delivered the
current tips, the early return skips the header/body phases and the gaps are
never repaired. Meanwhile the serving side is well suited to repair:
`fetch_headers_with_tips` returns **every header not reachable from the
requester's tips** (branch events included), layer-sorted (which is a valid
topological order because a parent's layer is always strictly lower than its
child's), capped at `MAX_HEADER_REP_HEADERS` (4096), and `handle_header_req`
whitelists revealed ids into `broadcasted_ids` so the follow-up body
`EventReq`s are admitted.

## Goals / Non-Goals

**Goals:**
- Any event that existed at peers while the app was suspended becomes
  fetchable again after resume, without a process restart.
- Repair works even when triggered late (after gossip has delivered tips).
- Repair also covers slots that rotate in while the app is long-lived
  (covers the `catch_up_sync`-exits gap).
- Observable: field logs state when a repair round ran and what it committed.

**Non-Goals:**
- Scrollback/pagination UI via `RangeReq` (separate change).
- Changing `fetch_parents` multi-peer fallback / retry semantics.
- Rotation/retention config changes; any p2p wire-protocol change.
- Recovering events no reachable peer still serves (out of any node's
  control once retention expires).

## Decisions

### D1: Repair primitive = header-sync + body-fetch, without the tip-quorum gate

Add a lib-level `EventGraph::dag_repair(dag_ts)` (in `src/event_graph/mod.rs`)
that reuses `sync_impl`'s machinery but:

- Skips the "missing tips" early return (always issues `HeaderReq(our_tips)`
  to all peers and inserts returned headers), because gap events are by
  definition not reachable from our tips — `fetch_headers_with_tips`
  excludes our tips' ancestors, so the response is exactly the unreached
  set.
- Runs `fetch_missing_events` afterward for bodies.
- Treats per-event commit failures leniently: log and continue, return
  `Ok` with a count (a single peer missing an RLN blob for one event must
  not fail the whole repair round; the strict behavior stays for initial
  sync where completeness is the contract).

Alternative rejected: calling existing `dag_sync` from the app — blocked by
the early return above. Alternative rejected: app-side `RangeReq` scan —
would need a cursor policy over `time_index` and duplicate the
header/body machinery; range sync is designed for ordered pagination, not
arbitrary-branch reconciliation.

### D2: Triggers — wake signal, peer recovery, periodic

In the app plugin:

- `screen_changed(screen_on=true)` and the `darkirc_start` slot schedule a
  repair round (debounced) — where such signals exist (mobile).
- A dedicated task watching `p2p.hosts().subscribe_channel()` schedules a
  repair round when peers transition 0 → ≥1. This is the primary trigger for
  suspend/resume on machines where no in-app wake signal exists or fires:
  laptop lid-close/resume freezes the process wholesale, and the first
  observable symptom inside the app is peers dropping to zero and later
  reconnecting.
- A periodic timer re-runs repair for the current slot every
  `REPAIR_INTERVAL` (default: 15 min, constant — matching the file's
  existing "TODO: these should be configurable" style) so a failed round is
  retried and rotated-in slots are reconciled without any event at all.
  This is also the backstop for resume shapes that produce no clean 0 → ≥1
  edge (e.g. connections that die and return one at a time while others
  stay up, or a resume racing the channel subscription).

Debounce/coalescing: a single in-flight guard (see D3) collapses
simultaneous triggers; triggers arriving during a round are latched, not
dropped.

Alternative rejected: re-arming the parked `dag_sync` loop by feeding it
channel events — the parked loop's contract is notify-only; overloading it
mixes initial-sync retry semantics with repair semantics. A separate
`repair_sync` task mirrors `catch_up_sync`'s shape and keeps the state
machines disjoint.

### D3: Concurrency and `synced`-flag discipline

- `dag_repair` must not run concurrently with an initial `dag_sync`/
  `sync_selected` for the same slot or with another repair round: guard with
  an app-side `AtomicBool`/mutex ("repair in flight"), since both paths
  terminate in `dag_insert_with_blobs` which is idempotent for known events
  but the strict variant's error contract differs from repair's lenient
  one.
- The `synced` flag is only ever set (never cleared) by the initial sync;
  repair observes it (must not run before initial sync completes — same
  gate `handle_event_put` uses) and must not flip it, so live gossip
  ingestion is never blocked by a repair round.

### D4: Observability

- Info log on repair round start (slot id, trigger source) and completion
  (headers gained, bodies committed, events skipped+why).
- Existing warn/error logs on peer-side serving refusals
  (`"declining to serve event ... - missing blob"`) remain the primary
  diagnostic for unrecoverable events.

## Risks / Trade-offs

- **`src/event_graph/mod.rs` is a security-critical subsystem.** D1 adds a
  read-heavy path reusing existing insert primitives; no validation,
  RLN, or pruning logic is altered. Flagged for explicit review, and the
  apply phase must run `make test` (event_graph tests are proof-dependent).
- **Wake traffic spike**: a repair round after a long suspension transfers
  all unreached headers/bodies for the current slot. Bounded by protocol
  limits (`MAX_HEADER_REP_HEADERS` 4096, `MAX_EVENT_REQ_IDS` 128/batch,
  `MAX_RANGE_PAGE_SIZE` 100). On mobile this is the same volume the initial
  sync would have paid; debouncing prevents amplification from multiple
  triggers.
- **Truncation**: `fetch_headers_with_tips` keeps the lowest 4096 headers
  when overflowing — a node very far behind may need multiple repair
  rounds to converge (each round advances the frontier). Acceptable;
  periodic retry converges.
- **Quorum caveat inherited from tips collection**: repair itself doesn't
  depend on the 2/3 tip quorum (that gate only feeds the early return we
  skip), but branch events known to a *minority* of peers are only
  recoverable while at least one such peer is connected and serving.
- **Known unrecoverable case**: events whose blobs no serving peer retains
  are skipped and logged, not fatally failed. The UI may still show gaps
  for those; this change guarantees retry + visibility, not impossible
  recovery.
