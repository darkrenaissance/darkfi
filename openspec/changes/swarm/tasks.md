# Tasks: swarm overlay for swarm rendezvous

## 1. Review checkpoint and module foundation

- [ ] 1.1 Obtain human review of the `src/net/swarm/` design, resolved-target
  dial work, and Cargo activation of existing `blake3`/`kvdb-overlay`;
  stop for separate supply-chain review if any new dependency, dependency
  source, `build.rs`, or proc-macro is needed.
- [ ] 1.2 Add the gated `swarm` feature and `src/net/swarm/` module structure
  without enabling existing binaries; verify default, `net`, and `swarm`
  feature combinations with `make check` and clean `make clippy`.
- [ ] 1.3 Implement validated role, fixed overlay identity, protocol/local
  maxima, timeout, source-policy, persistence, and serving settings; add zero,
  over-maximum, conflicting-identity, and invalid-role tests, then run
  `make test`.

## 2. Canonical descriptors

- [ ] 2.1 Implement exact version-1 descriptor validation/encoding with manual
  domain, lengths, big-endian integers, 32-byte app-name bound, and secret shape; add app-name,
  flag/secret, UTF-8 byte, and excluded-version-field tests, then run
  `make test`.
- [ ] 2.2 Implement BLAKE3 `SwarmId` and the normative darkirc byte/hash golden
  vector plus bound-field divergence and patch-equivalence tests; run
  `make test`.
- [ ] 2.3 Add `OsRng` private-secret generation and secret-safe debug/error
  behavior; verify overlay-facing values contain only derived IDs with
  `make test`.
- [ ] 2.4 Prove descriptors and derived IDs are rendezvous inputs only: knowing a
  private secret/ID and passing network compatibility MUST NOT grant application
  authorization. Add compatible-but-app-unauthorized and ID-only access tests,
  then run `make test`.

## 3. Shared networking prerequisites

- [ ] 3.1 Add bounded local version features to `net::Settings`, default empty,
  enforcing 10 external addresses, 10 features, 64-byte node ID, 32-byte app
  name, 1,024-byte URL, 32-byte feature name, and 32-byte semver
  prerelease/build plus duplicate/version checks; add every boundary test and
  run `make test`.
- [ ] 3.2 Plumb features into `VersionMessage` without changing compatibility;
  check complete outgoing version and verack against their maxima and implement
  bounded inbound decoders for both that check every string/vector—including
  semver strings—before reservation/allocation while preserving valid wire
  bytes. Add golden compatibility, huge declared count/string, overlong
  element, roundtrip, and combined-oversize tests. Generate each P2p node ID with
  `OsRng`; test overlay/swarm/restart independence and no persistence/reuse,
  then run `make test`.
- [ ] 3.3 Record focused human review that shared-net changes alter neither
  framing, magic, version compatibility, nor manual/seed/inbound reload
  behavior, and that transport changes are limited to the explicit
  validated-resolved-target path in 3.4.
- [ ] 3.4 Add a checked `ValidatedDialTarget` path that resolves clearnet once,
  rejects prohibited IPv4/IPv6 ranges outside explicit local-test mode, dials
  the exact socket without DNS re-resolution, preserves hostname only for TLS,
  and rejects arbitrary proxy-remote clearnet DNS. For Tor/I2P, make the exact
  socket the trusted locally configured proxy, never ad-selected; pass canonical
  hidden names only inside proxy negotiation/TLS and never local DNS. Add a
  pre-start `ManualSession` target API carrying connect URL plus resolved socket
  plus a two-phase pre-start plan that cancels verified targets at a monotonic
  switch and activates fresh targets without reload. Add API-state/phase-cancel,
  IPv4/IPv6 range, cached-no-DNS, rebinding, TLS-name, trusted-proxy socket,
  hidden-service no-local-DNS, proxy override/bypass, missing proxy, and
  production-lilith policy tests, then run `make test` and obtain focused
  transport/privacy review.
- [ ] 3.5 Audit the full advertised-target path—URL/scheme/host/port, empty,
  bounded-multiple, and over-16 DNS results, target construction, every accepted
  transport, proxy negotiation, TLS, timeout/cancellation, and compatibility—and
  remove or bypass all `unwrap`, `expect`, panic, unchecked indexing, and
  unimplemented branches. Budget/classify every DNS answer and select at most
  one exact socket per URL. Reject unaudited schemes before dialer construction;
  add arbitrary-input and no-unwind tests for every accepted scheme, then run
  `make test` and `make clippy` and obtain focused transport review.

## 4. Bounded correlated wire protocol

- [ ] 4.1 Implement nonempty `SwarmAd` with exact ID, visibility, ad ID,
  lifetime, 1..=32 address, 1,024-byte URL, shareable-scheme, and 65,536-byte
  message limits plus fixed command/field order and visibility values; add
  serialization/golden-command and every-boundary test, then run `make test`.
- [ ] 4.2 Implement fresh 16-byte request IDs, fixed 65-byte version/last-key/
  terminal-key cursors, direct lookup messages, fixed command/field order,
  defined error values 0..=3, and a 10-second default/60-second maximum local
  request timeout. Derive canonical response keys and validate strict ordering,
  uniqueness, `(last, terminal]` windows, exact next-last, unchanged terminal,
  and no next cursor on empty pages; deduplicate across pages under item/page
  caps. Add roundtrip, golden-command, malformed/key-order/window/terminal/
  empty-advance cursor, duplicate-page, invalid error-value, wrong-ID/type,
  local timeout/late response, and unsolicited-response tests; run `make test`.
- [ ] 4.3 Implement optional public enumeration messages with request
  correlation, explicit-enable/default-disabled policy, 256-ID/16,384-byte pages,
  no role privilege, and live normalized public-record filtering; add omitted-
  config default-disabled, explicit-enabled, non-public-only omission, mixed
  records, visibility transitions, attacker public-relabel, pagination, and byte-
  bound tests, then run `make test`.
- [ ] 4.4 Implement a 32-entry per-channel pending-request map with timeout and
  disconnect cleanup; add concurrent identical lookup and saturation tests,
  then run `make test`.
- [ ] 4.5 Define hard `MAX_BYTES` and metering for every swarm message, checking
  encoded bytes/counts before variable allocation/store work; add metering and
  small-message amplification tests, then run `make test`.
- [ ] 4.6 Audit every attacker-controlled swarm/version/verack decoder for
  fallible checked reads and absence of `unwrap`, `expect`, explicit panic,
  unchecked slicing/indexing, or allocation before declared-size validation;
  add truncation-at-every-byte, hostile count/length, arbitrary-input,
  no-unwind, and bounded-allocation tests for every message, then run
  `make test`.

## 5. Passive advertisement stores

- [ ] 5.1 Implement bounded in-memory address/public indexes, canonical URL
  keys, one normalized visibility/expiry record per address, public membership
  iff any live record is public, stateless last/terminal-key traversal, and
  deterministic eviction; add absent-address retention, same-address visibility
  transition, mixed record, cap, ordering, expiry, continuous mutation, no-
  restart progress, insertion-after-terminal, responder terminal-change/non-
  advancing cursor, and cursor-key validation tests, then run `make test`.
- [ ] 5.2 Implement monotonic expiry with a 7,200-second receive/author default,
  86,400-second hard maximum, receiver clamp, unchanged relay wire lifetime, and
  duplicate handling that never refreshes or requeues; add paused-clock,
  default/max/clamp, relay-preservation, duplicate/fresh-ID, and expiry tests,
  then run `make test`.
- [ ] 5.3 Implement protected seen-ID retention through local address expiry
  plus 86,400 seconds, a 256-default/1,024-maximum general quota per swarm, and
  32-default/256-maximum remote-inaccessible local-author swarm partitions of
  256 slots each within the global cap. Use checked capacity arithmetic and
  validate persisted general/local partitions separately without double count;
  retain stopped-swarm partitions until their IDs expire. Reject fresh ads or
  serving transitions before mutation/network activity when the applicable pool
  is full. Add one-swarm, distributed-swarm, global, sequential serving churn,
  local-reserve, stopped-swarm last-ID release/resume race, cadence-window,
  local-reserve wire/RPC/status/metric/telemetry exclusion, positive restart
  fixture where reserve IDs fit only when excluded from general accounting,
  protected-ID, cross-swarm ad-ID reuse, expired-admission, and replay tests,
  then run `make test`.
- [ ] 5.4 Implement dual-clock ad-address expiry plus restart-safe dedup:
  restore bounded remaining address lifetime and persist seen-ID deadlines on a
  monotonic epoch with a 300-second default/600-second maximum atomic checkpoint.
  Use `u64` ticks, compare before checked subtraction, expire equality, reject
  deltas over 173,400, clamp valid remainder to 172,800, and checked-add the new
  `Instant` deadline. Atomically restore records/new epoch without a full reset.
  Add clean restart, equality/underflow/maximum/overflow, sub-checkpoint crash
  extension, downtime, repeated restart, corrupt/missing epoch, interrupted
  atomic conversion, and paused-clock healthy checkpoint-at-300-seconds tests.
  Inject checkpoint delay/failure and prove fresh admission/authoring fail closed
  by 600 seconds while bounded reads remain; then run `make test`.
- [ ] 5.5 Implement `kvdb-overlay` trees and atomic address/public/dedup/metadata
  updates; fail startup when configured pool/reserve caps cannot hold valid
  persisted seen state and rebuild/verify the public index from normalized
  records. Add restart, mixed visibility, reduced-cap/reserve failure, stateless
  index, checkpoint, rollback-before-ID-commit, and interrupted-write tests,
  proving accepted seen/address/index state commits before relay eligibility;
  then run `make test`.
- [ ] 5.6 Bound and fallibly decode persisted keys/values. Fail startup on
  malformed/unverifiable seen-ID, quota/reserve, or epoch state; quarantine/
  rebuild only address/public-index state when replay/accounting remains intact.
  Test corrupt lengths, URLs, IDs, each authoritative class, and metadata with
  `make test`.
- [ ] 5.7 Keep store interfaces source-free and dialer-free; add a dial spy
  proving accept, store, relay preparation, query, replay, expiry, saturation,
  and restart never connect to advertised targets, then run `make test`.

## 6. Protocol roles, relay, and work accounting

- [ ] 6.1 Register `ProtocolSwarm` on `SESSION_DEFAULT` only; verify dispatch on
  ordinary inbound/outbound/manual/direct channels and absence on seed/refine
  channels with `make test`.
- [ ] 6.2 Implement ad validation, store admission, bounded relay queue,
  duplicate suppression, source exclusion by ephemeral channel ID, and
  unchanged identifying contents; enqueue relay only after atomic seen/quota/
  address/index commit. Add commit failure, rollback replay disclosure,
  malformed, gossip-loop, saturation, fanout, and exclusion tests, then run
  `make test`.
- [ ] 6.3 Implement direct/public query handlers over ordered indexes and
  correlated pending requests; direct lookup ignores visibility and enabled
  enumeration grants no role privilege. Add continuous-mutation bounded progress,
  terminal-key, non-snapshot omission/addition, disabled-public, transient
  requester, no-cross-swarm, and unsolicited-response tests, then run
  `make test`.
- [ ] 6.4 Implement the exact per-channel message/work rates and
  response-byte budget plus configured queue, global semaphore, page, candidate,
  previously-compatible retry/index, local-author partition, active-swarm,
  concurrent-attempt, and shutdown defaults/maxima plus request timeout,
  configured peer, bind/external address, and
  inbound/outbound/manual/total channel, dial concurrency/rate,
  resolution-total, and per-destination maxima from `swarm-overlay`; reject
  over-maximum config, delete channel accounting on disconnect, and add each
  saturation/strict-penalty test before `make test`.
- [ ] 6.5 Implement persistent/transient behavior with exact
  `swarm-ad-store`, no feature privilege, durable versus bounded memory, no
  transient authoring, and overlay-only persistence scope; add forged-feature
  and equal-state response tests, then run `make test`.

## 7. Fresh-instance ordinary-peer bootstrap

- [ ] 7.1 Implement bounded endpoint-only cache records containing the exact
  successful connect URL/resolved endpoint pair, with 256-record,
  262,144-file-byte, 1,024-URL-byte, shareability, egress, and atomic replacement
  limits; add malformed/oversized/truncated/content tests, then run `make test`.
- [ ] 7.2 Cache only a completed ordinary channel's actual validated endpoint
  after it exposes `swarm-ad-store`; never cache `ext_send_addr` or another
  advertised address and persist no feature, ID, ad, query, or mapping. Add
  malicious external-address and file-inspection tests, then run `make test`.
- [ ] 7.3 Implement cached-peer stage using a fresh overlay `P2p` with
  empty `Settings.peers`/`Settings.seeds` and install revalidated cached sockets
  through the pre-start manual-target API; on timeout fully stop/discard it,
  resolve configured peers once, and install them into a fresh stage. Do not use
  `P2p::reload()` or DNS-resolve a cached hostname.
- [ ] 7.4 Add stage-order, cleanup, timeout, no-overlap, no-seed-session, cached
  success, and configured fallback integration tests; run `make test`.
- [ ] 7.5 Test that failed cached app/protocol/store startup leaves no task or
  state before the configured stage and that overall bootstrap remains bounded;
  run `make test`.
- [ ] 7.6 Implement transient overlay stop independently from swarm registry
  lifetime. Default to session-bound retention with no lookup/join-triggered
  stop; allow immediate teardown only via explicit reduced-privacy policy with
  timing warning and deterministic stop after every caller-visible success,
  empty, error, timeout, or cancellation outcome but not join's internal lookup;
  reject stop while persistent store/gossip or serving-ad duties remain. Add
  default-no-trigger, all terminal outcomes, internal-phase retention,
  same-operator correlation, later reconnect, and swarm-survival tests with
  `make test`.

## 8. Registry-owned swarm lifecycle and source attempts

- [ ] 8.1 Implement per-ID `Initializing`, `Joining`, `Joined`, `Serving`, and
  `Stopping` ownership with serialized same-ID transitions and concurrent
  different-ID operation; add legal/duplicate transition tests, then run
  `make test`.
- [ ] 8.2 Implement full-ID paths and isolated settings, hosts, refinement,
  datastores, app state, and shutdown ownership; add cross-swarm isolation and
  delete tests, then run `make test`.
- [ ] 8.3 Implement the fallible initializer returning typed app state plus
  shutdown; retain a type-erased `Arc` and hook in the registry so caller-handle
  drop cannot end app state. Test pre-start ordering, drop ownership, and
  initializer failure with `make test`.
- [ ] 8.4 Keep overlay candidates as ephemeral typed original-URL/resolved-socket
  targets through the swarm pre-start manual connector; do not insert URL-only
  host/refinery state before compatibility. Drop failures, persist only after
  compatible ordinary channel, and re-resolve/revalidate every later outbound/
  retry/refine attempt. Bound the persisted-compatible index at 64 default/256
  maximum; reservoir-sample across every URL returned by the terminal traversal
  through completion or its fixed 16-page cap. Independently shuffle both tiers
  and DNS answers. Split candidate-preparation resolution time equally, then
  install a two-phase plan that cancels verified dialing at the midpoint of
  remaining dial time; verified also consumes at most half the attempts. Add
  source-cardinality, reservoir, lexical/hash/insertion/DNS-order grinding,
  resolution/dial-time starvation, phase cancellation, DNS-query-count/exact-
  socket, local/private/reserved, rebinding, repeated-victim, proxy-DNS, wrong-
  magic/app/version, temporary-direct, seed/refine-without-peer, unreachable,
  disconnect-race, and test-only injected-RNG tests; run `make test`.
- [ ] 8.5 Enforce channel ownership separation: no overlay stream transfer,
  swarm re-handshake, or swarm-tag multiplexing, even when the answering
  overlay peer also serves the swarm; add structural and same-operator tests,
  then run `make test`.
- [ ] 8.6 Implement overlay-only, static-only, and combined attempts with
  explicit source activation and per-attempt/overall deadlines; add policy and
  compatibility tests, then run `make test`.
- [ ] 8.7 Implement overlay-then-static as complete overlay-attempt rollback
  followed by a fresh static-configured `P2p` and repeated initializer—without
  manual/seed reload. Add state-leak, ordering, repeated-initializer,
  static-success, and aggregate-failure tests, then run `make test`.
- [ ] 8.8 Add fault injection at construction, initializer, insertion, start,
  channel wait, timeout, cancellation, and shutdown; verify complete rollback
  leaves other networks undisturbed with `make test`.
- [ ] 8.9 Implement silent idempotent leave and explicit retain/delete policy;
  add repeated leave, join/leave race, retained rejoin, and isolated deletion
  tests, then run `make test`.
- [ ] 8.10 Implement swarm shutdown ordering: stop authoring, cancel attempts,
  stop every swarm despite errors, then overlay; add finite concurrent teardown
  and failing-hook tests, then run `make test`.

## 9. Initial serving, controlled recreation, and authoring

- [ ] 9.1 Implement separate bind/external serving settings, persistent-role
  validation, atomic local-author partition allocation before initialization/
  listener/authoring, and pre-start listener configuration. Release a newly
  allocated empty partition on pre-author failure; retain nonempty partitions
  after stop. Add first-server-with-no-peer, reserve exhaustion/churn, forwarded
  external endpoint, malformed field, and transient rejection tests, then run
  `make test`.
- [ ] 9.2 Implement listener-ready serving completion and optional subsequent
  source discovery without requiring a peer handshake; verify no partial server
  remains after bind/initializer failure with `make test`.
- [ ] 9.3 Implement joined-to-serving as serialized full stop/recreate with a
  serving-configured `P2p` and repeated initializer, never inbound reload; add
  success, retained-state, bind failure, and no-partial-author tests, then run
  `make test`.
- [ ] 9.4 Validate advertised endpoints separately, emit explicit warning on
  local cross-swarm reuse, and avoid automatic Tor/I2P provisioning claims;
  add bind/external distinction and reuse tests, then run `make test`.
- [ ] 9.5 Implement one author task with a fixed non-configurable 30-minute
  base interval, independent uniform ±10-minute
  `OsRng` jitter, fresh `OsRng` ad IDs, shuffled swarm order, and bounded
  two-hour-default/24-hour-maximum lifetime and addresses; add default/clamp,
  config-rejection, reserve-capacity, and injected clock/RNG tests, then run
  `make test`.
- [ ] 9.6 Prove cadence-only behavior: initialize, listener ready, peer connect,
  recreate, overlay connect, and stop never emit immediately; only cadence
  ticks author and stop ceases future ads without withdrawal. Run `make test`.

## 10. Lilith persistent overlay

- [ ] 10.1 Add optional lilith overlay config and explicit `swarm` feature with
  ordinary peers, separate accept/external addresses, production local-test
  egress disabled, strict policy, and bounded limits; add
  overlay-only, inbound-only, mixed, prohibited-local-target, malformed, and
  omitted tests, then run `make test`.
- [ ] 10.2 Start lilith with the durable passive store and no ad-refinery/dial
  path; add direct ordinary cold-start, persisted expiry, dedup saturation,
  per-swarm quota, zero local-author partitions, checked monotonic-epoch
  remainder restart, equality/overflow/reduced-cap/epoch startup failure, two-
  hour stale-address clamp, rollback-before-ID-commit replay, forward-clock, and
  dial-spy tests, then run `make test`.
- [ ] 10.3 Implement aggregate-only status RPC for listener, aggregate
  connections, capacities, address/dedup, per-swarm-quota/checkpoint failures,
  eviction, expiry, and rejection; prove peer/advertised addresses, IDs, sources,
  per-peer data, and query mappings are absent with `make test`.
- [ ] 10.4 Keep overlay and legacy instances isolated in settings, policy,
  registry, paths, failures, and shutdown; add mixed and independent-failure
  tests, then run `make test`.
## 11. Multi-node abuse, lifecycle, and privacy verification

- [ ] 11.1 Add local cold-start tests for cached ordinary success, fresh
  configured fallback, descriptor lookup, app initialization, and ordinary
  swarm join without per-swarm overlay configuration; run `make test`.
- [ ] 11.2 Add first-server creation followed by cadence ad, client lookup, and
  ordinary join, proving a new swarm requires no preexisting peer; run
  `make test`.
- [ ] 11.3 Add poisoned/stale tests proving stores/lilith never dial targets,
  two-hour defaults reduce stale retention, CSPRNG ordering defeats lexical
  prefix grinding, joining rejects incompatibility fallibly, relays are not
  blamed, and static fallback remains usable; run `make test`.
- [ ] 11.4 Add concurrent abuse tests for oversized fields/URLs, query/pending
  floods, fresh-ID floods, protected-set saturation, replay loops, relay fanout,
  one-swarm/distributed saturation, local-author reserve, terminal-cursor
  mutation/duplicate/empty-page churn, durable-write/checkpoint pressure,
  candidate reservoir/order/time-budget grinding, dial concurrency/rate, per-
  destination repetition, DNS resolution totals, and victim reflection; verify
  every bound and strict penalty with `make test`.
- [ ] 11.5 Add lifecycle tests for initializer failure, caller-handle drop,
  duplicate joins, seed/refine filtering, cancellation, source reconstruction,
  serving recreation, concurrent leave, retained rejoin, isolated delete, and
  failing shutdown hooks; run `make test`.
- [ ] 11.6 Add artifact/privacy tests proving no protocol-added stable node/
  signing identity across overlay/swarms (excluding disclosed endpoint reuse),
  non-public omission, caches/stores free of queries/sources, scoped swarm and
  transport persistence, explicit ID-to-endpoint visibility but no source-peer/
  authorship mapping; run `make test`.

## 12. Pilot, documentation, and gates

- [ ] 12.1 Add default-off darkirc pilot using registry-owned app state and
  explicit overlay-then-static reconstruction while preserving current static
  behavior; add overlay success, fresh static fallback, repeated initializer,
  and complete failure tests, then run `make test`.
- [ ] 12.2 Add bounded aggregate pilot metrics for lookup latency, stale/poisoned
  failures, per-swarm/global remote dedup pressure, checkpoint failures, page
  completion/mutation omissions, and bootstrap/source fallback. Expose no local-
  author reserve occupancy/use/failure timing. Prove schemas contain no peer,
  query mapping, private ID, local-author fact, or secret with `make test`.
- [ ] 12.3 Write deployment/migration guidance covering ordinary peers versus
  seeds, staged reconstruction, initializer reruns, serving recreation downtime,
  bind versus external endpoints, ID/query disclosure, unsigned poisoning,
  gossip/store visibility of ID-to-endpoint mappings, egress/DNS/proxy policy,
  reflection budgets, per-swarm/global dedup saturation and local reserve,
  monotonic checkpoint extension and rollback-before-commit replay limitation,
  two-hour TTL defaults, mutation-tolerant non-snapshot pagination, randomized
  reservoir/time-partitioned candidate tiers, static fallback, endpoint
  reuse, control/data channel separation, session-bound default versus reduced-
  privacy immediate teardown and its timing correlation, transient reconnect,
  transport state, Tor/I2P limitations, rollback, and absent auth/PIR/global-
  observer guarantees. Record endpoint-reuse enforcement and query-peer privacy
  budgets as separate follow-up change scopes, not implementation in `swarm`.
- [ ] 12.4 Run and resolve all required gates without weakening tests/lints:
  `make fmt`, `make`, `make clippy`, `make test`, and `make check`.
- [ ] 12.5 Invoke `@anon-security-review` on the complete implementation diff,
  record the verdict, and treat FAIL as blocking; resolve or escalate findings.
- [ ] 12.6 Obtain final human patch review of shared `src/net`, swarm, lilith,
  and darkirc changes, explicitly covering attacker-induced dialing, exact
  direct/proxy route semantics, complete candidate-pipeline panic
  removal, reservoir/time-partition fairness, replay checkpoint arithmetic/
  atomicity/rollback limits, local-author partition metadata, source
  reconstruction, listener recreation, state ownership, dependency activation,
  and test evidence. Broader rollout/deprecation is a separate change.
