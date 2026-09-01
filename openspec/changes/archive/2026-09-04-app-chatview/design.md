## Context

The spec delta in `specs/chatview/spec.md` is the behavioral contract,
and this design document is the complete requirements record from the
redesign exploration — every agreed requirement lives in this change
(spec or design); nothing outside it is normative. The current
implementation (`src/ui/chatview/`) is the functional spec:
`MessageBuffer` holds a `Vec<Message>` newest-first with
cached parley layouts and mesh caches; `adjust_scroll`→
`calc_total_height` walks the entire buffer per scroll tick; `gen_meshes`
clones instructions for every message from newest to the viewport top
every frame; mesh caches are only cleared on rect/scale/epoch change
(never on scroll); `chat::make()` builds a full screen per channel and
switching toggles `is_visible`. Wheel/flick still share one `speed:
AtomicF32` decayed by a 10ms loop, but touch input now arrives through
the gesture subsystem (the applied `app-gesture` change):
`handle_touch`/`handle_touch_sync` are gone from `UIObject`, and the
migrated ChatView consumes the `GestureAction` stream — chatview2
builds on that seam from day one.

Scene/property system facts the design relies on: `Property*::wrap`
returns a live handle to a property object on a node (cross-node wrapping
already exists — `window_scale` from `/window`); nodes track parents;
nodes carry signals (`register`/`trigger`), method-call subscriptions,
`OnModify`, and task lists; `Pimpl` UIObjects implement `draw` and
`handle_*` input. Gesture subsystem facts (applied `app-gesture` change,
`src/ui/gesture/`): `UIObject` carries `gesture_set()`/
`gesture_hit_test()`/`handle_gesture(GestureAction)`; the window
`GestureSession` owns recognition — 10px touch slop, 300ms tap bound,
timer-fired single long-press, 20ms-throttled `DragMove`, release
velocity sampled over a 40ms window and carried on `DragEnd { vel }`
(px/sec) — with sticky hit-test-chain ownership resolved in node
priority order; wheel and keyboard remain `handle_mouse_wheel`/
`handle_key_down`.

## Goals / Non-Goals

**Goals:**

- Ground-up `src/ui/chatview2/` with one module per concern, replacing
  `src/ui/chatview/` at parity.
- Geometry operations independent of buffer size; bounded render
  resources; non-blocking loading.
- Scene API: chatview2 node + one stable sub-node per message type, ids
  in signal/method payloads, property inheritance with a single `regen`
  verb for styling and content changes.
- Keep kvdb storage with a type-tagged value format; persist unconfirmed
  messages.

**Non-Goals:**

- Rich span/block privmsg body (quotes, code, math) — v1 is plain text +
  nicks + URLs + cap/expand; APIs shaped for later.
- New message types beyond current parity set (stickers, forms, latex,
  all-view, first-contact notices) — the registry makes them additive.
- Layout sidecar (persisted height cache) — deferred until re-entry on
  big histories measures poorly.
- Hard buffer cap, strict window-bound eviction, full anchor-based scroll
  core — simplest-thing-first; upgrade paths noted below.
- Any change outside `bin/app`.

**Future message content** (out of v1, but the registry, property
inheritance, payload format, and height-change machinery are shaped so
each lands as a new message type without reworking buffer or scroll):
animated stickers, forms, emojis, code blocks, latex, status messages,
images and rich media, green-text quotes, multi-line text, clickable
nicks/media, network messages, all-view (merged-channel view showing
channel labels), nick-highlight on mention, first-time-message and
rename notices, show/hide timestamps, click-to-notify items.

## Decisions

### Module layout

```
src/util/fenwick.rs        generic Fenwick tree (see Fenwick section)
src/ui/chatview2/
├── mod.rs        ChatView2 node: properties, methods, signals, input
│                 dispatch, draw assembly. Orchestrates; owns nothing heavy.
├── buffer.rs     MsgBuffer: ordering, dedup, geometry. Pure data.
├── scroll.rs     ScrollController: gestures, animation, clamping,
│                 compensation, save/restore.
├── loader.rs     Loader: kvdb → filter → buffer, coverage maintenance.
└── msg/
    ├── mod.rs    MessageType contract + registry
    ├── privmsg.rs  privmsg type node + instances
    ├── filemsg.rs  fud file type node + instances
    └── datemsg.rs  date separator type node (derived, never stored)
```

Main structures and basic APIs per file (sketches, not final signatures):

`mod.rs` — the scene node and its public surface:

```rust
pub struct ChatView2 { /* node, renderer, redraw, executor, buffer,
                          scroll controller, loader handles */ }

impl ChatView2 {
    pub async fn new(node, kv_db, window_scale, renderer, redraw, sg_root,
        i18n_fish, ex) -> Pimpl
}

// Properties (rect, shared styling, is_at_bottom: bool — arrow visibility;
// scroll position itself is internal to the scroll controller).
// Methods (called via call_method) — view-wide concerns only:
//   set_channel(channel), set_filter, copy_select, unselect,
//   scroll_to_bottom
// Testing methods (live testing, e.g. driven from the python gui api):
//   get_line_ids() -> [(ts, id), …] of loaded messages in
//                    display order
//   delete_line(id) -> drop from buffer + channel kv tree
// Signals — view-wide concerns only:
//   select_changed(bool)
// Message lifecycle methods and message-derived signals live on the
// type nodes, since each type defines its own semantics: privmsg
// exposes insert_line/insert_unconf_line/confirm plus url/nick
// interaction signals; filemsg exposes set_file_status plus its file
// signals.
// UIObject: draw() assembles only the visible window. Touch input via
// gesture_set()/gesture_hit_test()/handle_gesture(GestureAction) — see
// the gesture input integration section; Tap/LongPress hit-dispatch to
// materialized instances through the type registry;
// handle_mouse_wheel (wheel) and handle_key_down (PageUp/PageDown)
// feed the scroll controller's page_tick.
```

`buffer.rs` — ordering + geometry, no rendering:

```rust
pub struct MsgBuffer {
    /// Loaded message records
    records: SlotMap<SlotKey, MsgRecord>,
    /// Arena slots in display order (newest first, by (ts, msg_id))
    order: Vec<SlotKey>,
    /// Cumulative heights over `order`
    fenwick: Fenwick,
    /// msg_id -> arena slot
    index: HashMap<MessageId, SlotKey>,
}

pub struct MsgRecord {
    pub ts: Timestamp,
    pub id: MessageId,
    /// Message type; its repr(u8) discriminant is the wire tag
    pub msg_type: MsgType,
    /// Type-owned payload; per-type state like `confirmed` lives here,
    /// not on the record
    pub payload: Vec<u8>,
    /// Last height reported by the owning type node
    pub height: f32,
}

impl MsgBuffer {
    /// Insert at any (ts, id) position; false if already loaded (dedup)
    pub fn insert(&mut self, rec: MsgRecord) -> bool
    /// Remove a loaded record; false if unknown (debug deletion,
    /// structural edit — batched Fenwick rebuild)
    pub fn remove(&mut self, id: &MessageId) -> bool
    /// Update a height; returns the delta for scroll compensation
    pub fn set_height(&mut self, id: &MessageId, h: f32) -> Option<f32>
    /// Total px of loaded content
    pub fn total_height(&self) -> f32
    /// Display-order range intersecting [scroll, scroll + view_h)
    pub fn visible_range(&self, scroll: f32, view_h: f32) -> Range<usize>
    /// Px from content bottom up to the top of msg `id`
    pub fn pos_of(&self, id: &MessageId) -> Option<f32>
    /// Oldest loaded ts (loader resume point)
    pub fn oldest_ts(&self) -> Option<Timestamp>
    pub fn clear(&mut self)
}
```

`scroll.rs` — gestures, animation, anchoring policy:

```rust
pub enum ScrollState {
    Idle,
    Drag { start_y: f32, scroll0: f32 },
    Glide { velocity: f32 },
    Anim { from: f32, to: f32, started: Instant },
}

pub struct ScrollController {
    state: ScrollState,
    /// Internal position: px from content bottom; 0 = live bottom.
    /// NOT a scene property — see the scroll decision section.
    scroll: f32,
    /// total_height − view_h, maintained by the chatview
    max_scroll: f32,
}

/// Serialized "what the user is looking at" (save/restore boundaries
/// only): viewport-top message + dy below its top edge
pub struct Anchor { pub msg: Option<MessageId>, pub dy: f32 }

impl ScrollController {
    /// Drag: 1:1, cancels Glide/Anim. Inputs are the session's
    /// DragStart/DragMove events (already slop-gated and throttled);
    /// drag_end consumes the session's DragEnd velocity — no local
    /// sampling, timers, or thresholds live here.
    pub fn drag_start(&mut self, y: f32)
    pub fn drag_move(&mut self, y: f32) -> f32
    pub fn drag_end(&mut self, velocity: f32)
    /// Wheel/PageUp/PageDown: set or extend Anim target by half a page
    pub fn page_tick(&mut self, dir: f32, page: f32)
    /// The down-arrow: teleport to bottom, cancel all motion
    pub fn scroll_to_bottom(&mut self)
    /// Whether scroll == 0 (drives the is_at_bottom scene property)
    pub fn is_at_bottom(&self) -> bool
    /// Animator advance; applies the frame's scroll internally
    pub fn tick(&mut self, now: Instant) -> Option<f32>
    pub fn clamp(&self, scroll: f32) -> f32
    /// Height-change compensation (see the scroll decision section)
    pub fn compensate(&mut self, delta: f32, msg_below_viewport: bool)
    /// Snapshot of the current view position; no persistence here
    pub fn anchor(&self) -> Anchor
    /// Resolve an anchor against pos_of(id); clamped fallback
    pub fn restore(&mut self, anchor: &Anchor,
        pos_of: impl Fn(&MessageId) -> Option<f32>) -> f32
}
```

`loader.rs` — the single background pipeline:

```rust
/// Why the loader was woken. Reasons are advisory bookkeeping: the
/// pump always just restores the coverage invariant, whatever the
/// trigger — except ChannelSwitch, which clears the buffer first.
/// The reason is recorded for trace logs and to let the pump skip
/// work it knows is pointless (e.g. NearTop when already covered).
pub enum Wakeup { ChannelSwitch, NearTop, Insert, FilterChange, RectChange }

pub type FilterFn = Arc<SyncMutex<Box<dyn Fn(&MsgRecord) -> bool + Send>>>;

pub struct Loader {
    /// Sole kvdb accessor for this chatview
    tree: Tree,
    buffer: Arc<AsyncMutex<MsgBuffer>>,
    filter: FilterFn,
    /// Wakers call wake(reason); reasons accumulate in a bitset so
    /// coalesced wakes are not lost while the pump is running
    cv: CondVar,
    pending: AtomicU8,
}

impl Loader {
    /// Wake the pump, recording the reason (bits coalesce)
    pub fn wake(&self, reason: Wakeup)
    /// Bind a channel tree; refill newest→older until coverage met
    pub async fn bind(&mut self, tree: Tree)
    /// Coverage pump: take pending reasons, ChannelSwitch clears the
    /// buffer, then load newest→older until viewport + margin is
    /// covered; apply filter, batch structural edits into one Fenwick
    /// rebuild per batch
    async fn pump(&mut self, viewport: Range<f32>, margin: f32)
    /// Decode kvdb entry -> record; unknown type id panics (corrupt)
    fn decode_entry(key: &[u8], val: &[u8]) -> MsgRecord
}
```

Wakers: the chatview calls `wake(ChannelSwitch)` from `set_channel`;
the draw/scroll path calls `wake(NearTop)` when the viewport approaches
the top of loaded coverage; the type nodes' insert methods call
`wake(Insert)`; `set_filter` and rect changes call their own reasons.
The loop itself is the plain condvar wait → pump → reset shown in the
loader decision section.

`msg/mod.rs` — the type contract and registry:

```rust
/// All message types, hardcoded — a fixed enum, no factories.
/// The wire tag IS the discriminant (repr(u8)): encode with
/// `msg_type as u8`.
#[repr(u8)]
pub enum MsgType {
    PrivMsg = 0,
    FileMsg = 1,
    DateMsg = 2,
}

/// Decode a wire tag. Unknown tags are corrupt data and panic —
/// errors are always explicit, never a silent skip.
pub fn msg_type_from_u8(t: u8) -> MsgType {
    match t {
        0 => MsgType::PrivMsg,
        1 => MsgType::FileMsg,
        2 => MsgType::DateMsg,
        _ => panic!("unknown msg type tag {t}"),
    }
}

pub trait MessageType {
    type Instance;
    fn msg_type(&self) -> MsgType;
    /// new() + regen(): build an instance from a record + live props
    fn materialize(&mut self, rec: &MsgRecord) -> &mut Self::Instance
    /// Drop render state, cancel render-scoped tasks (LRU budget)
    fn release(&mut self, id: &MessageId)
    /// Rebuild rendered state from live props + current data
    fn regen(&mut self, id: &MessageId)
    fn height(&self, id: &MessageId) -> Option<f32>
    fn draw(&mut self, id: &MessageId, rect: &Rectangle, renderer: &Renderer)
        -> Vec<DrawInstruction>
    /// Hit dispatch (urls, nicks, buttons) in msg-local coordinates
    fn hit_test(&mut self, id: &MessageId, pos: Point) -> Option<Hit>
    /// Clipboard contribution when selected (None = nothing copied)
    fn copy_text(&mut self, id: &MessageId) -> Option<String>
}
```

Each type file pairs one scene node (properties/signals/methods, stable
across channels) with its per-id instances. All three implement the
`MessageType` trait from `msg/mod.rs`; the sketches below show only the
type-specific surface.

`msg/privmsg.rs` — the type with insertion semantics:

```rust
pub struct PrivMsgNode {
    /// Scene node handle; properties: nick_colors, url_text_color,
    /// url_bg_color, url_bg_border_*, action_text_color, cap_max_height
    /* node, renderer, executor handles */
    instances: HashMap<MessageId, PrivMsg>,
}

pub struct PrivMsg {
    /// nick, text, is_action, is_notice, confirmed, expanded
    pub data: PrivData,
    /// Live handles resolved at new() (inherited or type-local)
    pub props: PrivProps,
    /// Txt layout, url click rects, cached draw instrs, height
    pub rendered: PrivRendered,
}

// Methods: insert_line, insert_unconf_line (persist via the loader,
//          buffer insert, materialize if visible), confirm(id) —
//          mark an unconfirmed message confirmed (rewrite payload in
//          the kv tree, update data, regen for styling)
// Signals: nick_clicked(id, nick), url_clicked(id, url)
```

`msg/filemsg.rs` — status lifecycle + eviction-surviving tasks:

```rust
pub struct FileMsgNode {
    /// Properties: max_height, margins, box styling, glow
    /* node, renderer, executor handles */
    instances: HashMap<MessageId, FileMsg>,
    /// Content-scoped tasks keyed by file url; survive release,
    /// dedup re-materialization, drain on stop()
    tasks: HashMap<Url, Task<()>>,
}

pub struct FileMsg {
    /// file_url, status, imgbuf (Arc<SyncMutex<Option<…>>>)
    pub data: FileData,
    pub props: FileProps,
    /// Status box / image meshes, active_rect, height
    pub rendered: FileRendered,
}

// Method:  set_file_status(url, status)
// Signals: fileurl_detected(url), download_request(id, url),
//          status_changed(id)
```

`msg/datemsg.rs` — derived day separators (see next section):

```rust
pub struct DateMsgNode {
    /// Properties: color (font/size inherited from the chatview)
    /* node, renderer, executor handles */
    instances: HashMap<MessageId, DateMsg>,
}

pub struct DateMsg {
    /// Local-midnight ts of the labeled day (from the record payload)
    pub data: DateData,
    pub props: DateProps,
    /// Rendered label line, height = line_height
    pub rendered: DateRendered,
}

// No methods, no signals. Never persisted. Copy text: the date label.
```

A question about the system maps to exactly one file (buffer↔ordering/
geometry, scroll↔gestures/animation, loader↔persistence/coverage,
msg/↔rendering/interaction).

### Derived records: how datemsg interleaves

Date separators occupy vertical space, so they must participate in
geometry exactly like any other message — but they have no storage and
no network identity. The model: separators are **derived records owned
by the buffer**, materialized through the ordinary `datemsg` type. A
separator for day D gets a synthetic composite key
`(ts = local_midnight(D), id = [0; 32])`, and its payload is the
midnight timestamp. Because every message of day D has
`ts >= midnight(D)`, and every message of any older day has
`ts < midnight(D)`, the ordinary `(ts, msg_id)` ordering places the
separator **exactly at the boundary of D's day-run** — no special
positioning logic exists anywhere; the zero id only breaks the
exact-midnight tie, sorting the separator older than a message stamped
at precisely midnight.

```
display order (bottom of screen = newest)
───────────────────────────────────────────
 privmsg  10:02   Aug 30   ┐
 privmsg  09:58   Aug 30   ┘ day-run D₂
 ◆ "Sun 30 Aug 2026"       key (midnight Aug 30, 0)
 filemsg  23:41   Aug 29   ┐
 privmsg  23:12   Aug 29   │ day-run D₁
 privmsg  01:03   Aug 29   ┘
 ◆ "Sat 29 Aug 2026"       key (midnight Aug 29, 0)
 …older / still loading…
```

Buffer invariant, maintained on every structural change: **for every
maximal same-day run in the loaded order, exactly one separator record
keyed to that day exists.**

```rust
// Inside every structural batch (insert / remove / reload):
fn sync_separators(&mut self, batch: &[MsgRecord]) {
    // A record starting a new day-run gets its separator next to it
    for rec in day_run_heads(batch) {
        if !self.has_separator(rec.day()) {
            self.insert_derived(separator_for(rec.day()));
        }
    }
    // A day-run that became empty leaves an orphan separator behind
    for day in orphaned_days(batch) {
        self.remove_derived(separator_of(day));
    }
}

fn separator_for(day: NaiveDate) -> MsgRecord {
    let midnight = local_midnight_ts(day);
    MsgRecord {
        ts: midnight,
        id: MessageId([0; 32]),       // zero id sorts older at the
                                      // exact-midnight tie
        msg_type: MsgType::DateMsg,
        payload: encode(&midnight),
        ..
    }
}
```

The maintenance paths:

- **Load (loader)**: `derive_separators` runs per record as the batch
  is collected (see the loader sketch); records and separators enter
  the buffer together, one Fenwick rebuild covering both. The topmost
  separator exists because loading stops mid-history; when an older
  day's messages load, its separator keys naturally to its own
  midnight — the previous separator needs no move.
- **Live insert (privmsg insert_line)**: `sync_separators` over the
  single-record batch; a record starting a new day-run inserts its
  separator next to it.
- **Removal (delete_line)**: if the last message of a day-run is
  removed, the run disappears and the orphan separator is removed in
  the same batch. This is the path the live-testing deletion exercises.
- **Filter/reload**: separators re-derive from whatever records the
  filter admits; they are never filtered themselves.

Separators count toward `total_height`/`visible_range` like any record,
are hit-tested/materialized through the registry like any type, are
selectable like any line (contributing their date label to copy), and
are never written to the kv tree.

### Buffer: arena + order index + Fenwick tree

Records live in a slotmap arena (`{ts, msg_id, msg_type, payload,
height}`); a `Vec<u32>` of arena slots ordered by `(ts, msg_id)`
is the display order; a Fenwick tree over heights in that order answers
`total_height`, `visible_range(scroll)`, and `pos_of(id)`. The per-frame
paths use only those queries:

```rust
// Draw path — cost is O(log n) + O(visible), never O(buffer):
let total = buffer.total_height();
let max_scroll = (total - view_h).max(0.);
let scroll = controller.clamp(requested, max_scroll);

for idx in buffer.visible_range(scroll, view_h) {
    let rec = buffer.record_at(idx);
    let node = type_node(rec.msg_type);
    node.materialize_if_needed(rec);
    instrs.extend(node.draw(&rec.id, &rect, &renderer));
}
```

Height changes and newest-position inserts are O(log n) point updates;
structural edits that move display positions (mid-array backfill
inserts, removals) are applied by the loader in batches with a single
O(n) rebuild per batch (see the Fenwick section):

```rust
// Live arrival (hot path): newest end, no positions move
buffer.insert(rec);              // order.push + fenwick.push

// Backfill batch (loader): positions shift — one rebuild covers it
buffer.insert_batch(batch);      // merge into order, then
                                 // fenwick.rebuild(heights)
```

Rejected alternatives: per-message scene nodes (rejected by requirement
— one node per type); plain `Vec<Message>` with linear scans (the
current design and the O(n) hot paths this change exists to remove);
uniform row quantization (heights are inherently variable — images,
expansion — so a fixed row unit is a lie while the expensive parts
remain parley layout and draw submission).

Heights come from the owning type node when a message is materialized
(wrapped at the current width) — the buffer never lays out text:

```rust
// materialize() measures; the height flows back into geometry
let h = node.materialize(rec)?.height;
let delta = buffer.set_height(&rec.id, h);   // fenwick.add(idx, h − old)
```

First layout of a message is the only remaining
linear-in-loaded-messages cost; it happens inside the loader,
incrementally. Full re-layout on resize is O(loaded) parley runs —
accepted; resize is rare and buffers are bounded by eviction pressure.

### Fenwick tree: what it offers and why

Every geometric question the chatview asks is a **prefix-sum question
over mutable heights**: total content height, which messages fall inside
`[scroll, scroll + view_h)`, and where message M sits. The summands
change constantly — inserts at any timestamp, async height changes
(image loads, expansion), filter rebuilds. A Fenwick tree (binary
indexed tree) is the minimal structure that answers both directions at
O(log n): a flat `Vec<f32>` laid parallel to the order index where entry
`i` holds the partial sum of a bit-aligned block of heights ending at
`i`. Walking bit patterns instead of scanning messages gives:

- `prefix(i)` — cumulative height of messages `0..i` (from the live
  bottom upward): O(log n). `total_height` is one call.
- `lower_bound(px)` — descend the tree to find the message containing
  pixel distance `px` from the bottom, never touching per-message
  heights: O(log n). The visible range at a scroll position is two
  calls (`scroll`, `scroll + view_h`); `pos_of(id)` is its inverse.
- `add(idx, δ)` — height change `h → h'` (`δ = h' − h`) and end-appends:
  O(log n), touching ~log n floats. Structural edits that *move*
  display positions (mid-array backfill inserts, removals) are not
  point-updates — the Fenwick is rebuilt from the order index in one
  O(n) pass per loader batch, keeping the structural cost off the
  frame path (µs at 10k records, under the loader's lock, amortized
  across the whole batch).

This is what makes the buffer-size-independence requirement structural
rather than hopeful: the per-frame costs of scrolling (clamp, visible
range, draw window) are O(log n) + O(visible), independent of how much
history is loaded, and height changes from async content updates are
cheap point-adds.

The tree lives in `src/util/fenwick.rs` as a plain reusable structure
(it knows nothing about messages); `buffer.rs` owns the invariant that
the Fenwick mirrors the order index. Basic design and API:

```rust
pub struct Fenwick {
    /// Partial sums, 1-indexed; node i covers the (i & -i) items
    /// ending at i
    tree: Vec<f32>,
    /// Number of items
    len: usize,
}

impl Fenwick {
    /// Build from display-order values (O(n))
    pub fn new(vals: &[f32]) -> Self
    /// Append an item (O(log n)) — live-arrival hot path
    pub fn push(&mut self, val: f32)
    /// Current value at position idx (O(log n))
    pub fn get(&self, idx: usize) -> f32
    /// val += delta at idx (O(log n)) — height changes
    pub fn add(&mut self, idx: usize, delta: f32)
    /// Overwrite the value at idx (O(log n))
    pub fn set(&mut self, idx: usize, val: f32)
    /// Sum of [0, idx) (O(log n)) — total_height, pos_of
    pub fn prefix(&self, idx: usize) -> f32
    /// Sum of [from, to) (O(log n))
    pub fn range(&self, from: usize, to: usize) -> f32
    /// First idx whose cumulative sum exceeds `target` (O(log n)) —
    /// px-from-bottom position → message lookup
    pub fn lower_bound(&self, target: f32) -> usize
    /// Full rebuild from new values (O(n)) — structural batches
    pub fn rebuild(&mut self, vals: &[f32])
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
}
```

Internals — the classic implicit layout: no node objects, no pointers,
one flat `Vec<f32>` where `tree[i]` sums the `i & -i` items ending at
`i`:

```rust
// Sum of items [0, i): walk downward, stripping the low bit
fn prefix(&self, mut i: usize) -> f32 {
    let mut sum = 0.;
    while i > 0 {
        sum += self.tree[i];
        i -= i & i.wrapping_neg();
    }
    sum
}

// Apply a delta at item i: walk upward through every covering node
fn add(&mut self, mut i: usize, delta: f32) {
    i += 1;
    while i <= self.len {
        self.tree[i] += delta;
        i += i & i.wrapping_neg();
    }
}

// lower_bound(target): descend top-down through the implicit tree,
// accumulating node sums, landing on the item containing `target`
// without ever touching per-message heights.
// get(idx) = prefix(idx + 1) - prefix(idx)
```

`src/util/fenwick.rs` carries randomized unit tests comparing
every operation against brute-force sums over a reference `Vec<f32>`.

Alternatives considered:

- **Precomputed flat prefix-sum array** — O(1) queries but O(n)
  recompute on every insert/height change; per-frame recompute is
  exactly the current `calc_total_height` failure mode this change
  removes.
- **Segment tree** — same asymptotics as Fenwick, but a recursive node
  structure with more code and worse cache behavior. It only pays off
  for range assignments or min/max queries (e.g. "first visible
  message with property X"), which this design doesn't need; if such a
  query appears later, Fenwick swaps out behind `buffer.rs`'s API.
- **Skiplist over absolute px positions** (an idea sketched in the old code
  comments) — pointer-chasing per level, worse constants, and it
  indexes absolute positions that every below-insertion invalidates;
  sums compose better than absolute positions under mutation.
- **Quantized uniform rows** — rejected in the buffer section:
  variable heights are the point (images, expansion), so a fixed row
  unit is a lie that still needs this machinery for accuracy.

Costs/limits accepted: float summation means `prefix` can drift by
rounding as deltas accumulate. Heights are bounded (~1e5–1e6 px total)
and update counts per session are far below f32 precision limits
(~7 significant digits), so drift stays sub-pixel; the buffer's
randomized unit tests compare Fenwick results against exact brute-force
sums, and a cheap full rebuild (O(n) adds) is available as a
periodic/manual re-anchoring tool if drift is ever observed.

### Scroll: internal pixels-from-bottom, exposed minimally

Scroll position is **internal controller state**, not a scene property:
a plain `f32`, pixels from content bottom (0 = live bottom), backed by
O(log n) geometry. Rationale: compensation and animations mutate it
constantly (per height change, per animation frame); as a scene
property every mutation would fire property events, atomic guards, and
subscriber notifications — overhead plus ordering hazards, for no
consumer that needs it. The only external consumers are satisfied by:

```rust
// Scene surface for scrolling (everything else is internal):
//   method:  scroll_to_bottom()     // the down-arrow button
//   property: is_at_bottom: bool    // arrow visibility; the chatview
//                                    // sets it when scroll hits 0
```

The controller state machine replaces the old shared `speed` scalar:

```
State ::= Idle
        | Drag  { grab_y, scroll0 }         1:1, no animation
        | Glide { velocity }                exponential decay
        | Anim  { from, to, t0, ease }      wheel/PageUp/PageDown
```

Inputs are intents; each intent names the state it produces, so
gestures never smear into a shared scalar:

```rust
// Grabbing: 1:1 tracking, cancels any in-flight motion
pub fn drag_start(&mut self, y: f32) {
    self.state = ScrollState::Drag { start_y: y, scroll0: self.scroll };
}

// Wheel/PageUp/PageDown: retarget the animation — never add velocity.
// Repeated ticks extend `to` from the current target (coalescing).
pub fn page_tick(&mut self, dir: f32, page: f32) {
    let base = match self.state {
        ScrollState::Anim { to, .. } => to,
        _ => self.scroll,
    };
    let to = self.clamp(base + dir * page);
    self.state =
        ScrollState::Anim { from: self.scroll, to, started: Instant::now() };
}

// Flick: the session pre-samples release velocity (DragEnd.vel, px/sec
// over its 40ms window); flick is this controller's threshold on it
pub fn drag_end(&mut self, velocity: f32) {
    self.state = ScrollState::Glide { velocity };
}

// The down-arrow: teleport to bottom, cancel all motion
pub fn scroll_to_bottom(&mut self) {
    self.state = ScrollState::Idle;
    self.scroll = 0.;
}
```

One animator task drives Glide/Anim on a deadline cadence (computed
from the easing curve) instead of the fixed 10ms tick, writing
`self.scroll` directly and triggering redraw. Bottom clamps hard at 0;
top clamps at loaded content (loader extends coverage as the viewport
nears the top of the loaded region).

`compensate()` is the single entry point for the height-change rule —
the chatview calls it after `buffer.set_height` reports a delta:

```rust
/// Height-change compensation. When a message entirely below the
/// viewport bottom changes height by `delta`, the content the user is
/// looking at must not move: since scroll measures from the content
/// bottom, keeping the same content in view means adding `delta`.
/// At scroll == 0 (bottom pinned) there is nothing to hold — the
/// content grows upward and the view auto-follows, so no adjustment.
/// Changes overlapping or above the viewport are visible growth by
/// design (image expanding in place) and are also left alone.
impl ScrollController {
    pub fn compensate(&mut self, delta: f32, msg_below_viewport: bool) {
        if msg_below_viewport && self.scroll > 0. {
            self.scroll += delta;
        }
    }
}

// Caller side (after a regen reported a new height):
let top = buffer.pos_of(&id);               // measured before update
if let Some(delta) = buffer.set_height(&id, new_h) {
    controller.compensate(delta, top <= controller.scroll());
}
```

This one rule covers live arrival while reading history (stable),
pinned bottom (auto-follow), and in-viewport expansion (grows in
place). Deferred alternative: anchor-based core (`(msg_id, dy)` as
ground truth) — more robust to mass reflow but more machinery; upgrade
path is to keep the controller API and swap the internal representation.

#### Anchors: what they are and how inserts affect them

An `Anchor` is the serialization of "what the user is looking at",
used only at save/restore boundaries (channel exit/entry, reflow) —
runtime stability is `compensate()`'s job, never the anchor's:

```rust
pub struct Anchor {
    /// Message whose top edge is at or above the viewport top edge —
    /// the oldest visible message. None = bottom (scroll == 0).
    pub msg: Option<MessageId>,
    /// dy = (scroll + view_h) − pos_of(msg): how far the viewport top
    /// sits below the anchor message's top, in px. 0 = the message's
    /// top is exactly at the viewport top; larger = it is further
    /// down (partially scrolled past).
    pub dy: f32,
}
```

Anchoring at the **viewport top** message makes the anchor immune to
inserts by construction:

- **Inserts below (newer messages arriving, live bottom growing)**:
  `pos_of(anchor)` shifts, but the anchor is expressed relative to the
  message itself — restore recomputes from `pos_of`, so the same
  content reappears at the same place.
- **Inserts above (older messages backfilled during sync)**: same —
  the anchor names content, not a position relative to the bottom; whatever
  shifted above it does not move it.
- **The anchor message disappearing** (deleted): restore clamps to a
  valid scroll (nearest valid position) — an explicit, logged
  fallback, never a silent jump.

```rust
// Snapshot — cheap, no persistence (the chatview's per-channel state
// map persists it on channel exit):
pub fn anchor(&self) -> Anchor { /* per the definition above */ }

// Resolve — after the loader's coverage reaches the anchor:
//   scroll = pos_of(msg) + dy − view_h, clamped
```

The method is `anchor()` (a snapshot), not `save_anchor()` — nothing
is persisted by the call itself; persistence is the chatview storing
the snapshot in its per-channel state map on exit.

### Gesture input integration

Touch input arrives through the gesture subsystem landed by
`app-gesture` (`src/ui/gesture/`, applied): the window `GestureSession`
owns recognition and delivers a `GestureAction` stream;
`handle_touch` no longer exists on `UIObject`. ChatView2 declares a
chatview-shaped `GestureSet` (tap + long-press + vertical drag after
slop) and maps actions to its own machinery — it runs no recognizers,
timers, or velocity sampling of its own:

| GestureAction | ChatView2 handling |
|---|---|
| `Down` | reset long-press mode; pause inertia while the finger is down |
| `DragStart { start }` | `scroll.drag_start(start.y)` — grab kills Glide/Anim |
| `DragMove { curr, .. }` | 1:1 drag on the chat axis: `scroll = scroll0 + dy` (scroll grows back into history — the inverted-axis trap the old widget hit) — or, in long-press select mode, extend the selection instead of scrolling |
| `DragEnd { vel, .. }` | flick threshold on `vel.y` → `drag_end(velocity)` → Glide |
| `Tap { pos }` | hit-dispatch through the type registry (URL open, nick/file activation), else line-toggle selection |
| `LongPress { pos }` | URL under the finger → toast copy; else select line + enter selection mode (single-fire, timer-driven — session-owned) |
| `Up` | clear touch-active state; inertia eligible again |

Session-provided (never re-implemented): 10px touch slop (the dead-zone
before drag start), 300ms tap bound, long-press timeout (system,
timer-fired, once per touch), 20ms move delivery throttle (velocity
sampling still observes every move), release velocity (40ms window,
px/sec). Physics stays controller-side — inertia, decay, grab-to-stop,
clamping, anchoring — which is exactly the split `app-gesture` defers
to chatview2's scroll controller.

Two integration invariants from the applied session's post-review
audit:

- **Exact hit regions**: `gesture_hit_test` passes exactly the rect the
  view acts on — chain resolution has no per-event sibling fallthrough,
  so an over-broad region steals taps from overlaying buttons (the
  TokenTable lesson).
- **Explicit priorities**: the session walks children in node-priority
  order; every interactive layer floating over the chatview (the
  scroll-to-bottom arrow, the cmd-hint popup) carries `priority` +1
  above it, or equal-priority tie-breaks can eat its taps.

The migrated old ChatView (`handle_gesture` in `src/ui/chatview/mod.rs`)
is the behavioral parity reference for this mapping until phase 19
deletes it. Wheel and PageUp/PageDown stay desktop handlers
(`handle_mouse_wheel`, `handle_key_down`) feeding `page_tick` — the
gesture subsystem is touch-only; `EMULATE_TOUCH` desktop emulation
routes through the session, so the same paths are testable on desktop.

### Reflow: width, scale, and styling changes

Wrapping depends on viewport width, so any width change (window
resize/rotation), window_scale change, or styling change (font size,
timestamp width, cap height) invalidates the rendered state of every
loaded message of the affected types. Height-only viewport changes
(e.g. the chat editor growing) do not re-wrap — they only recompute
coverage and max scroll, and re-clamp.

Reflow reuses the anchor machinery from channel restore:

```rust
// Width/scale/styling change → anchor protocol, NOT compensation
fn reflow(&mut self, atom: &mut PropertyAtomicGuard) {
    let anchor = self.controller.anchor();     // 1. before invalidating
    for node in self.affected_types() {         // 2. drop rendered state
        node.drop_rendered_all();
    }
    let heights = self.remeasure_all();         // 3. re-wrap, visible
    self.buffer.rebuild_heights(&heights);      //    first; one O(n)
                                                //    Fenwick rebuild
    self.controller.restore(&anchor, &pos_of);  // 4. pos_of recomputed
    self.redraw.trigger();
}
```

Step 3 in v1 is synchronous for all loaded messages — the O(loaded)
parley-run hitch the buffer section accepts; laying out the visible
window first keeps the frame correct. The same content stays under the
viewport across the reflow; a bottom-pinned view stays bottom-pinned.

The height-change compensation rule is deliberately NOT used for
reflow: it covers incremental async changes (one image loading, one
message expanding) where a single below-viewport delta keeps the view
stable. Mass rewrapping changes heights both above and below the
viewport, where only anchor restore is correct. If the synchronous
pass ever measures poorly on large buffers, the staged variant
(visible-first, loader corrects the remainder with stale heights in
the Fenwick until re-measured) is the upgrade path — same anchor
protocol, incremental invalidation.

### Loader: single background pipeline

One async task owns all kvdb access per chatview. Coverage invariant:
the loaded region always includes the live bottom and extends far
enough above the viewport to cover `viewport + preload margin`:

```rust
// The only kvdb reader/writer for this chatview
async fn run(mut self) {
    loop {
        self.cv.wait().await;
        self.pump().await;      // restores the coverage invariant
        self.cv.reset();
    }
}

async fn pump(&mut self) {
    let covered = self.buffer.total_height();
    if covered >= self.scroll + self.view_h + self.margin {
        return
    }

    // Iterate newest→older from the oldest loaded ts, decode,
    // apply the filter, derive separators, stop once covered:
    let mut batch = vec![];
    for entry in self.tree.range(..oldest_key).rev() {
        let rec = decode_entry(entry);          // panics on corrupt
        if !(self.filter)(&rec) {
            continue
        }
        batch.extend(derive_separators(&rec));  // day-run boundary
        batch.push(rec);
        if batch_height(&batch) >= shortfall {
            break
        }
    }
    self.buffer.insert_batch(batch);            // one Fenwick rebuild
    if batch_touches_viewport {
        self.redraw.trigger()
    }
}
```

Wake-ups: `set_channel`, scroll change nearing the top of coverage,
message insert, filter change, rect change. Insert of a live message
writes to kvdb and hands the record to the buffer directly (still
under the loader's lock ordering). Filter application happens here —
kvdb → filter → buffer — so filter swap is just a coverage rebuild
through the same path; filtered messages remain stored.

### Type nodes: data / props / rendered, rebuilt by regen

One sub-node per message type, created once with the chatview, stable
across channel switches. Per-id message instances owned by the type node:

```
msg instance = data      record payload + self-owned mutable state
                          (file status, image buffer, …) behind interior
                          mutability (Arc<SyncMutex<…>>)
              props     live property handles received in new()
              rendered  layouts, meshes, textures, hit rects, measured
                          height — a pure cache of (data, props)
```

`regen()` re-reads live props + current data and rebuilds rendered
state and height. Three triggers, one path:

```rust
// Trigger 1: a styling property changed → regen all of the type
fn on_styling_change(&self) {
    for id in self.instance_ids() {
        self.regen(&id);
    }
}

// Trigger 2: an async task updated data (image decoded, status) → one
fn on_data_update(&self, id: &MessageId) {
    self.regen(id);
}

// Trigger 3 (materialize after eviction) calls the same verb via new()
fn regen(&mut self, id: &MessageId) {
    let data = self.instance(id).data();       // current data
    let rendered = self.render(&data);         // live prop handles,
                                               // re-layout, re-measure
    self.instance_mut(id).rendered = rendered;
    self.report_height(id, rendered.height);   // → buffer + compensation
}
```

Property inheritance is resolved at `new()` time: the type node's own
property if it defines one, else the chatview's — the same
`PropertyPtr`, so change notification arrives via existing
subscriptions and the handler is just "regen".

Task classes: render-scoped (spawn on materialize, cancel on release —
animations, progressive decode) and content-scoped (hosted on the type
node, surviving eviction and optionally channel switches for
content-addressed work like fud downloads):

```rust
// Content-scoped hosting: keyed by content address, dedups
// re-materialization, survives release, drains on stop()
fn ensure_download(&mut self, url: &Url) {
    self.tasks.entry(url.clone()).or_insert_with(|| {
        self.ex.spawn(async move { download(url).await })
    });
}

// materialize(id) → ensure_download(&url): attach, never duplicate
// release(id)    → render-scoped tasks cancelled; downloads continue
// stop()         → all tasks dropped
```

For fud the plugin remains the true task host — the node-hosted task
may simply be a status subscription feeding the data layer.

### Testability: unit coverage of major codepaths

Most major codepaths MUST be unit-testable without a GPU or a window.
The structural rule that makes this true: **renderer-dependent work
(mesh/texture allocation) stays at the draw edge; everything else is
CPU-only.** Concretely:

```rust
// parley layout needs only fonts (data/font), not a GPU context:
let layout = text::make_layout2(text, color, font_size, lineheight,
    window_scale, Some(width), &[], &fg_ranges, align, wrap);
let height = layout.height();          // measurable in a unit test

// so the msg instance's `rendered` splits into:
//   testable:  txt layout, measured height, url hit rects, copy text
//   renderer-  mesh/texture caches (verified visually + via trace
//   bound:     logs, not by unit test)
```

Cache *behavior* is bookkeeping, not GPU state, and is unit-tested:
layout reused across draws until width/styling/data changes, regen
re-measures and reports, materialize-after-release rebuilds state, LRU
eviction order under budget pressure. The loader reads the kv tree
through its tree handle, so unit tests build fixture trees in-test.
The eviction policy is a pure
budget component tested standalone. Unit-test surface by phase:
fenwick ops (1), ordering/dedup/removal (2), geometry + compensation
(3), codec round-trip + corrupt-entry panic (4), scroll transitions/
anchors (5), type
framework cache lifecycle (8), privmsg layout cache + invalidation
(9), mixed-type copy ordering (11), separator sync/orphans (12), LRU
policy (14), cap/expand measurement (15). Trace logs and netdebug
verify the live integration of exactly these paths; they never
substitute for the unit tests.

### Selection: view-wide state, per-type copy text

Every line is selectable regardless of type (the old chatview exempted
date separators — dropped). Selection state and rendering live on the
chatview; what a selected line *contributes to the clipboard* is
defined by its type.

```rust
// ChatView2 (mod.rs) owns the state:
//   selected: HashSet<MessageId>
// Highlight is chatview-drawn: a filled rect of the message's extent
// (geometry comes from the buffer) in hi_bg_color, drawn behind the
// type's instructions — so selecting never invalidates a type's
// rendered cache and types stay ignorant of selection.

// Toggling: hit-test the y position against the buffer window
async fn select_line(&self, y: f32) {
    if let Some(rec) = self.buffer.hit(y, self.scroll, self.rect) {
        self.selected.insert(rec.id.clone());
        self.redraw.trigger();
        self.notify_select_changed().await;
    }
}

// Copying: display order, per-type contribution, join by newlines
fn copy_text(&self) -> String {
    self.buffer
        .iter_display_order()                  // newest→older
        .filter(|rec| self.selected.contains(&rec.id))
        .filter_map(|rec| self.type_node(rec.msg_type).copy_text(&rec.id))
        .collect::<Vec<_>>()
        .join("\n")
}
```

Per-type copy text: privmsg contributes its rendered line
(`<nick> text`, action/notice variants as displayed); filemsg
contributes its file URL; datemsg contributes its date label; future
types decide for themselves (a type MAY contribute nothing). Selection
gestures in gesture-stream terms (mouse click toggle, `Tap` toggle,
`LongPress` entering selection mode with subsequent `DragMove`
extending the selection instead of scrolling, drag sweep,
selection-mode taps), `unselect`, and the `select_changed` transition
signal are unchanged from the migrated chatview and operate uniformly
over all types.

### Wire format

kv key: 8-byte BE timestamp + 32-byte msg id (composite key is
required — same-millisecond collisions are real and derived filemsgs
intentionally share their source privmsg's timestamp). Value is
`[u8 tag][type-owned bytes]` (the tag is the `MsgType` discriminant);
rest. The privmsg payload after the tag is `nick, text` plus a
`confirmed` flag (confirmed is privmsg-owned state, living in the
payload — not record state).

**No backward compatibility.** The old chatview's untagged values are
not readable by chatview2 and no legacy decode path exists. Corrupt or
unknown data is an explicit, loud failure — decoding an unknown type
id panics; errors are never silently swallowed:

```rust
// key:   [u64 BE ts][32-byte msg id]
// value: [u8 tag][type-owned bytes]  (tag = MsgType discriminant)
//
// privmsg (MsgType::PrivMsg):
//   [nick: String][text: String][confirmed: bool]

fn decode_value(val: &[u8], ts: Timestamp, id: MessageId) -> MsgRecord {
    let (&tag, rest) = val.split_first().expect("empty value");
    let msg_type = msg_type_from_u8(tag);        // panics on unknown
    let payload = msg_type.decode_payload(rest).expect("bad payload");
    MsgRecord { ts, id, msg_type, payload, height: 0. }
}
```

Old channel trees therefore cannot be opened by chatview2; the
migration is a clean break (see Migration Plan), not a decode concern.
Storage engine decision: **kvdb stays.** The access pattern is ordered
range scans over `(ts, msg_id)` — a B-tree — and storage I/O is a
negligible fraction of load cost (layout/wrapping dominates by orders
of magnitude), so switching engines cannot meaningfully improve
performance. Turso/libsql would add a heavy dependency and a SQL
mapping for zero gain on this pattern (its strengths — SQL queries,
replication, remote access — are unused here), and adding a dependency
is a supply-chain decision requiring human review regardless.

### Channel switching and app integration

`set_channel(name)` looks up the channel's tree from an internal
registry (kvdb handle given at construction), saves the outgoing
channel's scroll anchor, releases the buffer, and lets the loader
refill:

```rust
// Single method; the caller already knows, so no signal
async fn set_channel(&mut self, channel: String) {
    let anchor = self.controller.anchor();
    self.channel_state.insert(self.current_channel.clone(), anchor);
    self.buffer.lock().await.clear();          // release in-memory state
    let tree = self.tree_registry.tree(&channel);
    self.loader.bind(tree).await;              // refill in the bg
    // Scroll restore resolves when coverage reaches the saved anchor
}
```

`src/app/schema/chat.rs` is reworked to build the screen once; the
per-channel `chat::make()` loop in `schema/mod.rs` disappears; the
channel label and relay paths in `main.rs` retarget the single chatview
via `set_channel`. Unread highlighting in the menu keys off
message-received events carrying the channel instead of per-screen
layers. Overlaying interactive layers (scroll-to-bottom arrow, cmd-hint
popup) keep explicit `priority` above the chatview2 node — the gesture
session resolves targets in priority order (see the gesture input
integration section).

## Risks / Trade-offs

- [Fenwick/bookkeeping bugs corrupt geometry] → buffer unit tests over
  insert/remove/height-change sequences with random operations and
  invariant checks (prefix sums vs brute force).
- [Panic on corrupt db data takes down the app] → intentional:
  corrupt/unknown entries are explicit failures, never silently
  skipped; the panic message names the channel tree, key, and type id
  so the offending entry can be found and removed.
- [Compensation rule degrades under mass reflow (resize/regen)] → mass
  reflow never uses the compensation rule; it follows the anchor
  snapshot/restore protocol (see Reflow section). If synchronous
  rewrap hitches on large buffers, stage it via the loader
  (visible-first, stale heights corrected incrementally).
- [Deep scroll restore loads+measures everything newest→anchor] →
  background-only, content streams in; if it measures poorly, build the
  deferred layout sidecar.
- [Per-type node `HashMap<id, …>` grows without pressure] → same LRU
  budget question as eviction; instrument before tuning (non-strict by
  decision).
- [Two chatviews during migration window (old screens + chatview2)] →
  switchover is a single cutover task in the sequence; old module deleted
  in the same change once parity tests pass.
- [Over-broad gesture hit region or missing overlay priority steals
  taps] → `gesture_hit_test` passes exactly the view rect; every
  floating interactive layer carries `priority` +1 (the session resolves
  targets in priority order, no sibling fallthrough) — verified on the
  dev screen with the arrow layer overlaid.

## Migration Plan

1. Land chatview2 alongside the old module (unused) — no behavior change.
2. Cutover `schema/chat.rs` + relays to the single screen; delete
   `src/ui/chatview/`.
3. Storage is a clean break, deliberately: chatview2 uses new per-channel
   trees (versioned names, e.g. `{channel}__chat_tree_v2`), so old trees
   are ignored rather than misread — history is rebuilt by a darkirc
   rescan of the DAG. Rollback before cutover is trivial (old path and
   old trees untouched); after cutover, rollback = revert the cutover
   commit (new trees are simply ignored by the old code).

## Development Protocol

tasks.md is sequenced as 19 phases, each ending at a **logically atomic
commit point**. A phase closes only through its gate: the phase's
verification runs and its evidence is checked, the owner performs a
thorough code review (amendments and clarifications are applied to the
code before proceeding), and only then is the phase committed. Work on
the next phase does not begin before the gate passes. Verification
tooling available at every phase:

- **Unit tests** (`cargo test` in `bin/app`) for pure components —
  fenwick, buffer, codec, scroll controller.
- **netdebug** (zeromq scene backend, enabled by default in `make dev`
  features): drive and inspect the live scene from a CLI — `CallMethod`
  (`insert_line`, `set_channel`, `get_line_ids`, `delete_line`),
  `GetPropertyValue` (e.g. `is_at_bottom`), node/property introspection, and
  signal subscriptions over the pub socket.
- **Trace logs** (`ui::chatview2*` log targets with filelog): assert
  internal behavior — loader batches, Fenwick rebuilds, layout cache
  hit/miss, materialize/release lifecycle, scroll state transitions.
- **Visual inspection** on the schema-chatview dev screen, compared
  against the old chatview where parity applies.

The dev screen — `src/app/schema/test_chatview.rs`, selected by the
`schema-test-chatview` cargo feature (following the existing
`schema-test-*` convention and its one-schema-at-a-time compile guard)
— lands in phase 6 and carries all interactive verification until the
cutover phases rework the real chat screen.

## Open Questions

- Eviction budget shape (bytes vs entry count, per-type vs global) —
  tuning detail, decided during implementation.
- `set_channel` argument is the channel name with an internal tree
  registry (assumed in design); if the schema layer prefers passing tree
  handles, it is a constructor-signature change only.
- Exact signal payload encodings (field order in the encoded data vec) —
  pinned down per signal while writing the msg nodes.
