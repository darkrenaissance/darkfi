Every phase below ends at a **logically atomic commit point** and closes
with a review gate: run the phase verification (unit tests and/or
netdebug + trace logs + visual inspection), then a thorough code review
by the owner with possible amendments or clarifications, then commit.
The next phase does not start before the gate passes. Tooling: unit
tests (`cargo test` in bin/app), netdebug zeromq CLI (CallMethod,
GetPropertyValue, signal subscriptions), trace logs (`ui::chatview2*`),
and visual inspection on the schema-chatview dev screen. See
design.md → Development Protocol.

## 1. Fenwick tree util

- [ ] 1.1 Implement `src/util/fenwick.rs` (new, get, add, set, push,
  prefix, range, lower_bound, rebuild) with randomized unit tests
  comparing every operation against brute-force sums over a reference
  `Vec<f32>`; verify `cargo test` in bin/app passes
- [ ] 1.2 Gate: review tests + implementation with the owner, apply
  amendments, commit as one atomic unit

## 2. Buffer: records and ordering

- [ ] 2.1 Implement `src/ui/chatview2/buffer.rs` record store: slotmap
  arena, order index sorted by `(timestamp, msg_id)`, dedup set, removal
  by id; verify unit tests pass: insert at any position, duplicate
  insert ignored, same-millisecond coexistence, removal, ordered
  iteration
- [ ] 2.2 Gate: review + amendments + atomic commit

## 3. Buffer: geometry

- [ ] 3.1 Wire the Fenwick tree into the buffer: `total_height`,
  `visible_range(scroll, view_h)`, `pos_of(msg_id)`, `set_height`
  point-updates, `insert_batch` single-rebuild, plus the
  below-viewport compensation math helper; verify randomized unit tests
  pass comparing geometry against a linear scan and compensation
  below/inside/above viewport and at scroll==0
- [ ] 3.2 Gate: review + amendments + atomic commit

## 4. Wire codec and legacy decode

- [ ] 4.1 Implement the tagged value codec (`[u8 tag][type bytes]`,
  privmsg payload = `nick, text` encoding + confirmed flag): a fixed
  `#[repr(u8)]` `MsgType` enum whose discriminants are the wire tags
  (encode via `as u8`, decode via a `from_u8` match, no factories);
  verify unit tests pass for encode/decode round-trip and that an
  unknown tag or undecodable payload panics with an identifying
  message (corrupt data is never silently skipped)
- [ ] 4.2 Gate: review + amendments + atomic commit

## 5. Scroll controller

- [ ] 5.1 Implement `src/ui/chatview2/scroll.rs`: internal
  pixels-from-bottom scroll (no scene property), Idle/Drag/Glide/Anim
  state machine with intents fed from the gesture subsystem's drag
  lifecycle (drag start/move/end; flick = threshold on the session's
  `DragEnd` velocity — no local sampling; page tick,
  scroll_to_bottom), clamping, is_at_bottom indication,
  height-change compensation application, anchor snapshot/resolve with
  bottom shortcut and clamped fallback; verify unit tests pass for
  state transitions (grab cancels motion, wheel coalescing extends the
  target, flick fed a `DragEnd`-style velocity decays to stop, clamps
  at 0 and top, scroll_to_bottom, anchor round-trip with inserts above
  and below)
- [ ] 5.2 Gate: review + amendments + atomic commit

## 6. Chatview2 skeleton + dev schema screen

- [ ] 6.1 Create the `src/ui/chatview2/` module skeleton and the
  `ChatView2` UIObject: properties (rect, shared styling, is_at_bottom
  bool), view-wide method stubs (`set_channel`, `set_filter`,
  `copy_select`, `unselect`, `scroll_to_bottom`, `get_line_ids`,
  `delete_line`), the gesture contract (`gesture_set` chatview-shaped:
  tap + long-press + vertical drag; exact-rect `gesture_hit_test`;
  `handle_gesture` stub), empty-buffer draw running the visible-window
  loop, and the node factory; verify `make compile-dev` succeeds and
  the dev screen renders an empty view
- [ ] 6.2 Create the `src/app/schema/test_chatview.rs` dev schema
  (modeled on `schema/test.rs`) hosting the chatview2 node for
  development, gated behind a new `schema-test-chatview` cargo feature
  following the existing `schema-test-*` convention (mutually exclusive
  with `schema-app` via the one-schema compile guard); verify by
  running the app with the feature enabled: screen shows, and a
  netdebug scene dump lists the chatview2 node with its properties and
  methods
- [ ] 6.3 Gate: review + amendments + atomic commit

## 7. Loader pipeline + live-testing methods

- [ ] 7.1 Implement `src/ui/chatview2/loader.rs`: single kvdb-owning
  bg task, coverage invariant (live bottom + viewport + preload
  margin), wake-ups (set_channel, near-top scroll, insert, filter, rect
  change), filter application at load, plus working `get_line_ids` and
  `delete_line` methods on the chatview; verify via netdebug on a
  fixture tree: `set_channel` then `get_line_ids` returns the expected
  (ts, id) list in display order, `delete_line` removes a record and it
  stays gone after re-`set_channel`; trace logs show load batches and
  Fenwick rebuilds as designed
- [ ] 7.2 Gate: review + amendments + atomic commit

## 8. Message type framework

- [ ] 8.1 Implement `src/ui/chatview2/msg/mod.rs`: `MessageType`
  trait (materialize/release/regen/height/draw/hit_test/copy_text) and
  the hardcoded `MsgType` enum dispatch (no factories, no placeholder —
  unknown type ids panic at decode), height reporting into the buffer;
  verify unit tests pass for materialize→release→materialize state
  rebuild, height report, and cache lifecycle (state cached across
  draws, invalidated by width/styling/data changes); visually confirm
  records render on the dev screen and trace logs show
  materialize/release as the window moves
- [ ] 8.2 Gate: review + amendments + atomic commit

## 9. Privmsg I: insertion and basic rendering

- [ ] 9.1 Implement `msg/privmsg.rs` part 1: the type's
  `insert_line`/`insert_unconf_line`/`confirm` methods (persist via the
  loader, dedup, buffer insert, confirm rewrites the payload in place
  and regens, materialize if visible) and basic rendering (nick colors,
  timestamp, plain text layout at the wrapped width); verify unit tests
  pass for layout cache behavior (layout reused across repeated
  measures, re-wrapped on width change, invalidated on styling/data
  change) and the unconfirmed→confirm in-place update; verify via
  netdebug `CallMethod insert_line` feeding batches of lines and
  `confirm` on one: trace logs show dedup, fenwick pushes, layout cache
  hit/miss behavior; visual inspection against the old chatview for
  basic lines and unconfirmed→confirmed restyling
- [ ] 9.2 Gate: review + amendments + atomic commit

## 10. Privmsg II: URLs, signals, variants

- [ ] 10.1 Implement privmsg part 2: URL spans with backgrounds,
  click/tap open, right-click/long-press copy with the toast overlay,
  `nick_clicked` signal carrying msg id + nick, CTCP ACTION rendering,
  NOTICE styling, unconfirmed gray; verify netdebug-driven inserts of
  urls/actions/notices render correctly (visual), the `nick_clicked`
  signal is observable on the netdebug pub socket, and toast behavior
  matches the old chatview
- [ ] 10.2 Gate: review + amendments + atomic commit

## 11. Selection across types

- [ ] 11.1 Implement selection: chatview-owned selected set,
  chatview-drawn highlight (no per-type cache invalidation), mouse
  click toggle, `Tap` toggle, long-press entering selection mode with
  subsequent `DragMove` extending the selection instead of scrolling,
  drag sweep, selection-mode taps, per-type `copy_text` joined in
  display order, `unselect`, `select_changed` transitions; verify unit
  test for mixed-type copy ordering passes, netdebug
  `copy_select`/`unselect` behave per spec, and manual
  drag/toggle/long-press selection works visually on the dev screen
- [ ] 11.2 Gate: review + amendments + atomic commit

## 12. Date separators

- [ ] 12.1 Implement `msg/datemsg.rs` and derived records: synthetic
  `(midnight, [0;32])` keys, `sync_separators` on insert/load/delete,
  orphan cleanup, selectable with date-label copy text; verify unit
  tests pass for separator sync and orphan removal, netdebug
  `delete_line` of a day's only message removes its separator, and
  visual day-boundary rendering matches the old chatview
- [ ] 12.2 Gate: review + amendments + atomic commit

## 13. Scroll input integration

- [ ] 13.1 Wire input on the dev screen: touch via `handle_gesture`
  mapping to the scroll controller per design's gesture integration
  table (`Down` pauses inertia and resets long-press mode, `DragStart`
  grabs (kills motion), `DragMove` 1:1 on the chat axis (scroll =
  scroll0 + dy), `DragEnd` flick on the session velocity, `Up` clears
  touch-active state), plus `handle_mouse_wheel` and PageUp/PageDown
  keys feeding `page_tick` (animated half-page with coalescing),
  clamps, a visible scroll-to-bottom arrow layer (priority +1 above
  the view) driven by `is_at_bottom` calling `scroll_to_bottom`, and
  the animator deadline-cadence task; verify the slop dead-zone, 20ms
  move cadence, and single-fire long-press come from the session (no
  local recognition), trace logs show the intended state transitions
  per gesture, netdebug `GetPropertyValue is_at_bottom` flips as
  expected, the overlaid arrow receives its taps (arbitration), and
  visual inspection confirms pixel-exact drag, smooth wheel animation,
  correct stops at both clamps, and the arrow toggling with position
- [ ] 13.2 Gate: review + amendments + atomic commit

## 14. Materialization lifecycle and eviction

- [ ] 14.1 Implement the virtualization window with soft margin and
  LRU eviction budget: materialize on window enter, release on exit
  (render-scoped tasks cancelled), bounded render resources; verify a
  unit test for the LRU/budget policy passes (eviction order under
  pressure, window membership), trace logs show materialize/release
  while scrolling a large fixture history, renderer debug stats show
  bounded memory/GPU resources after long scrolls, and visual
  inspection shows no ghost lines or missing lines around the window
  edges
- [ ] 14.2 Gate: review + amendments + atomic commit

## 15. Cap/expand for long messages

- [ ] 15.1 Implement the collapsed default height with expand
  affordance and toggle: height change flows through regen +
  compensation; verify a unit test for capped measurement and expand
  height reporting passes, and visual toggle on very long messages
  keeps surrounding content stable per the compensation rules
- [ ] 15.2 Gate: review + amendments + atomic commit

## 16. Reflow

- [ ] 16.1 Implement the reflow protocol (anchor snapshot → invalidate
  rendered state → re-wrap visible-first → single Fenwick rebuild →
  anchor restore; height-only rect changes just re-clamp); verify via
  netdebug `SetPropertyValue` on font_size and by resizing the window:
  trace logs show visible-first regen order and one rebuild, visual
  inspection confirms the anchored message stays put and bottom stays
  pinned
- [ ] 16.2 Gate: review + amendments + atomic commit

## 17. Filemsg, content-scoped tasks, i18n

- [ ] 17.1 Implement `msg/filemsg.rs`: `set_file_status` method, status
  lifecycle rendering, fud URL derivation, `download_request`/
  `fileurl_detected`/`status_changed` signals, downloaded image display
  with fit bounds, and the content-scoped `key → Task` map surviving
  eviction; wire `i18n_fish` into type constructors and translate file
  status strings; verify a fud round-trip with the plugin enabled
  (visual image + statuses), trace logs show task dedup/attach on
  re-materialization, eviction mid-download continues the task, and a
  language switch translates the status text
- [ ] 17.2 Gate: review + amendments + atomic commit

## 18. Single-screen cutover

- [ ] 18.1 Rework `src/app/schema/chat.rs` to a single chat screen
  with one chatview2: `set_channel` on channel selection, channel
  label binding, scroll-to-bottom arrow driven by `is_at_bottom` +
  `scroll_to_bottom`; remove the per-channel screen loop in
  `schema/mod.rs`; retarget relay paths in `main.rs` and plugins
  (darkirc insert/confirm to the privmsg node, fud status fan-out to
  the filemsg node); set explicit node priorities for every interactive
  layer floating over the chatview (scroll-to-bottom arrow, cmd-hint
  popup) above it, per the gesture session's priority-ordered
  targeting; verify on desktop with darkirc: receiving messages
  updates the active channel, switching channels clears/reloads and
  restores each channel's position, and unread indication works via
  signals
- [ ] 18.2 Gate: review + amendments + atomic commit

## 19. Old module removal + full validation

- [ ] 19.1 Delete `src/ui/chatview/` and all old-chatview references
  (`ui/mod.rs` re-exports, test schemas using `create_chatview`); verify
  `make compile-dev` and `make compile-apk` both succeed
- [ ] 19.2 Full validation: parity checklist from `specs/chatview/spec.md`
  (URLs, selection/copy incl. separators, file messages, actions/notices,
  unconfirmed, date separators, keys, touch incl. slop dead-zone,
  grab-to-stop, long-press select mode, wheel, restore, reflow) on
  desktop; performance pass scrolling a large history (no slowdown,
  bounded memory via renderer debug stats); android device smoke test
  (drag, flick, grab-stop, long-press select/copy, channel switching)
- [ ] 19.3 Gate: final review + atomic commit closing the change
