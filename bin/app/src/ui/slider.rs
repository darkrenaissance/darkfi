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

use async_trait::async_trait;
use darkfi_serial::serialize;
use miniquad::MouseButton;
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::instrument;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer, Vertex},
    mesh::MeshBuilder,
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyRect,
        PropertyShape, PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
    ExecutorPtr,
};

use super::{
    gesture::{GestureAction, GestureSet},
    DrawUpdate, OnModify, RedrawTrigger, UIObject, VectorShape,
};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::slider", $($arg)*); } }

pub type SliderPtr = Arc<Slider>;

pub struct Slider {
    node: SceneNodeWeak,
    renderer: Renderer,
    redraw: RedrawTrigger,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    dc_key: u64,

    rect: PropertyRect,
    value: PropertyFloat32,
    min: PropertyFloat32,
    max: PropertyFloat32,
    step: PropertyFloat32,
    is_active: PropertyBool,
    thickness: PropertyFloat32,
    thumb_radius: PropertyFloat32,
    track_color: PropertyColor,
    fill_color: PropertyColor,
    thumb_color: PropertyColor,
    thumb_shape: PropertyShape,
    show_stepper: PropertyBool,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    debug: PropertyBool,

    /// Interactions write `value`/fire `changing` only while held
    pointer_held: AtomicBool,
    /// Accept a touch start: swallow the matching Up even without a scrub
    swallow_touch: AtomicBool,
}

/// Track geometry for one draw/input pass (rect-local space; input
/// positions must be rebased by `rect.x` first)
struct Geometry {
    rect: Rectangle,
    min: f32,
    max: f32,
    inset: f32,
    thickness: f32,
    btn_w: f32,
    track_span: f32,
    cy: f32,
    track_top: f32,
}

impl Slider {
    pub async fn new(node: SceneNodeWeak, renderer: Renderer, redraw: RedrawTrigger) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let value = PropertyFloat32::wrap(node_ref, Role::Internal, "value", 0).unwrap();
        let min = PropertyFloat32::wrap(node_ref, Role::Internal, "min", 0).unwrap();
        let max = PropertyFloat32::wrap(node_ref, Role::Internal, "max", 0).unwrap();
        let step = PropertyFloat32::wrap(node_ref, Role::Internal, "step", 0).unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let thickness = PropertyFloat32::wrap(node_ref, Role::Internal, "thickness", 0).unwrap();
        let thumb_radius =
            PropertyFloat32::wrap(node_ref, Role::Internal, "thumb_radius", 0).unwrap();
        let track_color = PropertyColor::wrap(node_ref, Role::Internal, "track_color").unwrap();
        let fill_color = PropertyColor::wrap(node_ref, Role::Internal, "fill_color").unwrap();
        let thumb_color = PropertyColor::wrap(node_ref, Role::Internal, "thumb_color").unwrap();
        let thumb_shape = PropertyShape::wrap(node_ref, Role::Internal, "thumb_shape", 0).unwrap();
        let show_stepper = PropertyBool::wrap(node_ref, Role::Internal, "show_stepper", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            renderer,
            redraw,
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),

            rect,
            value,
            min,
            max,
            step,
            is_active,
            thickness,
            thumb_radius,
            track_color,
            fill_color,
            thumb_color,
            thumb_shape,
            show_stepper,
            z_index,
            priority,
            debug,

            pointer_held: AtomicBool::new(false),
            swallow_touch: AtomicBool::new(false),
        });

        Pimpl::Slider(self_)
    }

    /// Returns `None` for degenerate track geometry
    fn geometry(&self) -> Option<Geometry> {
        let rect = self.rect.get();
        let min = self.min.get();
        let max = self.max.get();
        if max <= min || !min.is_finite() || !max.is_finite() {
            return None
        }

        let inset = self.thumb_radius.get().max(0.);
        let btn_w = if self.show_stepper.get() { (2. * inset).max(30.) } else { 0. };
        let track_span = rect.w - 2. * btn_w;
        if track_span - 2. * inset <= 0. {
            return None
        }

        let thickness = self.thickness.get().max(0.);
        let cy = rect.h / 2.;
        Some(Geometry {
            rect,
            min,
            max,
            inset,
            thickness,
            btn_w,
            track_span,
            cy,
            track_top: cy - thickness / 2.,
        })
    }

    /// Normalized position of `val` in `[0, 1]`
    fn value_t(geo: &Geometry, val: f32) -> f32 {
        ((val - geo.min) / (geo.max - geo.min)).clamp(0., 1.)
    }

    /// Local x of the thumb center for normalized position `t`
    fn t_x(geo: &Geometry, t: f32) -> f32 {
        geo.btn_w + geo.inset + t * (geo.track_span - 2. * geo.inset)
    }

    /// Stepper amount: `step`, else range/10
    fn stepper_step(&self, geo: &Geometry) -> f32 {
        let step = self.step.get();
        if step > 0. && step.is_finite() {
            return step
        }
        (geo.max - geo.min) / 10.
    }

    /// Snap `val` onto the `step` grid, clamped
    fn snap(&self, geo: &Geometry, val: f32) -> f32 {
        let step = self.step.get();
        if !(step > 0. && step.is_finite()) {
            return val.clamp(geo.min, geo.max)
        }
        let val = geo.min + ((val - geo.min) / step).round() * step;
        val.clamp(geo.min, geo.max)
    }

    /// Map a parent-space x to a value (no dead zones); `None` if degenerate
    fn map_pos_to_value(&self, geo: &Geometry, x: f32) -> Option<f32> {
        let track_local = x - geo.rect.x - geo.btn_w;
        let t = (track_local / geo.track_span).clamp(0., 1.);
        if !t.is_finite() {
            return None
        }

        Some(self.snap(geo, geo.min + t * (geo.max - geo.min)))
    }

    /// Step the value once in `dir` (-1/1) and commit immediately
    async fn stepper_press(&self, geo: &Geometry, dir: f32) {
        let cur = self.value.get();
        let val = self.snap(geo, cur + self.stepper_step(geo) * dir);
        if val == cur {
            return
        }

        d!("stepper press: {cur} -> {val}");
        self.set_value_internal(val, "Slider::stepper_press").await;
        self.fire_changed(val).await;
    }

    /// Write `value` internally (echo-suppressed), then fire `changing`
    async fn set_value_internal(&self, val: f32, tag: &'static str) {
        let atom = &mut self.redraw.make_guard(gfxtag!(tag));
        self.value.set(atom, val);

        let node = self.node.upgrade().unwrap();
        if let Err(e) = node.trigger("changing", serialize(&val)).await {
            warn!(target: "ui::slider", "changing trigger failed: {e}");
        }
    }

    /// Fire the commit signal (track release or stepper press)
    async fn fire_changed(&self, val: f32) {
        let node = self.node.upgrade().unwrap();
        if let Err(e) = node.trigger("changed", serialize(&val)).await {
            warn!(target: "ui::slider", "changed trigger failed: {e}");
        }
    }

    /// Pointer-down at `pos`; returns whether the event is swallowed
    async fn pointer_down(&self, pos: Point) -> bool {
        if !self.is_active.get() || !self.rect.get().contains(pos) {
            return false
        }

        let Some(geo) = self.geometry() else { return false };
        self.swallow_touch.store(true, Ordering::Relaxed);

        if geo.btn_w > 0. {
            let local_x = pos.x - geo.rect.x;
            if local_x <= geo.btn_w {
                self.stepper_press(&geo, -1.).await;
                return true
            }
            if local_x >= geo.rect.w - geo.btn_w {
                self.stepper_press(&geo, 1.).await;
                return true
            }
        }

        let Some(val) = self.map_pos_to_value(&geo, pos.x) else { return true };

        d!("Down: jump to {val}");
        self.pointer_held.store(true, Ordering::Relaxed);
        self.set_value_internal(val, "Slider::pointer_down").await;
        true
    }

    /// Pointer-move at `pos` while scrubbing
    async fn pointer_move(&self, pos: Point) -> bool {
        if !self.pointer_held.load(Ordering::Relaxed) {
            return false
        }

        let Some(geo) = self.geometry() else { return true };
        let Some(val) = self.map_pos_to_value(&geo, pos.x) else { return true };

        d!("DragMove: {val}");
        self.set_value_internal(val, "Slider::pointer_move").await;
        true
    }

    /// Pointer-up; commits once if a scrub was in progress
    async fn pointer_up(&self) -> bool {
        if !self.swallow_touch.swap(false, Ordering::Relaxed) {
            return false
        }

        if self.pointer_held.swap(false, Ordering::Relaxed) {
            let val = self.value.get();
            d!("Up: commit {val}");
            self.fire_changed(val).await;
        }
        true
    }

    /// Append an origin-centered `shape` at `(x, y)`, scaled by `scale`
    fn append_shape(
        mesh: &mut MeshBuilder,
        shape: &VectorShape,
        x: f32,
        y: f32,
        scale: f32,
        rect: &Rectangle,
    ) {
        if shape.verts.is_empty() {
            return
        }
        let Ok(mut verts) = shape.eval(rect.w, rect.h) else {
            warn!(target: "ui::slider", "shape eval failure");
            return
        };
        for vert in &mut verts {
            vert.pos[0] = vert.pos[0] * scale + x;
            vert.pos[1] = vert.pos[1] * scale + y;
        }
        mesh.append(verts, shape.indices.clone());
    }

    fn build_mesh(&self) -> Option<MeshBuilder> {
        let geo = self.geometry()?;
        let rect = geo.rect;

        let track_color = self.track_color.get();
        let fill_color = self.fill_color.get();
        let thumb_color = self.thumb_color.get();

        let mut mesh = MeshBuilder::new(gfxtag!("slider"));

        mesh.draw_filled_box(
            &Rectangle::new(geo.btn_w, geo.track_top, geo.track_span, geo.thickness),
            track_color,
        );

        let t = Self::value_t(&geo, self.value.get());
        let thumb_x = Self::t_x(&geo, t);

        let fill_w = thumb_x - geo.btn_w;
        if fill_w > 0. {
            mesh.draw_filled_box(
                &Rectangle::new(geo.btn_w, geo.track_top, fill_w, geo.thickness),
                fill_color,
            );
        }

        // Thumb: schema shape scaled to thumb_radius
        let thumb_shape = self.thumb_shape.get();
        Self::append_shape(&mut mesh, &thumb_shape, thumb_x, geo.cy, geo.inset, &rect);

        if geo.btn_w > 0. {
            let at_min = self.value.get() <= geo.min + f32::EPSILON;
            let at_max = self.value.get() >= geo.max - f32::EPSILON;
            let mut glyph = thumb_color;
            let bar_h = 2.5f32;
            let bar_w = geo.btn_w * 0.4;

            // Minus
            glyph[3] = if at_min { 0.35 } else { 1. };
            mesh.draw_filled_box(
                &Rectangle::new(geo.btn_w / 2. - bar_w / 2., geo.cy - bar_h / 2., bar_w, bar_h),
                glyph,
            );

            // Plus
            glyph[3] = if at_max { 0.35 } else { 1. };
            let cx = rect.w - geo.btn_w / 2.;
            mesh.draw_filled_box(
                &Rectangle::new(cx - bar_w / 2., geo.cy - bar_h / 2., bar_w, bar_h),
                glyph,
            );
            mesh.draw_filled_box(
                &Rectangle::new(cx - bar_h / 2., geo.cy - bar_w / 2., bar_h, bar_w),
                glyph,
            );
        }

        debug_assert!(
            mesh.verts.len() <= u16::MAX as usize && mesh.indices.len() <= u16::MAX as usize
        );

        Some(mesh)
    }
}

#[async_trait]
impl UIObject for Slider {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        // External writes repaint; internal ones echo-suppress
        on_modify.when_change_external(self.value.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.min.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.max.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.step.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.thickness.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.thumb_radius.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.track_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.fill_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.thumb_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.thumb_shape.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.show_stepper.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.z_index.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.debug.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
    }

    #[instrument(target = "ui::slider")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        if let Err(e) = self.rect.eval(atom, &parent_rect) {
            warn!(target: "ui::slider", "Rect eval failure: {e}");
        }

        let Some(mesh) = self.build_mesh() else {
            if self.debug.get() {
                let rect = self.rect.get().with_zero_pos();
                let mut mesh = MeshBuilder::new(gfxtag!("slider_debug"));
                mesh.draw_outline(&rect, [1., 0., 0., 1.], 1.);
                return Some(DrawUpdate {
                    key: self.dc_key,
                    draw_calls: vec![(
                        self.dc_key,
                        DrawCall::new(
                            vec![DrawInstruction::Draw(
                                mesh.alloc(&self.renderer).draw_untextured(),
                            )],
                            vec![],
                            self.z_index.get(),
                            "slider_debug",
                        ),
                    )],
                });
            }
            return None
        };

        let mut instrs = vec![DrawInstruction::Move(self.rect.get().pos())];
        instrs.push(DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured()));

        if self.debug.get() {
            let rect = self.rect.get().with_zero_pos();
            let mut dbg = MeshBuilder::new(gfxtag!("slider_debug"));
            dbg.draw_outline(&rect, [1., 0., 0., 1.], 1.);
            instrs.push(DrawInstruction::Draw(dbg.alloc(&self.renderer).draw_untextured()));
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "slider"),
            )],
        })
    }

    fn gesture_set(&self) -> GestureSet {
        GestureSet::SLIDER
    }

    fn gesture_hit_test(&self, pos: Point) -> bool {
        self.is_active.get() && self.rect.get().contains(pos)
    }

    async fn handle_gesture(&self, gesture: GestureAction) -> bool {
        match gesture {
            GestureAction::Down { pos } => self.pointer_down(pos).await,

            GestureAction::DragMove { curr, .. } => self.pointer_move(curr).await,

            GestureAction::Up { .. } => self.pointer_up().await,

            _ => false,
        }
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }
        self.pointer_down(mouse_pos).await
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        self.pointer_move(mouse_pos).await
    }

    async fn handle_mouse_btn_up(&self, _btn: MouseButton, _mouse_pos: Point) -> bool {
        self.pointer_up().await
    }
}

impl Drop for Slider {
    fn drop(&mut self) {
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for Slider {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
