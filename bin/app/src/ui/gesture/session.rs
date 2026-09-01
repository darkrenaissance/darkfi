/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! The window-level gesture session.
//!
//! The session is fed from the miniquad Stage thread at the `gfx`
//! touch entry, before any sync claiming, so recognition observes
//! every touch. At touch start it resolves the sticky target chain by
//! hit-testing the widget tree in priority order; all events for that
//! touch go to that chain until the touch ends or is cancelled. The
//! recognizer math runs inline on the Stage thread (pure, cheap);
//! long-press timers are version-guarded executor tasks; delivery is
//! async through a single-consumer channel so events arrive in
//! recognition order.

use miniquad::TouchPhase;
use parking_lot::Mutex as SyncMutex;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    gfx::{Point, Vector},
    scene::{SceneNodePtr, SceneNodeWeak},
    ExecutorPtr,
};

use super::recognizer::Recognition;
use crate::ui::{get_children_ordered, get_ui_object_ptr, UIObject};

use super::{Arena, GestureAction, GestureConstants, GestureSet, MoveThrottle, VelocityTracker};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::gesture::session", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "ui::gesture::session", $($arg)*); } }

/// One link of a resolved gesture target chain.
#[derive(Clone)]
pub struct GestureTarget {
    /// The widget
    pub obj: Arc<dyn UIObject + Send>,
    /// Translation from window space into the widget's parent space,
    /// accumulated from the layers above it.
    pub offset: Point,
}

/// Extend the gesture chain under `pos` (given in the children's
/// parent space) from `children`, which must be in priority order
/// (highest priority first): the first child that passes hit-testing
/// owns the touch and the chain descends into it.
pub(crate) fn scan_children(
    children: &[Arc<dyn UIObject + Send>],
    pos: Point,
    offset: Point,
    chain: &mut Vec<GestureTarget>,
) {
    for child in children {
        if child.gesture_hit_test(pos) {
            chain.push(GestureTarget { obj: child.clone(), offset });
            child.gesture_descend(pos, offset, chain);
            break
        }
    }
}

/// A resolved gesture event awaiting delivery.
struct Delivery {
    /// Delivery target
    target: GestureTarget,
    /// The action, in window coordinates
    action: GestureAction,
}

impl GestureTarget {
    fn delivery(&self, action: GestureAction) -> Delivery {
        Delivery { target: self.clone(), action }
    }

    /// Deliver `action` translated into this target's space.
    async fn dispatch(&self, mut action: GestureAction) -> bool {
        let off = self.offset;
        action.translate(Vector { x: -off.x, y: -off.y });
        self.obj.handle_gesture(action).await
    }
}

/// Recognition state for the active (primary) touch.
struct ActiveTouch {
    /// Touch id that owns recognition
    id: u64,
    /// Sticky target chain resolved at touch start, root first
    chain: Vec<GestureTarget>,
    /// Recognizers observing this touch
    arena: Arena,
    /// Release velocity samples; observes every move
    vel: VelocityTracker,
    /// DragMove delivery gate
    throttle: MoveThrottle,
    /// Touch start, window space
    start: Point,
    /// Last position a DragMove was delivered for, window space
    prev_move: Point,
    /// Latest observed position, window space
    curr: Point,
    start_instant: Instant,
}

impl ActiveTouch {
    fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.start_instant)
    }

    /// The deepest chain entry: the hit-tested target receiving the
    /// `Down`/`Up` passthrough.
    fn deepest(&self) -> &GestureTarget {
        self.chain.last().unwrap()
    }

    fn action_for(&self, rec: Recognition, pos: Point) -> GestureAction {
        match rec {
            Recognition::Tap => GestureAction::Tap { pos },
            Recognition::LongPress => GestureAction::LongPress { pos },
            Recognition::DragStart => GestureAction::DragStart { start: self.start },
            Recognition::DragMove => {
                GestureAction::DragMove { start: self.start, prev: self.prev_move, curr: pos }
            }
            Recognition::DragEnd { vel } => {
                GestureAction::DragEnd { start: self.start, curr: pos, vel }
            }
        }
    }

    fn moved(&mut self, pos: Point, now: Instant) -> Vec<Delivery> {
        let elapsed = self.elapsed(now);
        self.vel.push(elapsed, pos);
        self.curr = pos;

        let recs = self.arena.on_move(elapsed, pos - self.start);
        let mut out = vec![];

        for (i, rec) in recs {
            match rec {
                Recognition::DragStart => {
                    let action = self.action_for(rec, pos);
                    out.push(self.chain[i].delivery(action));
                }
                Recognition::DragMove => {
                    // Delivery is throttled to the move period; the
                    // velocity tracker above still saw this sample.
                    if self.throttle.should_deliver(elapsed) {
                        let action = self.action_for(rec, pos);
                        self.prev_move = pos;
                        out.push(self.chain[i].delivery(action));
                    }
                }
                _ => {}
            }
        }

        out
    }

    fn ended(&mut self, pos: Point, now: Instant) -> Vec<Delivery> {
        let elapsed = self.elapsed(now);
        self.vel.push(elapsed, pos);
        self.curr = pos;

        let vel = self.vel.velocity();
        let recs = self.arena.on_up(elapsed, vel);
        let mut out = vec![];

        for (i, rec) in recs {
            let action = self.action_for(rec, pos);
            out.push(self.chain[i].delivery(action));
        }

        out.push(self.deepest().delivery(GestureAction::Up { pos }));
        out
    }
}

/// Inner state guarded by the session lock.
struct Inner {
    /// Bumped whenever the active touch changes; long-press timers
    /// validate against it so a stale timer cannot fire.
    gen: u64,
    /// The active touch, if any. Secondary touch ids are ignored.
    active: Option<ActiveTouch>,
}

/// The window-owned gesture session: target resolution, recognition,
/// timers, throttling, arbitration, and async delivery.
///
/// Lock ordering invariant: `inner` is the innermost lock. It is taken
/// on the Stage thread and, while held, the tree walk takes scene and
/// widget locks below it — so nothing may ever take `inner` from
/// inside `handle_gesture` (or with widget locks held); widgets have
/// no session handle today and must stay that way.
pub struct GestureSession {
    /// The window node; chain resolution starts at its children
    node: SceneNodeWeak,
    /// Executor for the long-press timers and the delivery consumer
    ex: ExecutorPtr,
    /// Recognition thresholds
    consts: GestureConstants,
    inner: SyncMutex<Inner>,
    delivery_tx: async_channel::Sender<Delivery>,
}

pub type GestureSessionPtr = Arc<GestureSession>;

impl GestureSession {
    pub fn new(node: SceneNodeWeak, ex: ExecutorPtr) -> GestureSessionPtr {
        let (delivery_tx, delivery_rx) = async_channel::unbounded::<Delivery>();

        // Single consumer so deliveries arrive in recognition order. A
        // panicking widget handler must not kill the input stream, so
        // each dispatch is panic-isolated (the panic is reported by
        // the default hook; delivery for that event is simply lost).
        ex.spawn(async move {
            loop {
                let Ok(delivery) = delivery_rx.recv().await else {
                    t!("Gesture delivery channel closed");
                    break
                };

                let dispatched =
                    futures::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(async {
                        delivery.target.dispatch(delivery.action).await
                    }))
                    .await;

                match dispatched {
                    Ok(true) => {}
                    Ok(false) => {
                        t!("Gesture not claimed by target");
                    }
                    Err(panic) => {
                        let msg = panic
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| panic.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "unknown panic".to_string());
                        e!("Gesture handler panicked: {msg}");
                    }
                }
            }
        })
        .detach();

        Arc::new(Self {
            node,
            ex,
            consts: GestureConstants::platform(),
            inner: SyncMutex::new(Inner { gen: 0, active: None }),
            delivery_tx,
        })
    }

    /// Stage-thread touch feed. Runs recognition inline and queues
    /// gesture delivery onto the executor.
    pub fn touch_event(self: &Arc<Self>, phase: TouchPhase, id: u64, pos: Point) {
        let now = Instant::now();
        let (deliveries, timer_gen) = {
            let mut inner = self.inner.lock();
            match phase {
                TouchPhase::Started => {
                    // Secondary touches never disturb the primary.
                    if inner.active.is_some() {
                        (vec![], None)
                    } else {
                        let chain = self.resolve_chain(pos);
                        if chain.is_empty() {
                            (vec![], None)
                        } else {
                            let sets: Vec<GestureSet> =
                                chain.iter().map(|target| target.obj.gesture_set()).collect();
                            let arena = Arena::new(self.consts, &sets);
                            let timer_gen = arena.wants_long_press().then_some(inner.gen + 1);
                            inner.gen += 1;

                            let down = chain.last().unwrap().delivery(GestureAction::Down { pos });

                            inner.active = Some(ActiveTouch {
                                id,
                                chain,
                                arena,
                                vel: VelocityTracker::new(self.consts.sample_window_ms),
                                throttle: MoveThrottle::new(self.consts.move_delivery_period),
                                start: pos,
                                prev_move: pos,
                                curr: pos,
                                start_instant: now,
                            });

                            (vec![down], timer_gen)
                        }
                    }
                }
                TouchPhase::Moved => match &mut inner.active {
                    Some(active) if active.id == id => (active.moved(pos, now), None),
                    _ => (vec![], None),
                },
                TouchPhase::Ended => match inner.active.take() {
                    Some(mut active) if active.id == id => {
                        inner.gen += 1;
                        (active.ended(pos, now), None)
                    }
                    stale => {
                        inner.active = stale;
                        (vec![], None)
                    }
                },
                TouchPhase::Cancelled => {
                    // Tear down all pending recognition state and
                    // timers. No recognized gesture (Tap, LongPress,
                    // DragEnd) is emitted for a cancelled touch, but
                    // the `Up` passthrough is still delivered so
                    // widgets can tear down their armed state — the
                    // old handlers treated Ended and Cancelled alike.
                    match inner.active.take() {
                        Some(mut active) if active.id == id => {
                            inner.gen += 1;
                            (
                                vec![active
                                    .deepest()
                                    .delivery(GestureAction::Up { pos: active.curr })],
                                None,
                            )
                        }
                        stale => {
                            inner.active = stale;
                            (vec![], None)
                        }
                    }
                }
            }
        };

        if let Some(gen) = timer_gen {
            let timeout = self.consts.long_press_timeout;
            let me = Arc::downgrade(self);
            self.ex
                .spawn(async move {
                    darkfi::system::msleep(timeout as u64).await;
                    let Some(session) = me.upgrade() else { return };
                    session.fire_long_press(gen);
                })
                .detach();
        }

        for d in deliveries {
            let _ = self.delivery_tx.try_send(d);
        }
    }

    /// Resolve the sticky target chain at touch start: hit-test the
    /// tree in priority order, descending through layers with
    /// coordinate translation.
    fn resolve_chain(&self, pos: Point) -> Vec<GestureTarget> {
        let Some(node) = self.node.upgrade() else { return vec![] };
        let children = ordered_objs(&node);
        let mut chain = vec![];
        scan_children(&children, pos, Point::zero(), &mut chain);
        t!("Resolved gesture chain of {} nodes", chain.len());
        chain
    }

    /// Long-press timer fire: validate the generation, let the arena
    /// decide, and queue the delivery.
    fn fire_long_press(&self, gen: u64) {
        let deliveries = {
            let mut inner = self.inner.lock();
            if inner.gen != gen {
                // The touch ended, was cancelled, or was replaced.
                return
            }

            let Some(active) = &mut inner.active else { return };
            let Some((i, rec)) = active.arena.long_press_due() else { return };
            let pos = active.curr;
            let action = active.action_for(rec, pos);
            vec![active.chain[i].delivery(action)]
        };

        for d in deliveries {
            let _ = self.delivery_tx.try_send(d);
        }
    }
}

/// Collect a node's children as UI objects in priority order.
fn ordered_objs(node: &SceneNodePtr) -> Vec<Arc<dyn UIObject + Send>> {
    get_children_ordered(node).iter().map(|child| get_ui_object_ptr(child)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;

    use crate::{
        gfx::Rectangle,
        prop::{PropertyAtomicGuard, Role},
        scene::{SceneNode, Slot},
        ui::{
            gesture::{Axes, Direction, DragCfg, GestureConstants, LongPressCfg, TapCfg},
            Button, Layer, RedrawTrigger, UIObject,
        },
        Renderer,
    };

    /// A recording gesture widget. Leaf probes hit-test their rect;
    /// layer probes forward to children with an origin translation,
    /// mirroring `Layer`.
    struct Probe {
        label: &'static str,
        set: GestureSet,
        hit: Option<Rectangle>,
        origin: Point,
        children: Vec<Arc<Probe>>,
        log: Arc<StdMutex<Vec<(&'static str, GestureAction)>>>,
    }

    impl Probe {
        fn leaf(
            label: &'static str,
            set: GestureSet,
            hit: Rectangle,
            log: Arc<StdMutex<Vec<(&'static str, GestureAction)>>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                label,
                set,
                hit: Some(hit),
                origin: Point::zero(),
                children: vec![],
                log,
            })
        }

        fn layer(
            label: &'static str,
            origin: Point,
            children: Vec<Arc<Probe>>,
            log: Arc<StdMutex<Vec<(&'static str, GestureAction)>>>,
        ) -> Arc<Self> {
            Arc::new(Self { label, set: GestureSet::NONE, hit: None, origin, children, log })
        }
    }

    #[async_trait]
    impl UIObject for Probe {
        fn priority(&self) -> u32 {
            0
        }

        fn gesture_set(&self) -> GestureSet {
            self.set
        }

        fn gesture_hit_test(&self, pos: Point) -> bool {
            if let Some(rect) = self.hit {
                return rect.contains(pos)
            }

            let local = pos - self.origin;
            self.children.iter().any(|child| child.gesture_hit_test(local))
        }

        fn gesture_descend(&self, pos: Point, offset: Point, chain: &mut Vec<GestureTarget>) {
            let local = pos - self.origin;
            let children: Vec<Arc<dyn UIObject + Send>> = self
                .children
                .iter()
                .map(|child| child.clone() as Arc<dyn UIObject + Send>)
                .collect();
            scan_children(&children, local, offset + self.origin, chain);
        }

        async fn handle_gesture(&self, gesture: GestureAction) -> bool {
            self.log.lock().unwrap().push((self.label, gesture));
            true
        }
    }

    const TEST_CONSTS: GestureConstants = GestureConstants {
        touch_slop: 10.,
        tap_max_duration: 300,
        long_press_timeout: 400,
        move_delivery_period: 20,
        sample_window_ms: 40,
    };

    fn scroller_set() -> GestureSet {
        GestureSet {
            tap: Some(TapCfg { axes: Axes::Both }),
            long_press: Some(LongPressCfg {}),
            drag: Some(DragCfg { axes: Axes::Y, direction: Direction::Any, min_travel: None }),
        }
    }

    fn make_touch(chain: Vec<GestureTarget>, pos: Point) -> ActiveTouch {
        let sets: Vec<GestureSet> = chain.iter().map(|target| target.obj.gesture_set()).collect();
        ActiveTouch {
            id: 0,
            arena: Arena::new(TEST_CONSTS, &sets),
            vel: VelocityTracker::new(TEST_CONSTS.sample_window_ms),
            throttle: MoveThrottle::new(TEST_CONSTS.move_delivery_period),
            chain,
            start: pos,
            prev_move: pos,
            curr: pos,
            start_instant: Instant::now(),
        }
    }

    fn deliver(deliveries: Vec<Delivery>) {
        for d in deliveries {
            smol::block_on(d.target.dispatch(d.action));
        }
    }

    fn labels(log: &Arc<StdMutex<Vec<(&'static str, GestureAction)>>>) -> Vec<&'static str> {
        log.lock().unwrap().iter().map(|(label, _)| *label).collect()
    }

    #[test]
    fn scan_first_passer_in_priority_order_owns() {
        let log = Arc::new(StdMutex::new(vec![]));
        let hi =
            Probe::leaf("hi", GestureSet::TAP, Rectangle::new(0., 0., 100., 100.), log.clone());
        let lo =
            Probe::leaf("lo", GestureSet::TAP, Rectangle::new(0., 0., 100., 100.), log.clone());

        let children: Vec<Arc<dyn UIObject + Send>> =
            vec![hi.clone() as Arc<dyn UIObject + Send>, lo.clone() as Arc<dyn UIObject + Send>];

        let mut chain = vec![];
        scan_children(&children, Point::new(50., 50.), Point::zero(), &mut chain);
        assert_eq!(chain.len(), 1);

        // The first child in (priority-ordered) input owns the touch
        let handled =
            smol::block_on(chain[0].dispatch(GestureAction::Tap { pos: Point::new(1., 1.) }));
        assert!(handled);
        assert_eq!(labels(&log), vec!["hi"]);
    }

    #[test]
    fn scan_descends_with_translation() {
        let log = Arc::new(StdMutex::new(vec![]));
        let leaf =
            Probe::leaf("leaf", GestureSet::TAP, Rectangle::new(0., 0., 100., 50.), log.clone());
        let layer = Probe::layer("layer", Point::new(100., 50.), vec![leaf.clone()], log.clone());

        let children: Vec<Arc<dyn UIObject + Send>> = vec![layer as Arc<dyn UIObject + Send>];

        let mut chain = vec![];
        scan_children(&children, Point::new(150., 80.), Point::zero(), &mut chain);
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[1].offset, Point::new(100., 50.));
    }

    #[test]
    fn tap_delivered_in_local_coords_to_deepest() {
        let log = Arc::new(StdMutex::new(vec![]));
        let leaf =
            Probe::leaf("leaf", GestureSet::TAP, Rectangle::new(0., 0., 100., 50.), log.clone());
        let target = GestureTarget {
            obj: leaf.clone() as Arc<dyn UIObject + Send>,
            offset: Point::new(100., 50.),
        };

        let mut touch = make_touch(vec![target], Point::new(150., 80.));
        let down = touch.deepest().delivery(GestureAction::Down { pos: Point::new(150., 80.) });
        let mut up = touch.ended(Point::new(150., 80.), Instant::now());
        let mut deliveries = vec![down];
        deliveries.append(&mut up);
        deliver(deliveries);

        // Only the deepest chain entry receives anything, and the
        // positions are translated into its space
        assert_eq!(labels(&log), vec!["leaf", "leaf", "leaf"]);
        let events = log.lock().unwrap();
        assert!(matches!(events[0].1, GestureAction::Down { pos } if pos == Point::new(50., 30.)));
        assert!(matches!(events[1].1, GestureAction::Tap { pos } if pos == Point::new(50., 30.)));
        assert!(matches!(events[2].1, GestureAction::Up { pos } if pos == Point::new(50., 30.)));
    }

    #[test]
    fn drag_lifecycle_throttle_and_velocity() {
        let log = Arc::new(StdMutex::new(vec![]));
        let leaf = Probe::leaf(
            "scroller",
            scroller_set(),
            Rectangle::new(0., 0., 100., 200.),
            log.clone(),
        );
        let target = GestureTarget { obj: leaf as Arc<dyn UIObject + Send>, offset: Point::zero() };

        let mut touch = make_touch(vec![target], Point::new(50., 100.));
        let mut all = vec![];

        // Beyond slop: DragStart
        std::thread::sleep(Duration::from_millis(2));
        all.append(&mut touch.moved(Point::new(50., 125.), Instant::now()));

        // Moves 5ms apart, 10px apart: throttled to 20ms delivery
        for i in 1..=5u32 {
            std::thread::sleep(Duration::from_millis(5));
            all.append(&mut touch.moved(Point::new(50., 125. + 10. * i as f32), Instant::now()));
        }

        // Let the sample window be dominated by the last two samples
        std::thread::sleep(Duration::from_millis(45));
        all.append(&mut touch.moved(Point::new(50., 225.), Instant::now()));
        std::thread::sleep(Duration::from_millis(25));
        all.append(&mut touch.ended(Point::new(50., 265.), Instant::now()));

        deliver(all);

        let events = log.lock().unwrap();
        let starts =
            events.iter().filter(|(_, a)| matches!(a, GestureAction::DragStart { .. })).count();
        let moves =
            events.iter().filter(|(_, a)| matches!(a, GestureAction::DragMove { .. })).count();
        let ends =
            events.iter().filter(|(_, a)| matches!(a, GestureAction::DragEnd { .. })).count();
        assert_eq!(starts, 1, "exactly one DragStart");
        assert_eq!(ends, 1, "exactly one DragEnd");
        assert_eq!(moves, 3, "6 post-start moves throttled to 20ms delivery: {moves}");

        let end_ev = events
            .iter()
            .find(|(_, a)| matches!(a, GestureAction::DragEnd { .. }))
            .expect("no DragEnd");
        let GestureAction::DragEnd { vel, curr, .. } = end_ev.1 else { unreachable!() };
        assert!(curr == Point::new(50., 265.));

        // Last samples: 25ms between (225 -> 265): ~1600 px/s
        assert!((vel.y - 1600.).abs() < 500., "vel.y = {}", vel.y);
        assert!(vel.x.abs() < 1.);
    }

    #[test]
    fn drag_move_prev_chaining() {
        let log = Arc::new(StdMutex::new(vec![]));
        let leaf = Probe::leaf(
            "scroller",
            scroller_set(),
            Rectangle::new(0., 0., 100., 200.),
            log.clone(),
        );
        let target = GestureTarget { obj: leaf as Arc<dyn UIObject + Send>, offset: Point::zero() };

        let mut touch = make_touch(vec![target], Point::new(50., 0.));
        let mut all = vec![];

        std::thread::sleep(Duration::from_millis(2));
        all.append(&mut touch.moved(Point::new(50., 30.), Instant::now()));
        std::thread::sleep(Duration::from_millis(25));
        all.append(&mut touch.moved(Point::new(50., 60.), Instant::now()));
        deliver(all);

        let events = log.lock().unwrap();
        let move_ev =
            events.iter().find(|(_, a)| matches!(a, GestureAction::DragMove { .. })).unwrap();
        let GestureAction::DragMove { start, prev, curr } = move_ev.1 else { panic!() };
        // prev is the position the previous delivered event carried;
        // for the first DragMove that is the touch start
        assert!(prev == Point::new(50., 0.));
        assert!(curr == Point::new(50., 60.));
        assert!(start == Point::new(50., 0.));
    }

    fn plant_active(session: &GestureSession, chain: Vec<GestureTarget>, pos: Point) {
        let mut inner = session.inner.lock();
        inner.gen += 1;
        inner.active = Some(make_touch(chain, pos));
    }

    /// Drive the executor long enough for queued deliveries to reach
    /// their targets, then snapshot the log.
    fn drained_log(
        ex: &ExecutorPtr,
        log: &Arc<StdMutex<Vec<(&'static str, GestureAction)>>>,
    ) -> Vec<(&'static str, GestureAction)> {
        smol::block_on(async {
            ex.run(async { smol::Timer::after(Duration::from_millis(30)).await }).await
        });
        log.lock().unwrap().clone()
    }

    #[test]
    fn secondary_touch_ids_are_inert() {
        let ex: ExecutorPtr = Arc::new(smol::Executor::new());
        let session = GestureSession::new(Arc::downgrade(&SceneNode::root()), ex.clone());

        let log = Arc::new(StdMutex::new(vec![]));
        let leaf = Probe::leaf(
            "scroller",
            scroller_set(),
            Rectangle::new(0., 0., 100., 200.),
            log.clone(),
        );
        let chain =
            vec![GestureTarget { obj: leaf as Arc<dyn UIObject + Send>, offset: Point::zero() }];

        plant_active(&session, chain, Point::new(50., 0.));

        // A second finger landing and moving must not disturb the
        // primary touch or deliver anything
        session.touch_event(TouchPhase::Started, 1, Point::new(10., 10.));
        session.touch_event(TouchPhase::Moved, 1, Point::new(10., 150.));
        let events = drained_log(&ex, &log);
        assert!(events.is_empty(), "secondary touch produced {events:?}");

        // The primary still recognizes its drag
        std::thread::sleep(Duration::from_millis(2));
        session.touch_event(TouchPhase::Moved, 0, Point::new(50., 50.));
        let events = drained_log(&ex, &log);
        assert!(events.iter().any(|(_, a)| matches!(a, GestureAction::DragStart { .. })));
    }

    #[test]
    fn cancellation_tears_down_and_invalidates_timer() {
        let ex: ExecutorPtr = Arc::new(smol::Executor::new());
        let session = GestureSession::new(Arc::downgrade(&SceneNode::root()), ex.clone());

        let log = Arc::new(StdMutex::new(vec![]));
        let leaf = Probe::leaf(
            "scroller",
            scroller_set(),
            Rectangle::new(0., 0., 100., 200.),
            log.clone(),
        );
        let chain =
            vec![GestureTarget { obj: leaf as Arc<dyn UIObject + Send>, offset: Point::zero() }];

        plant_active(&session, chain, Point::new(50., 0.));
        let gen = session.inner.lock().gen;

        session.touch_event(TouchPhase::Cancelled, 0, Point::new(50., 5.));

        // The Up passthrough is delivered so widgets can tear down
        // their armed state, but no recognized gesture fires.
        let events = drained_log(&ex, &log);
        assert_eq!(events.len(), 1, "cancel must deliver only the Up passthrough");
        assert!(matches!(events[0], ("scroller", GestureAction::Up { .. })));

        // A stale long-press timer firing after cancellation must be
        // ignored: no further events for the cancelled touch.
        session.fire_long_press(gen);
        let events = drained_log(&ex, &log);
        assert_eq!(events.len(), 1, "stale timer emitted {events:?}");
    }

    /// End-to-end: a real `Layer` wrapping a real `Button`, driven
    /// through the session. Pins chain resolution through a nested
    /// layer with coordinate translation and tap delivery.
    #[test]
    fn tap_through_real_layer_and_button() {
        smol::block_on(async {
            let ex: ExecutorPtr = Arc::new(smol::Executor::new());

            let (redraw_tx, _redraw_rx) = RedrawTrigger::new();
            let (method_tx, _method_rx) = async_channel::unbounded();
            let renderer = Renderer::new(method_tx);

            let root = SceneNode::root();

            let layer_node = crate::app::node::create_layer("layer");
            {
                let atom = &mut PropertyAtomicGuard::none();
                layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                for (i, v) in [100., 50., 400., 300.].into_iter().enumerate() {
                    layer_node
                        .get_property("rect")
                        .unwrap()
                        .set_f32(atom, Role::App, i, v)
                        .unwrap();
                }
            }
            let layer_node =
                layer_node.setup(|me| Layer::new(me, renderer.clone(), redraw_tx.clone())).await;
            root.link(layer_node.clone());

            let btn_node = crate::app::node::create_button("btn");
            {
                let atom = &mut PropertyAtomicGuard::none();
                btn_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
                for (i, v) in [0., 0., 100., 50.].into_iter().enumerate() {
                    btn_node.get_property("rect").unwrap().set_f32(atom, Role::App, i, v).unwrap();
                }
            }
            let btn_node =
                btn_node.setup(|me| Button::new(me, renderer.clone(), redraw_tx.clone())).await;
            layer_node.link(btn_node.clone());

            let (slot, click_rx) = Slot::new("test_click");
            btn_node.register("click", slot).unwrap();

            let session = GestureSession::new(Arc::downgrade(&root), ex.clone());

            // Window-space (150, 80) == layer-local (50, 30) == inside
            // the button rect
            session.touch_event(TouchPhase::Started, 0, Point::new(150., 80.));
            session.touch_event(TouchPhase::Ended, 0, Point::new(150., 80.));

            let clicked = smol::block_on(async {
                ex.run(async {
                    // Wait for the async delivery chain to fire the signal
                    click_rx.recv().await.is_ok()
                })
                .await
            });
            assert!(clicked, "emulated tap through nested layer did not click the button");

            // And the button was the resolved target: a tap outside its
            // rect (but inside the layer) must not click
            let (slot2, click_rx2) = Slot::new("test_click2");
            btn_node.register("click", slot2).unwrap();
            session.touch_event(TouchPhase::Started, 0, Point::new(350., 80.));
            session.touch_event(TouchPhase::Ended, 0, Point::new(350., 80.));

            smol::block_on(async {
                ex.run(async { smol::Timer::after(Duration::from_millis(30)).await }).await
            });
            assert!(click_rx2.try_recv().is_err(), "tap outside the button must not click");
        });
    }
}
