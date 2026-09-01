## 1. Static-DAG app payload plumbing (event_graph)

- [ ] 1.1 Define the static content tag registry (first byte `0x00`/`0x01` =
      RLN payloads, `0x02..` = app payloads) and a dispatch helper in
      `src/event_graph/`, and add a unit test asserting `RLNNode` encodings
      only ever produce first bytes `0x00`/`0x01` (guards the tag collision
      contract in design D1)
- [ ] 1.2 Extend `handle_static_put` to admit app-tagged events via the same
      structural checks plus a content size bound, in both RLN modes, relaying
      them onward; verify with unit tests covering admit (both modes),
      oversize reject, malformed skip, and that RLN-event handling is
      unchanged (existing RLN tests stay green)
- [ ] 1.3 Relax `static_sync`/`EventRep` blob alignment so app-tagged static
      events may carry empty blobs, re-applying structural checks at sync;
      verify with a two-node sync test where node B pulls an app-tagged event
      from node A
- [ ] 1.4 Make RLN state rebuild/audit (`rebuild_rln_state_from_static` and
      the startup audit) skip app-tagged events deterministically; verify via
      a test that rebuilds over mixed RLN+app history and asserts an identity
      tree identical to the RLN-only rebuild

## 2. Channel chain types and resolver (irc2)

- [ ] 2.1 Define `ChannelAction` (`Register`, `Transfer`, `PolicyList`, `Pin`,
      `Unpin`) with prev-link to the previous action's static event id and
      schnorr signature over channel+payload+prev, plus the `PolicyId` u8 enum
      and `{policy, params, enabled}` entries; include the ChanServ name
      mapping (`ALLOWLIST`/`ADMINHIDE`/`FILTER` ↔ 0/1/2) with unknown-name
      rejection; verify serialization round-trip and name-parse unit tests
      pass
- [ ] 2.2 Implement the chain resolver (winning registration by canonical
      order, longest valid chain, canonical tie-break, transfer rekeys,
      invalid-signature links ignored); verify with unit tests: register,
      transfer-then-policy, stale-owner rejection, competing chains,
      registration race, garbage events ignored
- [ ] 2.3 Add a resolved-channel-state cache in `irc2` fed by a
      `static_pub` subscription, exposing per-channel owner/policy/pins;
      verify with a test that publishes chain events into a test EventGraph
      and asserts the cache converges to the resolver output

## 3. ChanServ and signing keys (desktop darkirc)

- [ ] 3.1 Add schnorr owner/admin keypair config parsing to darkirc TOML
      settings (secret base58; public derived), refusing invalid input with a
      clear error; verify settings unit tests including `--gen` style keypair
      generation output if following the existing chacha keypair precedent
- [ ] 3.2 Implement ChanServ service (`REGISTER`, `INFO`, `TRANSFER`,
      `POLICY LIST|SET|DEFAULT`, `PIN`, `UNPIN`, `HELP`) with NOTICE replies
      and refusal when no matching key is configured; verify with irc2 unit
      tests per command (success, wrong/missing key, unknown channel)
- [ ] 3.3 Extend `INFO` output with resolved owner, chain tip, default policy
      list, and pins from the cache in 2.3; verify via a ChanServ test against
      a seeded static DAG
- [ ] 3.4 Add the ChanServ integration test: two-node live network, register
      on node A, `INFO` on node B resolves the same owner; `TRANSFER` followed
      by a policy update signed by the new key is accepted on both

## 4. Hide actions in the rotating DAG

- [ ] 4.1 Define the `HideAction` rotating content type (tag `0x02`:
      channel, target event id, hidden flag, actor pk, schnorr sig) and the
      rotating content tag dispatch (tag-first decode, unknown tag skip) in
      the irc2 relay path; verify with unit tests for each tag path plus
      unknown-tag skip
- [ ] 4.2 Implement hidden-set resolution (valid actions by keys in the
      currently enabled `AdminHide` set, last-wins per target in canonical
      rotating order, across retained windows) as part of the resolved-state
      cache; verify with unit tests: hide, unhide, unauthorized ignored,
      disabled-policy ignored, cross-window expiry
- [ ] 4.3 Add ChanServ `HIDE`/`UNHIDE` commands (admin key required) and the
      send path building the rotating event, including the RLN signal flow
      when RLN is enabled; verify with a two-node test: hide on A marks the
      message hidden on B, unhide restores it
- [ ] 4.4 Apply hidden marking in the relay path (event-id check before
      msg_id conversion) so hidden messages are stored but flagged, not
      dropped; verify with an irc2 relay test asserting the message is
      retained and flagged

## 5. Tagged Privmsg and policy evaluators

- [ ] 5.1 Retag all darkirc rotating content with the leading tag byte
      (Privmsg `0x00` with optional signer pk + schnorr sig over serialized
      core fields, computed before channel/DM encryption; no untagged form,
      hard break per design D2) with send-side signing when the user's key is
      in an enabled `AllowList`; verify serialization round-trip and
      sign/verify unit tests, including that encrypted sends carry sig fields
      inside the ciphertext
- [ ] 5.2 Implement built-in policy evaluators (`AllowList` signature check,
      `AdminHide` authorization already in 4.2, `Filter` regex matching
      over the decoded privmsg nick and/or content) with unknown-id entries
      and uncompilable regex rules ignored; adding the `regex` dependency to
      `bin/darkirc/Cargo.toml` is a review-flagged supply-chain step; verify
      with per-policy evaluator unit tests (signed/unsigned, regex match on
      nick, regex match on msg content, invalid regex skipped, unknown
      policy id)
- [ ] 5.3 Verify robust dispatch: unknown tags and malformed tagged content
      are skipped without error propagation or peer penalty (unit test
      covering both rotating and static paths, per the app-payloads spec)

## 6. App integration

- [ ] 6.1 Port tag-byte dispatch and the resolved-policy cache into
      `bin/app/src/plugin/darkirc.rs` relay (Privmsg, HideAction,
      hidden marking before msg_id conversion); verify with plugin-level tests
      mirroring 4.4/5.2
- [ ] 6.2 Add the policy override table (per-channel, per-policy rows
      overriding the owner default flag) with schema-level tests asserting
      override resolution (`default || override`) is consulted by the
      evaluators
- [ ] 6.3 Make the chat screen's channel-name label tappable: add a normal
      button node over the existing channel label (label placement constants
      in `bin/app/src/app/schema/chat.rs`, e.g. `CHANNEL_LABEL_X/Y`), using
      the same button pattern as the chat screen's send/emoji buttons;
      activation opens that channel's policy overlay; verify the button hit
      area covers the label and fires for both mouse and touch
- [ ] 6.4 Build the policy overlay scene node (following existing overlay/
      layer patterns) listing the resolved default policy list with toggle
      switches bound to the override table; toggling writes the override row
      and re-filters the channel buffer as a pure view update (hidden
      messages are marked, not dropped); verify overlay wiring against a
      seeded resolved-policy cache including the unregistered-channel empty
      state
- [ ] 6.5 Add owner-key storage to the app settings store (secret only, public
      derived, never logged); verify round-trip and no-leak assertions in
      settings tests

## 7. Integration and review gates

- [ ] 7.1 Full workspace gates pass: `make` then `make test` then
      `make clippy` all clean with `--all-features`
- [ ] 7.2 End-to-end multi-node scenario passes: register → policy list →
      posts → hide/unhide → pin → rotation expiry leaves pin renderable and
      hide state expired (extend the irc2 integration harness)
- [ ] 7.3 Human review of the RLN admission-path diff (`handle_static_put`,
      `static_sync`, RLN rebuild) per repo policy, plus the
      `@anon-security-review` pass on the change diff before marking ready to
      archive
