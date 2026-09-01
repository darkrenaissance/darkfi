## Why

The app's chat UI duplicates a full screen (layer tree, ChatView, kv tree,
mesh caches) for every joined channel, scrolls with costs that grow linearly
with buffer size, keeps render resources (meshes, glyphs, textures, layouts)
forever once messages scroll out of view, and conflates every scroll gesture
into a single velocity scalar — making animated wheel paging, stable
anchoring, and richer content (images, expandable rich messages, stickers)
progressively harder to bolt on. The full requirements list agreed during
exploration is captured in this change's `design.md` and spec; this change
implements it as a ground-up `chatview2` with the current implementation
serving as the functional spec (feature parity, no regressions).

## What Changes

- New `src/ui/chatview2/` module in `bin/app`: modular chatview — buffer
  (ordered arena + Fenwick height index), scroll controller, background
  loader, virtualizing view, and per-type message nodes.
- **BREAKING** (internal scene API): the old `src/ui/chatview/` is removed
  once parity is reached; `src/app/schema/chat.rs` moves from one duplicated
  screen per channel to a single chat screen with the chatview retargeted
  via `set_channel`.
- Message system: one scene sub-node per message **type** (not per
  message), carrying type-specific styling properties, signals, and
  methods with msg ids in payloads; messages are records in the buffer
  plus per-id render state owned by the type node.
- Property inheritance: shared styling (font_size, line_height, timestamp
  styling, selection color) defined once on the chatview2 node; type nodes
  define only type-specific properties; property handles passed into the
  msg type's `new()`; `.regen()` rebuilds rendered state (used for both
  styling changes and async content updates).
- Storage: keep kvdb (no custom format); kv key stays the
  `(timestamp, msg_id)` composite; value becomes a type ID followed by
  type-owned bytes; unconfirmed messages are persisted with a confirmed
  flag.
- Scrolling: pixel scroll from bottom (scroll=0 is always the bottom),
  1:1 finger drag, animated half-page mouse-wheel jumps, flick inertia as
  distinct states of a scroll controller fed by the new `ui/gesture`
  subsystem (drag lifecycle events, release velocity on `DragEnd`); the
  controller owns physics only — recognition, slop, long-press timers,
  and velocity sampling are session-provided; compensation rule for
  height changes below the viewport; per-channel scroll restore (anchor
  msg id + offset) on re-entry.
- Performance: visible-range lookup, total height, and position queries
  are O(log n) in buffer size; only visible messages (soft window + LRU
  budget) hold render resources; loading is a single async background
  pipeline; no hard buffer cap in v1.
- Runtime-settable message filter callback applied in the load pipeline.
- Privmsg v1 ships plain text + nicks + URLs plus capped-height with
  expand; the rich span/block body model (quotes, styling, code, math) is
  deferred but the APIs are shaped for it.
- Message types take the i18n_fish for translation support.

## Capabilities

### New Capabilities

- `chatview`: the chatview2 widget — scene API (properties, methods,
  signals, per-type message sub-nodes), buffer and geometry semantics
  (ordering, insertion, dedup, heights), scrolling behavior (gestures,
  animation, clamping, anchoring, restore), storage format, background
  loading, filter, resource eviction, and feature parity with the current
  chatview (URL clicks, selection/copy, file messages, notices/actions,
  unconfirmed messages, date separators, keyboard scrolling).

### Modified Capabilities

(none — no existing specs in this repo; the chat screen rework in
`src/app/schema/` is part of this change's tasks and is exercised through
the `chatview` capability.)

## Impact

- `bin/app/src/ui/chatview2/` (new module), `bin/app/src/ui/chatview/`
  (removed at switchover), `bin/app/src/ui/mod.rs`.
- `bin/app/src/app/schema/chat.rs` (single screen, channel switching),
  `bin/app/src/app/schema/mod.rs` (per-channel screen creation loop),
  `bin/app/src/app/node.rs` (node factories), `bin/app/src/main.rs`
  (message relay paths), `bin/app/src/plugin/darkirc.rs`,
  `bin/app/src/plugin/fud.rs` (insert/status paths, unconfirmed
  persistence).
- Per-channel kv trees: value format gains a type ID tag; keys unchanged.
  Existing history remains readable (v1 privmsg payload is the current
  `nick, text` encoding plus a confirmed flag).
- `bin/app/src/ui/gesture/` (applied by the `app-gesture` change) is
  consumed as-is, not modified: chatview2 implements `gesture_set`/
  `gesture_hit_test`/`handle_gesture` instead of the removed
  `handle_touch`; overlaying interactive layers (scroll-to-bottom arrow,
  cmd hints) need explicit node priority above the chatview for the
  gesture session's priority-ordered targeting.
- Build/test via `bin/app` Makefile targets (`make compile-dev`,
  `compile-apk`); no changes outside `bin/app`, no new dependencies.
