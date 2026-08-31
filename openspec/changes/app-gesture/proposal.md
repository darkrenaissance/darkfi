## Why

The app UI has five overlapping hand-rolled gesture recognizers (`GestureProcessor`,
`ChatView::TouchInfo`, `Menu::TouchInfo`+`DragInfo`, `BaseEdit::TouchStateAction`,
`EmojiPicker::TouchInfo`) plus a dead pinch node, each re-deriving movement, dwell
time, and velocity with wildly divergent thresholds (tap strictness ranges from
0.05px to 10px across widgets). Touch dispatch is split-brained: a sync Stage-thread
path that can swallow events before the async path — and before the window gesture
processor — ever sees them, no hit-testing, and `handle_gesture` cannot propagate
past the first `Layer`. Widget touch code is the largest block of incidental
complexity in `bin/app/src/ui/`.

## What Changes

- New gesture subsystem in `bin/app/src/ui/gesture/`: a window-level
  `GestureSession` owning the touch stream, target resolution, long-press timers,
  version-guarded cancellation, move throttling, and recognizer arbitration; plus a
  recognizer library (tap, long-press, drag lifecycle, flick) that exists exactly
  once.
- Unified gesture constants (touch slop, tap duration, long-press timeout from the
  system `long_press_timeout()`, flick velocity, move delivery period) replacing the
  per-widget scatter. Per-widget config survives only where semantic: axis lock,
  drag direction, `min_travel: 0.` for precision drags.
- `UIObject` gains `gesture_set()` and `gesture_hit_test()`; the existing
  (dead-dispatch) `handle_gesture()` is re-pointed to the new `GestureAction`;
  `Layer`/`ScrollLayer` forward gestures with coordinate translation.
- New `GestureAction` stream: `Down`/`Up` passthrough for immediate feedback,
  `Tap`, `LongPress`, `DragStart`/`DragMove`/`DragEnd { vel }` (flick is derived
  from `DragEnd` velocity by the consumer).
- The session is fed from the Stage thread (`gfx` touch entry) so it observes all
  touches regardless of sync claiming; `EMULATE_TOUCH` mouse emulation routes
  through the same session so desktop development produces gestures.
- All touch widgets migrate to the new model: Button, TokenTable, EmojiPicker,
  Menu, ChatView, BaseEdit (hybrid: `Down`-armed precision drags for selection
  handles). Recognition semantics live in recognizers; controller physics (scroll
  inertia, grab-to-stop) stay widget-side.
- **BREAKING** (internal app API): `handle_touch` and `handle_touch_sync` are
  removed from `UIObject` once every widget is migrated; the four widget
  `TouchInfo` state machines and `win/gesture.rs` (`GestureProcessor` and the
  old `GestureAction`) are deleted at the end; the dead `ui/gesture.rs` pinch
  node is removed up front — the new `ui/gesture/` directory takes its module
  path (pinch returns later as a recognizer if wanted).
- Accepted behavior deltas toward platform-standard feel: slop-bounded taps
  everywhere (replaces 0.05px strictness in ChatView/Menu), slop dead-zone before
  scroll starts, long-press fires once during hold by timer, touch ownership
  sticks to the Started target, EmojiPicker may gain flick inertia, all gesture
  delivery is async (the sync path dies; on-device latency to be verified during
  migration with a sync hatch as fallback).

## Capabilities

### New Capabilities
- `gesture`: the gesture recognition and delivery system for the app UI — session
  semantics (stream ownership, targeting, timers, throttling, arbitration),
  recognizer contracts and unified constants, the `GestureAction` event stream,
  the `UIObject` gesture contract
  (`gesture_set`/`gesture_hit_test`/`handle_gesture`),
  coordinate-translation forwarding, and the migrated behavior of each widget
  under the new system (button taps, scrollers, chat selection/scroll/flick,
  edit hybrid handling, menu reorder/edit-mode) including the accepted behavior
  deltas above.

### Modified Capabilities

(none — no main specs exist yet; widget behavior deltas are captured as
requirements of the new `gesture` capability)

## Impact

- Code: `bin/app/src/ui/` (new `gesture/` module; rewrites of `win/mod.rs` touch
  entry, `layer.rs`, `scroll_layer.rs`, `button.rs`, `tokentable/`,
  `emoji_picker/`, `menu/`, `chatview/`, `edit/`; deletion of the dead
  `ui/gesture.rs` and `win/gesture.rs` up front), and `bin/app/src/gfx/mod.rs`
  (Stage-thread session feed).
  No workspace crates, no dependencies, no consensus/ZK surfaces.
- Parallel work: `app-chatview` (chatview2) is specified to consume drag/flick as
  scroll-controller inputs — this change should land its session and recognizers
  first so chatview2 builds on it; ChatView's own migration is the proof of
  composition (or is absorbed by chatview2 if it lands first).
- Verification: `make compile-dev` (desktop), `make compile-apk` (Android), and
  on-device touch-feel verification for the async-only delivery (edit selection
  handles are the latency-sensitive case).
