## Context

Touch input currently enters the app on the miniquad Stage thread and splits into
two dispatch paths with different ordering and different visibility:

```
                       miniquad Stage thread
                              │ touch_event()
                              ▼
                ┌───────────────────────────────┐
                │  handle_touch_sync()  (sync)  │   BaseEdit: Started+Moved
                │  returns true ⇒ event         │   Menu:     Started+Moved
                │  is swallowed entirely        │   Button:  Started
                └──────────────┬────────────────┘
                               │ if unclaimed
                               ▼
                 event_pub.notify_touch() → executor hop
                               │
                               ▼
                ┌───────────────────────────────┐
                │  Window::handle_touch (async) │
                │  1. GestureProcessor::process │   inert: nothing consumes
                │  2. raw handle_touch fallback │   full tree, coord-translated
                └───────────────────────────────┘
```

Five overlapping recognizers exist (see proposal.md - Why): the window-level
`GestureProcessor` (landed from the earlier attempt, commits `9f16bed6f` /
`56f1f8c60`, currently dead dispatch), and hand-rolled state machines in
ChatView, Menu, BaseEdit, and EmojiPicker. Recognition constants diverge by two
orders of magnitude (tap strictness 0.05px in ChatView/Menu vs 10px in the
processor). `handle_gesture` cannot propagate past the first `Layer` and is not
hit-tested. The `EMULATE_TOUCH` desktop path bypasses the processor entirely.
`ui/gesture.rs` (two-finger pinch node) is dead code.

Relevant prior decisions: `BaseEdit`'s Stage-thread handling exists for
selection-handle drag latency (its design.md D6 accepted one hop to the
serialized redraw pass); ChatView's scroll already runs fully async with a 20ms
throttle, so the async hop is proven tolerable for scrolling.

## Goals / Non-Goals

Goals:

- Recognition mechanics (distance/time/velocity state machines) exist exactly
  once; gesture constants exist exactly once.
- Widgets shrink to a declarative contract: which gestures they accept, where
  they are hit, and what the gestures mean.
- One dispatch path with one ordering and one coordinate translation.
- Long-press that fires during hold (timer-driven), fixing the still-finger gap
  in the processor and in BaseEdit's move-triggered check.

Non-Goals:

- Pinch/multi-finger gestures in v1. The stream is shaped so a `Pinch`
  recognizer can be added without API break. Secondary touches keep today's
  "ignored, cannot disturb the primary" semantics.
- Mouse, wheel, and keyboard handling. Desktop keeps `handle_mouse_*`;
  `EMULATE_TOUCH` only gets routed through the session so emulated touches
  produce gestures.
- Scroll physics. Inertia, decay, grab-to-stop, anchoring stay widget-side
  (and become chatview2's scroll controller under `app-chatview`).
- Rewriting ChatView itself (`app-chatview` owns that; this change supplies the
  recognizer seam it is specified to consume).

## Decisions

### D1: Window-owned session + per-node configured recognizers

The window owns a `GestureSession`: the touch stream, target resolution,
timers, throttling, and arbitration. Recognition is a small library of pure
per-gesture state machines instantiated per accepting node with that node's
`GestureCfg`.

Alternatives rejected:

- Window god-object with one hardcoded behavior (the landed attempt's shape):
  legitimately different per-widget semantics (axis lock, drag direction,
  zero-threshold precision drags) cannot be expressed.
- Per-widget self-contained recognizers with no orchestration (Android
  `GestureDetector` style): keeps the targeting/timer/arbitration duplication
  this change exists to remove, and inherits the sync-path blindness.

This mirrors the convergent design of iOS `UIGestureRecognizer`/Flutter's
gesture arena: distributed recognizers, one central orchestrator.

### D2: The session is fed from the Stage thread

The feed point is `gfx` `touch_event`, before any sync claiming. A touch that
begins on a sync-claiming widget (BaseEdit) still drives recognition for its
target chain. The recognizer math runs inline (pure, cheap); timers are
executor tasks using the version-guard pattern ChatView/Menu already use.

### D3: One event stream, lifecycle included; flick is derived

```
Down ──▶ (recognition) ──▶ Tap | LongPress | DragStart ─ DragMove* ─ DragEnd
```

`Down`/`Up` are delivered to the hit-tested target immediately, without
recognition, replacing the raw handlers' remaining legitimate uses (press
visuals, precision-grab arming, cleanup). `DragEnd` carries end velocity;
flick is the consumer's threshold on that velocity (ChatView's
`scroll_start_accel · dist/time` formula reproduces exactly), so no separate
`Flick` event is minted.

### D4: Sticky ownership via hit-test chain resolved at touch start

At `Down`, the session walks the tree (existing priority ordering) and resolves
the chain of `gesture_hit_test` passers; all events for that touch id go to that chain
until `Up`/cancel. A touch that wanders into a sibling mid-gesture does not
hand off — the iOS behavior, strictly more predictable than today's
per-phase re-propagation.

### D5: Arbitration is first-resolved-wins

All recognizers in the target chain observe the stream; the first to resolve
claims (events delivered to its node), the rest cancel. Tap-vs-drag is
mechanical via slop/timeout; child-vs-parent (row tap inside a scrollable
menu) resolves as "tap wins within slop, drag wins beyond it" — the standard
mobile contract. No Flutter-scale arena politics are needed at this app's
complexity.

### D6: Unified constants, Android-flavored

One source of truth, `long_press_timeout()` already pulled from Android
`ViewConfiguration`:

| Constant | Replaces (today) | Value |
|---|---|---|
| `touch_slop` | 10/15/10/10/5/0.5/0.05px scatter | 10px |
| `tap_max_duration` | 300ms / unbounded ×3 | 300ms |
| `long_press_timeout` | 500ms hardcoded / sys ×3, move-triggered | sys, timer-fired |
| `flick` sampling | 40ms window ×2 | 40ms, velocity on `DragEnd` |
| `move_delivery_period` | 20ms ×2, none ×2 | 20ms (raw samples still collected) |

Per-node config survives only where semantic: `axes` (y-lock for scrollers),
`direction` (BaseEdit's vertical/horizontal split), `min_travel: 0.` for
precision drags.

### D7: Async-only delivery; the sync path dies

All gesture delivery is async (executor), like today's `handle_touch` path.
Rationale: every visual feedback already gates on the serialized redraw pass;
the sync path only accelerates state mutation by one executor hop. ChatView
scroll proves the hop is imperceptible for the highest-frequency gesture.
Risk and fallback in Risks.

### D8: Deletions

The dead code goes first: the `ui/gesture.rs` pinch node (with its
`create_gesture` registration and `Pimpl::Gesture` variant) and `win/gesture.rs`
(with the dead `gesture_proc` dispatch in `Window::handle_touch`) are removed
when the module lands — the new `ui/gesture/` directory takes the pinch node's
module path, and Rust rejects `gesture.rs` and `gesture/mod.rs` coexisting.
All of it is dead code, so the early deletion is behavior-neutral.

At the end: `handle_touch`/`handle_touch_sync` leave `UIObject`; the four
widget `TouchInfo` machines are removed.

## The API

Constants and stream:

```rust
pub struct GestureConstants {
    /// Maximum travel between down and up that still counts as a tap.
    pub touch_slop: f32,
    pub tap_max_duration: u32,
    pub long_press_timeout: u32,
    pub move_delivery_period: u32,
    pub sample_window_ms: u32,
}

pub enum GestureAction {
    Down { pos: Point },
    Up { pos: Point },
    Tap { pos: Point },
    LongPress { pos: Point },
    DragStart { start: Point },
    DragMove { start: Point, prev: Point, curr: Point },
    DragEnd { start: Point, curr: Point, vel: Vector },
}
```

Widget contract on `UIObject`. `gesture_set` and `gesture_hit_test` are new;
`handle_gesture` already exists as dead dispatch taking the old
`win::GestureAction` and is re-pointed to the new type when the module lands
(the `ui::GestureAction` re-export moves from `win` to `gesture` in the same
step, or the two collide at `ui::` scope). Defaults keep non-participating
nodes inert:

```rust
fn gesture_set(&self) -> GestureSet {
    GestureSet::NONE
}

fn gesture_hit_test(&self, pos: Point) -> bool {
    false
}

async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    false
}
```

`GestureSet` composes recognizer configs:

```rust
pub struct GestureCfg {
    pub tap: Option<TapCfg>,
    pub long_press: Option<LongPressCfg>,
    pub drag: Option<DragCfg>,
}

pub struct TapCfg {
    pub axes: Axes,
}

pub struct DragCfg {
    pub axes: Axes,
    pub direction: Direction,
    pub min_travel: f32,
}
```

`Axes` (both/y-only/x-only) and `Direction` (any/vertical/horizontal) encode
the semantic per-widget differences; all numeric thresholds come from
`GestureConstants`.

Layer forwarding mirrors `handle_touch` today — subtract the layer origin,
recurse in priority order — and is written once:

```rust
async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    if !self.is_visible.get() {
        return false
    }

    let mut gesture = gesture;
    gesture.translate(-self.rect.get().pos());

    for child in self.get_children() {
        let obj = get_ui_object3(&child);
        if obj.handle_gesture(gesture.clone()).await {
            return true
        }
    }

    false
}
```

## Migration samples

### Button (tap)

Before: `handle_touch` + `handle_touch_sync` + `handle_mouse_btn_down/up`
(~110 lines) simulating mouse events, gated by an atomic `mouse_btn_held`
flag, with no movement threshold.

After — the entire touch surface:

```rust
fn gesture_set(&self) -> GestureSet {
    GestureSet::TAP
}

fn gesture_hit_test(&self, pos: Point) -> bool {
    self.is_active.get() && self.rect.get().contains(pos)
}

async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    let GestureAction::Tap { pos: _ } = gesture else {
        return false
    };

    let node = self.node.upgrade().unwrap();
    node.trigger("click", vec![]).await.unwrap();

    true
}
```

The `mouse_btn_held` gate disappears: down→up within slop *is* the click
validity check. Mouse handlers remain for desktop. Accepted delta: the
sloppy-drag-that-returns no longer clicks (now slop-bounded, standard).

### EmojiPicker (scroll + tap)

Before: 65 lines of local `TouchInfo { start_pos, start_scroll, is_scroll }`
deciding scroll-vs-tap at a 0.5px y threshold.

After:

```rust
fn gesture_set(&self) -> GestureSet {
    GestureSet::SCROLL_VERT
}

fn gesture_hit_test(&self, pos: Point) -> bool {
    self.rect.get().contains(pos)
}

async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    match gesture {
        GestureAction::DragStart { start } => {
            *self.drag_state.lock() = Some((start.y, self.scroll.get()));
            true
        }
        GestureAction::DragMove { curr, .. } => {
            let Some((start_y, start_scroll)) = *self.drag_state.lock() else {
                return false
            };

            let scroll = (start_scroll + start_y - curr.y).clamp(0., self.max_scroll());
            let atom = &mut self.redraw.make_guard(gfxtag!("EmojiPicker::drag"));
            self.scroll.set(atom, scroll);
            self.draw_cache.clear();
            true
        }
        GestureAction::Tap { pos } => {
            let rect = self.rect.get();
            self.click_emoji(pos - rect.pos()).await;
            true
        }
        _ => false,
    }
}
```

Flick inertia for EmojiPicker is an open adoption decision (see Open
Questions); the `DragEnd { vel }` input makes it a five-line addition.

### ChatView (scroll + flick inertia + long-press select + tap)

Before: ~250 lines inline in `handle_touch` — `TouchInfo` with a 40ms sample
queue, a long-press timer task with a `touch_hold_version` guard, 20ms move
throttling, tap forwarding gated on 0.05px y-travel, `end_touch_phase`
acceleration math, grab-stops-inertia via a 200ms rule.

After: `handle_touch` disappears; the recognizer supplies the semantics:

```rust
fn gesture_set(&self) -> GestureSet {
    GestureSet::CHATVIEW
}

async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    match gesture {
        GestureAction::DragStart { .. } => {
            // A grab kills running inertia (today's >200ms rule)
            self.speed.store(0., Ordering::Relaxed);
            *self.drag_state.lock() = Some(self.scroll.get());
            true
        }
        GestureAction::DragMove { curr, .. } => {
            // 1:1 finger scroll via scrollview(), clamped
            true
        }
        GestureAction::DragEnd { vel, .. } => {
            // Feed inertia: accel = scroll_start_accel * vel.y
            self.speed.fetch_add(accel, Ordering::Relaxed);
            self.motion_cv.notify();
            true
        }
        GestureAction::LongPress { pos } => {
            // URL toast if on_url, else select_line + select mode
            true
        }
        GestureAction::Tap { pos } => {
            // Forward to message (URL/file), else toggle line selection
            true
        }
        _ => false,
    }
}
```

The inertia loop, `scroll_resist` decay, and the motion task are untouched —
recognition and physics stay separated. `PrivMessage`/`FileMessage` handlers
keep their exact code and are invoked from the `Tap` branch instead of the
inline tap check.

### Menu (long-press edit mode + reorder drag + tap)

Before: sync `Started`/`Moved` + async `Ended` juggling, `TouchInfo` +
`DragInfo`, a cancellable long-press task, double long-press evaluation
(timer during hold and `elapsed` at end).

After: one `LongPress` event (single-fire by construction). The hamburger
item-reorder grab stays a `Down`-armed precision drag — grabbing an icon is a
zero-threshold action, not a recognized gesture:

```rust
async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    match gesture {
        GestureAction::Down { pos } => {
            // Arm DragInfo if pos is on the hamburger of a row in edit mode
            true
        }
        GestureAction::DragMove { curr, .. } => {
            // Update insert_idx of the armed reorder, invalidate draw
            true
        }
        GestureAction::Up { .. } => {
            // Commit reorder if armed and indices differ
            true
        }
        GestureAction::LongPress { .. } => {
            // Enter edit mode, fire "edit_active"
            true
        }
        GestureAction::Tap { pos } => {
            // handle_selection or X-delete in edit mode
            true
        }
        _ => false,
    }
}
```

### BaseEdit (hybrid, deliberately partial)

Selection-handle dragging needs sub-slop precision and stage-adjacent
latency, so `Down` arms it and a `min_travel: 0.` drag recognizer drives it;
long-press and tap move to the session, fixing the still-finger latent bug
(move-triggered long-press today):

```rust
fn gesture_set(&self) -> GestureSet {
    GestureSet::EDIT
}

async fn handle_gesture(&self, gesture: GestureAction) -> bool {
    match gesture {
        GestureAction::Down { pos } => {
            // try_handle_drag(): grab a selection handle by radius,
            // or arm word-select/cursor state
            true
        }
        GestureAction::DragMove { curr, .. } => {
            // Handle drag with select+autoscroll, or ScrollVert, or
            // SetCursorPos per the armed mode
            true
        }
        GestureAction::LongPress { pos } => {
            // start_touch_select(): word select + action menu
            true
        }
        GestureAction::Tap { pos } => {
            // touch_set_cursor_pos() + focus_request
            true
        }
        GestureAction::Up { pos } => {
            // handle_touch_end(): cursor set, stop autoscroll, focus
            true
        }
        _ => false,
    }
}
```

## Risks / Trade-offs

- [Async-only delivery regresses selection-handle drag latency] → All visual
  feedback already waits on the serialized redraw pass; the sync path only
  saves one executor hop. Verify on device during the BaseEdit migration
  step; if measurable, add a `handle_gesture_sync` delivery variant for
  `Down`/`DragMove` consumers as a contained fallback (D7 does not preclude
  it).
- [Slop-bounded taps feel different in ChatView/Menu (0.05px → 10px)] →
  Accepted delta toward platform-standard feel; the mechanism for per-node
  tightening exists (`TapCfg`) if field use disagrees.
- [Mixed raw/gesture widgets during migration can double-act] → Migration is
  per-widget and the raw path stays intact until D8; a migrated widget stops
  claiming raw phases, and ownership stickiness (D4) prevents siblings from
  seeing the strays. Each step ships green.
- [Touch ownership stickiness changes edge behavior] → Called-out delta; a
  touch that starts on widget A and ends over widget B now belongs to A
  entirely. Matches iOS; more predictable than per-phase re-propagation.
- [Sequencing collision with `app-chatview`] → ChatView migration here is the
  proof of composition; if chatview2 lands first, this change skips task 8
  and chatview2 consumes the session directly. Decide at task-8 time.
- [Recognizer regressions] → Recognizers are pure state machines; synthetic
  touch-stream unit tests pin tap/drag/long-press/velocity behavior before
  any widget migrates.

## Migration Plan

1. Delete the dead code (both old gesture files, see D8), then land the gesture
   module (constants, `GestureAction`, recognizers + unit tests), session
   (Stage feed, targeting, timers, throttling, arbitration), `UIObject`
   additions and the `handle_gesture` re-point, Layer forwarding. Raw path
   untouched; nothing behavior-visible yet.
2. Migrate leaf widgets: Button, TokenTable, EmojiPicker. Desktop verify via
   `make compile-dev` + `EMULATE_TOUCH`.
3. Migrate Menu.
4. Migrate ChatView (or hand to chatview2 — see Risks).
5. Migrate BaseEdit; on-device latency check (Risks).
6. Deletions (D8) + `make compile-apk` + on-device feel pass over: chat
   scrolling/flick/grab-stop, URL tap, line select, edit word-select/handles,
   menu edit-mode/reorder, emoji scroll, button taps.

Rollback: every step is an independent commit reverting to the previous
shippable state; step 1 is inert by construction.

## Open Questions

- EmojiPicker flick inertia: adopt for feel-consistency or preserve its
  current dead-stop? Decide during its migration task; input (`DragEnd
  { vel }`) is available either way.
- Menu reorder: drive updates from `DragMove` with an armed flag (as sketched)
  or from a `min_travel: 0.` drag recognizer like BaseEdit's handles?
  Task-level decision, both supported.
- Pinch: resurrect the dead node's behavior as a `Pinch` recognizer when a
  consumer appears (image zoom is the plausible one). Deferred by design.
