# Tasks: swarm overlay for subnet discovery

## 1. Module foundation

- [ ] 1.1 Create `src/swarm/` module skeleton (`mod.rs`, `settings.rs` with
  `SwarmSettings` for overlay + role configuration), export it from the
  `darkfi` crate, and verify `make` compiles with `make clippy` clean.

## 2. SubnetId derivation (D2, spec: swarm-overlay)

- [ ] 2.1 Implement `SubnetDescriptor` (app_name, magic_bytes,
  version_constraint, optional secret) with canonical serialization and
  BLAKE3 `SubnetId` derivation; verify unit tests pass: identical
  descriptors yield equal ids, any field divergence yields different ids,
  secret-bearing ids are the same length and format as public ones.
- [ ] 2.2 Add a pinned-descriptor constructor from an existing
  `{app_name, magic_bytes, version}` network triple; verify a golden-value
  unit test fixes the darkirc triple's id so all deployments agree.

## 3. Minimal `src/net` additions (security zone: additive only)

- [ ] 3.1 Plumb a features list from `net::Settings` into the
  `VersionMessage` handshake (currently hardcoded to `vec![]` in
  `protocol_version.rs`); verify a version-exchange test round-trips the
  feature and that the diff contains no other `src/net` changes (flag this
  diff explicitly in the change notes for review).
- [ ] 3.2 Verify existing net behavior is unchanged:
  `cargo test -p darkfi --release --all-features net` passes and
  `make clippy` is clean.

## 4. Overlay wire messages (D4)

- [ ] 4.1 Define `SubnetAd`, `GetSubnets`, `Subnets`, `GetSubnetAddrs`,
  `SubnetAddrs` via `impl_p2p_message!` with `MAX_BYTES` estimates and
  metering configurations in the existing style; verify serialization
  roundtrip and size-estimate unit tests pass.

## 5. Advertisement store (D3, spec: swarm-overlay)

- [ ] 5.1 Implement the ad store: per-subnet and total entry caps, TTL
  expiry, shareable-scheme filtering, disk persistence, and no recording of
  querier data; verify unit tests pass for TTL expiry, cap enforcement,
  restart persistence, and non-shareable-scheme rejection.
- [ ] 5.2 Implement the ad refinery task: rate-limited liveness checks of
  advertised addresses with drop/downgrade on failure; verify an integration
  test shows a dead advertised address is dropped before TTL expiry and
  checks are not issued in a simultaneous burst.

## 6. ProtocolSwarm: gossip and queries (D4, D8)

- [ ] 6.1 Implement `ProtocolSwarm` gossip handling: validate `SubnetAd`
  (one subnet, shareable schemes), insert into the ad store, relay
  unmodified with no provenance or hop fields; register via
  `ProtocolRegistry` on the overlay's outbound and inbound sessions; verify
  a test asserts relayed ads are unchanged and non-shareable addresses are
  dropped rather than relayed.
- [ ] 6.2 Implement `GetSubnets`/`GetSubnetAddrs` query handling answered
  from local state for every connected peer; verify a test shows a
  persistent and a transient node with identical local state answer with
  the same message types and semantics.
- [ ] 6.3 Implement the jittered ad re-gossip cadence with no
  event-triggered emission; verify tests show no ad is sent on subnet
  start, listener startup, or new overlay connection, and that the cadence
  tick does emit.

## 7. Swarm lifecycle and roles (D1, D5, D6, D8)

- [ ] 7.1 Implement `Swarm` owning the overlay `P2p` plus the subnet
  registry, with persistent and transient constructors (persistent:
  `swarm-store` handshake feature via task 3.1, high inbound; transient:
  zero inbound, minimal outbound, optional overlay hostlist cache and
  nothing else on disk); verify a transient integration test writes only
  the overlay hostlist cache and emits no ads.
- [ ] 7.2 Implement `join(descriptor)`: overlay resolution, greylist
  seeding of resolved addrs, subnet `P2p` spawn with `SubnetId`-namespaced
  datastore/hostlist paths, app protocol registration before start; verify
  a descriptor-only join succeeds against a local overlay seed with no
  subnet seed configuration, and unknown subnets fail in bounded time.
- [ ] 7.3 Implement `serve()`: per-subnet opt-in requiring that subnet's
  inbound addrs, advertising only those addrs on the 6.3 cadence; verify
  tests show served-subnet ads carry only that subnet's addresses and
  join-only mode never emits an ad.
- [ ] 7.4 Implement `leave()`: stop and deregister the subnet `P2p` with no
  departure message and state retained for rejoin; verify tests assert no
  overlay message is emitted on leave and rejoin resumes from the persisted
  hostlist.
- [ ] 7.5 Verify runtime lifecycle tests pass: late subnet spawn does not
  disturb running subnets or the overlay, and concurrent teardown of
  several subnets terminates cleanly.
- [ ] 7.6 Implement transient cache-first bootstrap: the overlay hostlist
  cache is persisted between sessions and dialed before configured seeds,
  with cache contents limited to overlay peer addresses; verify an
  integration test where a second session connects through cached peers
  with all seeds unreachable, and a test asserting the cache contains no
  subnet identifiers or query records.

## 8. Multi-node integration tests (spec scenarios)

- [ ] 8.1 Cold-start chain test over local transports: overlay seed →
  persistent node → transient node discovers and joins a subnet with no
  per-subnet seeds anywhere; verify the transient node connects into the
  subnet and leaves no trace in any hostlist or ad store.
- [ ] 8.2 Abuse tests: an ad flood exceeds store caps and triggers
  metering/ban penalties, and poisoned (unreachable) addrs are dropped by
  refinement; verify both behaviors in one integration test.
- [ ] 8.3 Linkability structure test: one node serving two subnets through
  distinct addresses emits ads that share no field linking them; verify by
  structural inspection of all emitted messages in a two-subnet test.

## 9. Lilith overlay seed (D7, spec: lilith-overlay-seed)

- [ ] 9.1 Add the `[overlay]` config section with parsing, overlay-only
  mode, and mixed overlay + legacy per-network operation; verify config
  parsing unit tests (overlay-only, mixed, malformed) and a mixed-config
  integration test where both the overlay seed and legacy nets run.
- [ ] 9.2 Wire the durable ad store and refinery into lilith; verify a
  restart test shows persisted ads survive a stop/start cycle within TTL
  bounds and no querier data is stored.
- [ ] 9.3 Retarget lilith's RPC: listener health, participating subnet ids,
  ad store stats, with no per-querier data; verify a JSON-RPC test asserts
  the reported fields and the absence of query-source information.

## 10. Pilot adoption, docs, gates

- [ ] 10.1 darkirc pilot behind a config flag: pinned descriptor via 2.2,
  `Swarm`-based networking with static-seed fallback preserved; verify
  tests exercise both the overlay path and the fallback path.
- [ ] 10.2 Write deployment guidance: onion-per-subnet recommendation,
  shared-endpoint linkability hazard (required by the no-cross-subnet
  linkability spec requirement), and transient battery-vs-mixing
  recommendation to hold the overlay connection for the app session.
- [ ] 10.3 Full gates green: `make`, `make clippy`, `make test`, and
  `make fmt` all complete without errors.
- [ ] 10.4 Invoke `@anon-security-review` on the full diff and record the
  verdict on the change; a FAIL is blocking — resolve findings or escalate
  before marking the change ready to apply.
