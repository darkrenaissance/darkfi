## 1. Gesture core

- [ ] 1.1 Delete the dead code up front: `ui/gesture.rs` (pinch node, with its
       `create_gesture` registration and `Pimpl::Gesture` variant — the new
       module takes over its path) and `win/gesture.rs` (`GestureProcessor`
       and the old `GestureAction`, with the dead `gesture_proc` dispatch in
       `Window::handle_touch`); then create `bin/app/src/ui/gesture/` module
       with `GestureConstants` (slop 10px, tap 300ms, sys long-press timeout,
       20ms move period, 40ms sample window), the `GestureAction` stream
       (`Down`/`Up`/`Tap`/`LongPress`/`DragStart`/`DragMove`/`DragEnd { vel }`),
       and the `GestureSet`/config types (`Axes`, `Direction`, `min_travel`);
       repoint the `ui::GestureAction` re-export and the dead
       `UIObject::handle_gesture` signature to the new types; verify
       `make compile-dev` in `bin/app`
- [ ] 1.2 Implement the recognizer library as pure state machines (tap,
       long-press, drag lifecycle with 40ms velocity sampling) taking
       `GestureConstants`; verify unit tests pass covering: tap within/beyond
       slop, tap duration bound, long-press firing during hold, long-press
       cancelled by movement, throttled delivery vs full sampling, and
       `DragEnd` velocity from the sample window
- [ ] 1.3 Implement gesture arbitration rules (first-resolved-wins, tap vs
       drag via slop, cascade cancellation); verify unit tests cover
       descendant-tap-wins-within-slop and ancestor-drag-wins-beyond-slop

## 2. Session and dispatch

- [ ] 2.1 Implement `GestureSession`: fed from the Stage thread at the `gfx`
       touch entry before any sync claiming; resolves the hit-test target
       chain at `Down` (priority order, sticky ownership until `Up`/cancel);
       runs version-guarded long-press timers on the executor; throttles
       `DragMove` delivery to 20ms; ignores secondary touch ids; verify unit
       tests pass for sticky ownership, priority ordering, and cancellation
       teardown
- [ ] 2.2 Add `gesture_set()`/`gesture_hit_test()` defaults to `UIObject` and
       implement `Layer`/`ScrollLayer` gesture forwarding with
       coordinate translation (mirroring `handle_touch` translation); verify
       `make compile-dev` and a unit test pinning local-coordinate delivery
       through a nested layer
- [ ] 2.3 Route `EMULATE_TOUCH` mouse-emulated touches through the session so
       desktop emulation produces real gestures; verify an emulated tap
       triggers the gesture path on a desktop build with `emulate-android`
       enabled

## 3. Widget migrations

- [ ] 3.1 Migrate Button: `Tap` fires the click signal; delete
       `handle_touch`/`handle_touch_sync` and the `mouse_btn_held` gate; mouse
       path unchanged; verify emulated tap clicks on desktop with no double
       activation
- [ ] 3.2 Migrate TokenTable: row click on `Tap`; delete the touch-to-mouse
       simulation; verify emulated row tap fires `row_click`
- [ ] 3.3 Migrate EmojiPicker: 1:1 clamped scroll on vertical drag, emoji
       activation on `Tap`; resolve the flick-inertia open question (adopt or
       preserve dead-stop) and record the choice; verify scroll clamps at
       bounds and tap selects under emulation
- [ ] 3.4 Migrate Menu: `LongPress` enters edit mode (single fire during
       hold), `Tap` selects/deletes, reorder drag armed at `Down` on the
       reorder handle; delete `TouchInfo`/`DragInfo` and the long-press task
       juggling; verify edit mode, reorder commit, and item selection under
       emulation
- [ ] 3.5 Migrate ChatView: 1:1 scroll on drag, inertia fed from `DragEnd`
       velocity, grab-stops-inertia on `DragStart`, `LongPress` for line
       select / URL toast, `Tap` forwarding to message content (URLs, file
       downloads) and line-toggle; delete `TouchInfo`, the
       `touch_hold_version` timer, and `end_touch_phase`; first coordinate
       with `app-chatview` whether this task or chatview2 performs the
       migration; verify scroll, flick, grab-stop, and URL tap under
       emulation
- [ ] 3.6 Migrate BaseEdit (hybrid): `Down` arms selection-handle grab and
       word-select/cursor state, `min_travel: 0.` drag drives handle movement
       and vertical scroll, `LongPress` selects word + shows action menu,
       `Tap` sets cursor + requests focus, `Up` finalizes; delete
       `TouchStateAction` and the sync/async phase split; verify word-select,
       handle drag, cursor tap, and focus under emulation

## 4. Cleanup and verification

- [ ] 4.1 Delete the old paths: `handle_touch`/`handle_touch_sync` from
       `UIObject` and all implementors, and the four widget `TouchInfo`
       state machines; verify `make compile-dev` with no remaining
       references to the removed APIs or the old `GestureAction`
- [ ] 4.2 Verify `make compile-apk` succeeds
- [ ] 4.3 On-device feel pass: chat scroll/flick/grab-stop, URL tap, line
       selection, edit word-select and selection handles (latency check —
       if handle drag feels laggy, implement the documented
       `handle_gesture_sync` fallback for `Down`/`DragMove`), menu
       edit-mode/reorder, emoji scroll, button taps
- [ ] 4.4 Cross-check every scenario in `specs/gesture/spec.md` against unit
       tests and the on-device pass; record any scenario lacking coverage
