## 1. Lib: repair primitive (`src/event_graph/mod.rs`)

- [ ] 1.1 Add `EventGraph::dag_repair(dag_ts)` reusing `sync_impl`'s
       peer-query/body-fetch machinery but skipping the missing-tips early
       return (always issue `HeaderReq(our_tips)` to all peers, insert
       returned headers via `header_dag_insert`, then run
       `fetch_missing_events`). Verify with a unit test in
       `src/event_graph/tests.rs`: node A and B hold a DAG where B is
       missing a mid-history branch (not an ancestor of B's tips) but holds
       all tips; `dag_repair` commits the branch on B.
- [ ] 1.2 Make per-event commit failures in the repair body-fetch phase
       lenient (log + skip + count, return `Ok` with counts) instead of the
       strict `DagSyncFailed` used by initial sync. Verify with a unit
       test: one peer serves a header but no blob for one event; repair
       commits the rest and reports the skip.
- [ ] 1.3 Run `make test` (proofs + contracts must be prebuilt) and confirm
       the full `event_graph` test suite passes, including existing
       `dag_sync`/`fetch_missing_events` strict-path tests (behavior of
       initial sync unchanged).

## 2. App: repair task and triggers (`bin/app/src/plugin/darkirc.rs`)

- [ ] 2.1 Add a `repair_sync` task mirroring `catch_up_sync`'s shape: waits
       on an in-flight guard + trigger latch, requires
       `event_graph.is_synced()`, calls `dag_repair` for
       `current_genesis`, never touches the `synced` flag. Verify by code
       review against design D3 and by a desktop debug run
       (`make compile-dev`) showing the repair start/complete log lines.
- [ ] 2.2 Wire triggers: `screen_changed(screen_on=true)`, `darkirc_start`
       slot, a `subscribe_channel` watcher firing on peers 0→≥1, and a
       periodic timer (`REPAIR_INTERVAL`, 15 min constant). Triggers during
       an in-flight round are latched, not dropped. Verify with a debug
       build log showing coalescing (one round despite three simultaneous
       triggers) and periodic rounds firing idle.
- [ ] 2.3 Emit observability per spec: round start (slot id + trigger
       source) and completion (headers gained, bodies committed, skipped
       count). Verify the log lines appear in a debug run.

## 3. End-to-end verification

- [ ] 3.1 Reproduce the incident shape locally (models both mobile screen-off
       and laptop lid-close resume): run two desktop nodes +
       seed, suspend node B's connections entirely (stop outbound), generate
       traffic on a branch B will not receive via gossip, resume B, and
       verify B's history converges to A's within one repair round
       (diff the two nodes' `order_events()` output). Repeat with B's
       process itself frozen (SIGSTOP) across the gap to model OS suspend
       with no wake signal, verifying the peer-recovery/periodic triggers
       fire the repair.
- [ ] 3.2 Verify no regression on resume cost: with no gap (B current),
       a repair round issues only the header exchange and fetches zero
       bodies (log shows 0 committed), and live `EventPut` ingestion is
       never blocked during a round (send traffic through B while a round
       runs; messages relay to the UI).
- [ ] 3.3 Compile checks: `make compile-dev` (desktop) and
       `make compile-apk` (android) both succeed; `make clippy` clean.
