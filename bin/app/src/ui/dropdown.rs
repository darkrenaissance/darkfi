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
use darkfi_serial::Encodable;
use miniquad::MouseButton;
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Arc, Weak,
    },
};
use tracing::instrument;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Point, Rectangle, RenderApi, Renderer},
    mesh::MeshBuilder,
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr,
        PropertyRect, PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
    sfx, shape, text, ExecutorPtr,
};

use super::{
    gesture::{GestureAction, GestureSet},
    DrawUpdate, OnModify, RedrawTrigger, UIObject,
};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::dropdown", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "ui::dropdown", $($arg)*); } }

pub type DropdownPtr = Arc<Dropdown>;

const OUTLINE_W: f32 = 1.;
const PAD_X: f32 = 12.;
const ARROW_PAD: f32 = 10.;
const FADE_MS: u64 = 150;
const FADE_STEPS: u32 = 50;

pub struct Dropdown {
    node: SceneNodeWeak,
    renderer: Renderer,
    redraw: RedrawTrigger,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    dc_key: u64,
    me: Weak<Self>,

    rect: PropertyRect,
    items: PropertyPtr,
    selected: PropertyUint32,
    item_height: PropertyFloat32,
    list_width: PropertyFloat32,
    box_color: PropertyColor,
    outline_color: PropertyColor,
    selected_color: PropertyColor,
    hover_color: PropertyColor,
    text_color: PropertyColor,
    font_size: PropertyFloat32,
    is_active: PropertyBool,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    debug: PropertyBool,
    window_scale: PropertyFloat32,

    is_open: AtomicBool,
    /// Fade step count 0..=FADE_STEPS; draw maps it to [0, 1]
    fade: AtomicU32,
    /// Hovered list row. `usize::MAX` = no hover
    hover: AtomicUsize,
    ex: SyncMutex<Option<ExecutorPtr>>,
    /// Text layouts keyed by the items string they were built from
    layout_cache: SyncMutex<Option<(String, f32, Vec<text::TextLayout>)>>,
}

impl Dropdown {
    pub async fn new(
        node: SceneNodeWeak,
        renderer: Renderer,
        redraw: RedrawTrigger,
        window_scale: PropertyFloat32,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let items = node_ref.get_property("items").unwrap();
        let selected = PropertyUint32::wrap(node_ref, Role::Internal, "selected", 0).unwrap();
        let item_height =
            PropertyFloat32::wrap(node_ref, Role::Internal, "item_height", 0).unwrap();
        let list_width = PropertyFloat32::wrap(node_ref, Role::Internal, "list_width", 0).unwrap();
        let box_color = PropertyColor::wrap(node_ref, Role::Internal, "box_color").unwrap();
        let outline_color = PropertyColor::wrap(node_ref, Role::Internal, "outline_color").unwrap();
        let selected_color =
            PropertyColor::wrap(node_ref, Role::Internal, "selected_color").unwrap();
        let hover_color = PropertyColor::wrap(node_ref, Role::Internal, "hover_color").unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let self_ = Arc::new_cyclic(|me| Self {
            node,
            renderer,
            redraw,
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),
            me: me.clone(),

            rect,
            items,
            selected,
            item_height,
            list_width,
            box_color,
            outline_color,
            selected_color,
            hover_color,
            text_color,
            font_size,
            is_active,
            z_index,
            priority,
            debug,

            window_scale,

            is_open: AtomicBool::new(false),
            fade: AtomicU32::new(0),
            hover: AtomicUsize::new(usize::MAX),
            ex: SyncMutex::new(None),
            layout_cache: SyncMutex::new(None),
        });

        Pimpl::Dropdown(self_)
    }

    /// The item labels
    fn items(&self) -> Vec<String> {
        self.items.get_str_vec().unwrap_or_default()
    }

    /// The selected index, clamped into `[0, items.len())`
    fn selected(&self, len: usize) -> Option<usize> {
        if len == 0 {
            return None
        }
        Some((self.selected.get() as usize).min(len - 1))
    }

    fn geometry(&self) -> Option<Geometry> {
        let rect = self.rect.get();
        if rect.w <= 0. || rect.h <= 0. {
            return None
        }
        let item_height = self.item_height.get();
        if item_height <= 0. || !item_height.is_finite() {
            return None
        }
        let list_w = self.list_width.get();
        let list_w = if list_w > 0. && list_w.is_finite() { list_w } else { rect.w };
        let len = self.items().len();
        Some(Geometry { rect, item_height, list_w, list_len: len })
    }

    /// List row index at widget-local `local`, if any
    fn item_at(geo: &Geometry, local: Point) -> Option<usize> {
        if local.x < 0. || local.x > geo.list_w {
            return None
        }
        let rel_y = local.y - geo.rect.h;
        if rel_y < 0. {
            return None
        }
        let idx = (rel_y / geo.item_height).floor() as usize;
        (idx < geo.list_len).then_some(idx)
    }

    /// Select `idx` and fire `selection_changed`
    async fn select(&self, idx: usize, item: String) {
        let atom = &mut self.redraw.make_guard(gfxtag!("dropdown select"));
        self.selected.set(atom, idx as u32);

        let mut data = vec![];
        (idx as u32).encode(&mut data).unwrap();
        item.encode(&mut data).unwrap();
        let node = self.node.upgrade().unwrap();
        if let Err(e) = node.trigger("selection_changed", data).await {
            w!("selection_changed trigger failed: {e}");
        }
    }

    /// Spawn a fade task; weak self cancels it if the widget drops
    fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
        let Some(ex) = self.ex.lock().clone() else { return };
        let task = ex.spawn(fut);
        self.tasks.lock().push(task);
    }

    /// Open: fade the list in
    fn open(&self) {
        self.is_open.store(true, Ordering::Relaxed);
        self.hover.store(usize::MAX, Ordering::Relaxed);
        let node = self.node.upgrade().unwrap();
        smol::block_on(node.trigger("opened", vec![])).ok();
        let me = self.me.clone();
        let redraw = self.redraw.clone();
        self.spawn(async move {
            for step in 1..=FADE_STEPS {
                let Some(this) = me.upgrade() else { return };
                this.fade.store(step, Ordering::Relaxed);
                redraw.trigger();
                darkfi::system::msleep(FADE_MS / FADE_STEPS as u64).await;
            }
        });
        self.redraw.trigger();
    }

    /// Close: fade the list out, then flip closed
    fn close(&self) {
        self.hover.store(usize::MAX, Ordering::Relaxed);
        let me = self.me.clone();
        let redraw = self.redraw.clone();
        self.spawn(async move {
            let start_step = match me.upgrade() {
                Some(this) => this.fade.load(Ordering::Relaxed),
                None => return,
            };
            for step in (0..start_step).rev() {
                let Some(this) = me.upgrade() else { return };
                this.fade.store(step, Ordering::Relaxed);
                redraw.trigger();
                darkfi::system::msleep(FADE_MS / FADE_STEPS as u64).await;
            }
            let Some(this) = me.upgrade() else { return };
            this.is_open.store(false, Ordering::Relaxed);
            let node = this.node.upgrade().unwrap();
            node.trigger("closed", vec![]).await.ok();
            redraw.trigger();
        });
    }

    /// Pointer-down, returns whether the event is claimed
    async fn pointer_down(&self, pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }
        let Some(geo) = self.geometry() else { return false };

        let was_open = self.is_open.load(Ordering::Relaxed);
        let rect = geo.rect;
        let local = Point::new(pos.x - rect.x, pos.y - rect.y);

        if was_open {
            if rect.contains(pos) {
                d!("close (box tap)");
                self.close();
                return true
            }
            if let Some(idx) = Self::item_at(&geo, local) {
                let items = self.items();
                let item = items[idx].clone();
                d!("select {idx} ({item})");
                if self.selected.get() != idx as u32 {
                    self.select(idx, item).await;
                }
                self.close();
                return true
            }
            self.close();
            return false
        }

        if rect.contains(pos) {
            d!("open");
            self.open();
            return true
        }

        false
    }

    /// Build (or reuse) one text layout per item
    fn layouts(&self, items: &[String], font_size: f32) -> Vec<text::TextLayout> {
        let joined = items.join("\u{1f}");
        let window_scale = self.window_scale.get();
        let text_color = self.text_color.get();

        {
            let cache = self.layout_cache.lock();
            if let Some((key, key_size, layouts)) = cache.as_ref() {
                if key == &joined && *key_size == font_size && layouts.len() == items.len() {
                    return layouts.clone()
                }
            }
        }

        let layouts: Vec<text::TextLayout> = items
            .iter()
            .map(|item| {
                text::make_layout2(
                    item,
                    text_color,
                    font_size,
                    1.,
                    window_scale,
                    None,
                    &[],
                    &[],
                    parley::Alignment::Start,
                    parley::OverflowWrap::Normal,
                )
            })
            .collect();

        *self.layout_cache.lock() = Some((joined, font_size, layouts.clone()));
        layouts
    }

    /// The open list as a deferred overlay (`SetPos` anchor, `SetAlpha`
    /// fade, drawn above sibling rows)
    fn build_list_instrs(&self, geo: &Geometry, renderer: &Renderer) -> Vec<DrawInstruction> {
        let items = self.items();
        if items.is_empty() {
            return vec![]
        }

        let rect = geo.rect;
        let item_h = geo.item_height;
        let list_w = geo.list_w;
        let list_h = items.len() as f32 * item_h;
        let box_color = self.box_color.get();
        let outline_color = self.outline_color.get();
        let selected_color = self.selected_color.get();
        let hover_color = self.hover_color.get();
        let selected = self.selected(items.len());
        let hover = self.hover.load(Ordering::Relaxed);
        let alpha = self.fade.load(Ordering::Relaxed) as f32 / FADE_STEPS as f32;

        let mut list_instrs = vec![DrawInstruction::Move(Point::new(rect.x, rect.y + rect.h))];

        let mut mesh = MeshBuilder::new(gfxtag!("dropdown_list"));
        mesh.draw_filled_box(&Rectangle::new(0., 0., list_w, list_h), box_color);
        mesh.draw_outline(&Rectangle::new(0., 0., list_w, list_h), outline_color, OUTLINE_W);
        if let Some(sel) = selected {
            mesh.draw_filled_box(
                &Rectangle::new(0., sel as f32 * item_h, list_w, item_h),
                selected_color,
            );
        }
        if hover != usize::MAX && Some(hover) != selected && (hover as u64) < items.len() as u64 {
            let inset = OUTLINE_W;
            mesh.draw_filled_box(
                &Rectangle::new(
                    inset,
                    hover as f32 * item_h + inset,
                    list_w - 2. * inset,
                    item_h - 2. * inset,
                ),
                hover_color,
            );
        }
        list_instrs.push(DrawInstruction::Draw(mesh.alloc(renderer).draw_untextured()));

        let font_size = self.font_size.get();
        let layouts = self.layouts(&items, font_size);
        let mut cur = Point::new(0., 0.);
        for (idx, layout) in layouts.iter().enumerate() {
            let want = Point::new(PAD_X, idx as f32 * item_h + (item_h - layout.height()) / 2.);
            list_instrs.push(DrawInstruction::Move(Point::new(want.x - cur.x, want.y - cur.y)));
            cur = want;
            let mut txt = text::render_layout(layout, renderer, gfxtag!("dropdown_item"));
            list_instrs.append(&mut txt);
        }

        vec![
            DrawInstruction::SetPos(Point::new(0., 0.)),
            DrawInstruction::SetAlpha(alpha),
            DrawInstruction::Overlay(list_instrs),
        ]
    }

    fn build_mesh(&self) -> Option<MeshBuilder> {
        let geo = self.geometry()?;
        let rect = geo.rect;
        let mut mesh = MeshBuilder::new(gfxtag!("dropdown"));

        let arrow_color = [0., 0.94, 1., 1.];
        let arrow_shape = shape::create_dropdown_arrow(arrow_color);
        let arrow_size = rect.h * 0.5;
        let cx = rect.w - arrow_size / 2. - ARROW_PAD;
        let cy = rect.h / 2.;
        let flip = if self.is_open.load(Ordering::Relaxed) { -1. } else { 1. };
        if !arrow_shape.verts.is_empty() {
            if let Ok(mut verts) = arrow_shape.eval(rect.w, rect.h) {
                for vert in &mut verts {
                    vert.pos[0] = vert.pos[0] * arrow_size + cx;
                    vert.pos[1] = vert.pos[1] * arrow_size * flip + cy;
                }
                mesh.append(verts, arrow_shape.indices.clone());
            }
        }

        Some(mesh)
    }
}

struct Geometry {
    rect: Rectangle,
    item_height: f32,
    list_w: f32,
    list_len: usize,
}

#[async_trait]
impl UIObject for Dropdown {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);
        *self.ex.lock() = Some(ex.clone());

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.items.clone(), |self_, _| async move {
            self_.layout_cache.lock().take();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.selected.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.item_height.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.list_width.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.box_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.outline_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.selected_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.hover_color.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.text_color.prop(), |self_, _| async move {
            self_.layout_cache.lock().take();
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.font_size.prop(), |self_, _| async move {
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.is_active.prop(), |self_, _| async move {
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

    #[instrument(target = "ui::dropdown")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        if let Err(e) = self.rect.eval(atom, &parent_rect) {
            w!("Rect eval failure: {e}");
        }

        let rect = self.rect.get();
        let mut instrs = vec![DrawInstruction::Move(rect.pos())];

        if let Some(mesh) = self.build_mesh() {
            instrs.push(DrawInstruction::Draw(mesh.alloc(&self.renderer).draw_untextured()));
        }

        if self.geometry().is_some() {
            let items = self.items();
            if let Some(sel) = self.selected(items.len()) {
                let layout = self.layouts(std::slice::from_ref(&items[sel]), self.font_size.get());
                let y = (rect.h - layout[0].height()) / 2.;
                instrs.push(DrawInstruction::Move(Point::new(PAD_X, y)));
                let mut txt =
                    text::render_layout(&layout[0], &self.renderer, gfxtag!("dropdown_label"));
                instrs.append(&mut txt);
            }
        }

        if self.debug.get() {
            let mut dbg = MeshBuilder::new(gfxtag!("dropdown_debug"));
            dbg.draw_outline(&rect.with_zero_pos(), [1., 0., 0., 1.], 1.);
            instrs.push(DrawInstruction::Draw(dbg.alloc(&self.renderer).draw_untextured()));
        }

        if self.is_open.load(Ordering::Relaxed) {
            if let Some(geo) = self.geometry() {
                instrs.append(&mut self.build_list_instrs(&geo, &self.renderer));
            }
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "dropdown"),
            )],
        })
    }

    fn gesture_set(&self) -> GestureSet {
        GestureSet::TAP
    }

    fn gesture_hit_test(&self, pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }
        let Some(geo) = self.geometry() else { return false };
        if geo.rect.contains(pos) {
            return true
        }
        if !self.is_open.load(Ordering::Relaxed) {
            return false
        }
        let local = Point::new(pos.x - geo.rect.x, pos.y - geo.rect.y);
        Self::item_at(&geo, local).is_some()
    }

    async fn handle_gesture(&self, gesture: GestureAction) -> bool {
        match gesture {
            GestureAction::Down { pos } => self.pointer_down(pos).await,
            GestureAction::Tap { .. } => true,
            GestureAction::Up { .. } => false,
            _ => false,
        }
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }
        self.pointer_down(mouse_pos).await
    }

    /// Track the hovered list element and play the hover blip on change
    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        if !self.is_open.load(Ordering::Relaxed) {
            return false
        }
        let Some(geo) = self.geometry() else { return false };
        let local = Point::new(mouse_pos.x - geo.rect.x, mouse_pos.y - geo.rect.y);
        let idx = Self::item_at(&geo, local).unwrap_or(usize::MAX);
        let prev = self.hover.swap(idx, Ordering::Relaxed);
        if prev != idx {
            if idx != usize::MAX {
                sfx::play_tick();
            }
            self.redraw.trigger();
        }
        false
    }
}

impl Drop for Dropdown {
    fn drop(&mut self) {
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for Dropdown {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
