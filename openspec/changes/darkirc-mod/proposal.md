## Why

DarkIRC (and the chat in `app`, which embeds the `irc2` stack) has no moderation:
any RLN-rate-limited message renders for everyone, so signal-to-noise in public
channels depends entirely on posters' good behavior. Moderation today would
require either a central operator or protocol changes rushed into the chat path.
The event graph already provides the right primitives — a replicated, never-
pruned static DAG (currently RLN-only) and rotating chat DAGs — so we can add
owner-designated, client-enforced channel policy without a central party and
without touching anonymity: policy targets signal-to-noise, not traffic (flooding
remains RLN's job).

## What Changes

- **Static DAG app payloads**: static-DAG admission learns a content tag byte
  that separates RLN payloads from application payloads. DarkIRC channel
  metadata (registration, ownership, default policy list, pins) rides the
  existing static DAG. No second EventGraph instance is introduced.
- **Content type tag byte on rotating DAG events**: darkirc rotating content
  is discriminated by a leading tag byte (Privmsg, hide action) instead of
  trial deserialization. **BREAKING**: this is a hard break of the chat wire
  format — there is no untagged legacy form; all darkirc nodes must upgrade
  together. Existing messages age out within one 24h rotation window and the
  network self-cleans.
- **Channel registry with owner chains**: each public (`#`) channel can be
  registered in the static DAG with an owner public key. Owner actions
  (transfer ownership, set default policy list, pin messages) form a
  prev-linked, owner-signed chain; resolution is deterministic (longest valid
  chain, canonical tie-break).
- **Policy model**: a hardcoded `u8` policy enum (`AllowList`, `AdminHide`,
  `Filter`, …) with named values on the ChanServ command surface
  (`ALLOWLIST`, `ADMINHIDE`, `FILTER`). The owner publishes the channel's
  default policy list with per-policy enable flags and opaque params. Users
  override defaults locally.
- **Hide actions in the rotating DAG**: admins (keys named by the `AdminHide`
  policy) publish signed hide/unhide actions as rotating events referencing a
  target event id. Hidden messages are marked hidden in clients, never removed
  from the DAG. Tombstones expire with their targets (same rotation window).
- **ChanServ** in `irc2`: `/msg ChanServ REGISTER|INFO|TRANSFER|POLICY|PIN|HIDE|
  UNHIDE|HELP` for channel owners and admins. Works in desktop darkirc; the app
  does not expose owner flows in this version.
- **Owner/admin signing keys in config**: schnorr keypairs configured in
  darkirc TOML (desktop) / app settings store (app).
- **App policy UI**: the channel-name label at the top of the chat screen
  (e.g. `#dev`) becomes tappable via a button placed over the existing label;
  it opens a per-channel overlay listing the owner's current default policy
  set, where the user can toggle each policy off/on locally. Overrides are
  stored as rows in the app's local table. No user-installed policies in this
  version.
- **Pins**: owner-signed static events embedding a snapshot of the pinned
  message; snapshots for encrypted channels are re-encrypted under the channel
  saltbox. (Public channels only in this version, but snapshots are encrypted
  wherever the source channel is encrypted so the mechanism is safe by default.)

## Capabilities

### New Capabilities

- `event-graph/app-payloads`: Tagged application payloads in the static DAG and
  content-type tag bytes on darkirc rotating events; admission, dispatch, and
  unknown-content handling for both DAG kinds.
- `chat-moderation`: Channel registration and owner chains, the hardcoded
  policy enum and default policy lists, rotating-DAG hide actions, ChanServ
  commands, and client-side policy application (desktop + app UI toggles).

### Modified Capabilities

(none — no existing specs)

## Impact

- `src/event_graph/` — static-DAG admission (`handle_static_put`), `static_sync`
  re-verification, RLN state rebuild must skip app payloads; touches the RLN
  admission path (security-critical zone; needs review).
- `bin/darkirc/` (`irc2`) — Privmsg wire format (tagged content, optional
  signature fields), content-tag dispatch in the relay path, ChanServ service,
  config keys for signing keypairs, static-event broadcast for owner chains.
- `bin/app/` — `plugin/darkirc.rs` relay (tag dispatch, hidden marking),
  policy cache, per-channel policy toggle UI, override rows in the local table,
  owner-key storage in the settings store; the chat screen's channel-name
  label gets a button overlay opening the policy overlay.
- Wire compatibility: hard break — mixed-version networks are unsupported;
  rotating content drains within one rotation window; static RLN encoding is
  unchanged so RLN state is unaffected.
- No changes to RLN circuits, proofs, or rate-limiting semantics.
