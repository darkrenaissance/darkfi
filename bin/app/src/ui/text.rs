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
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::sync::Arc;
use tracing::instrument;

use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, Rectangle, RenderApi, Renderer},
    mesh::MeshBuilder,
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyEnum, PropertyFloat32,
        PropertyRect, PropertyStr, PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
    text,
    util::i18n::I18nBabelFish,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, RedrawTrigger, UIObject};

pub type TextPtr = Arc<Text>;

pub struct Text {
    node: SceneNodeWeak,
    renderer: Renderer,
    i18n_fish: I18nBabelFish,
    redraw: RedrawTrigger,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

    dc_key: u64,

    rect: PropertyRect,
    height: PropertyFloat32,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    lineheight: PropertyFloat32,
    text_align: PropertyEnum,
    overflow_wrap: PropertyEnum,
    use_i18n: PropertyBool,
    debug: PropertyBool,

    window_scale: PropertyFloat32,
    /// Cached layout + rendered instrs. `None` means stale: recompute in
    /// the draw pass. Layout is the expensive part (shaping, line breaks).
    draw_cache: SyncMutex<Option<(text::TextLayout, Vec<DrawInstruction>)>>,
}

impl Text {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        renderer: Renderer,
        i18n_fish: I18nBabelFish,
        redraw: RedrawTrigger,
    ) -> Pimpl {
        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let height = PropertyFloat32::wrap(node_ref, Role::Internal, "height", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let lineheight = PropertyFloat32::wrap(node_ref, Role::Internal, "lineheight", 0).unwrap();
        let text_align = PropertyEnum::wrap(node_ref, Role::Internal, "text_align", 0).unwrap();
        let overflow_wrap =
            PropertyEnum::wrap(node_ref, Role::Internal, "overflow_wrap", 0).unwrap();
        let use_i18n = PropertyBool::wrap(node_ref, Role::Internal, "use_i18n", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            renderer,
            i18n_fish,
            redraw,
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),

            rect,
            height,
            z_index,
            priority,
            text,
            font_size,
            text_color,
            lineheight,
            text_align,
            overflow_wrap,
            use_i18n,
            debug,

            window_scale,
            draw_cache: SyncMutex::new(None),
        });

        Pimpl::Text(self_)
    }

    fn make_layout(&self) -> text::TextLayout {
        let text = self.text.get();
        let font_size = self.font_size.get();
        let lineheight = self.lineheight.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let width = self.rect.get_width();
        let text_align = self.text_align.get();
        let overflow_wrap = self.overflow_wrap.get();

        let text = if self.use_i18n.get() {
            if let Some(trans) = self.i18n_fish.tr(&text) {
                //t!("Translate '{text}' to '{trans}'");
                trans
            } else {
                format!("tr err: {}", text)
            }
        } else {
            text
        };

        text::make_layout2(
            &text,
            text_color,
            font_size,
            lineheight,
            window_scale,
            Some(width),
            &[],
            &[],
            &text_align,
            &overflow_wrap,
        )
    }

    fn regen_mesh(&self, layout: &text::TextLayout) -> Vec<DrawInstruction> {
        let mut debug_opts = text::DebugRenderOptions::OFF;
        if self.debug.get() {
            debug_opts |= text::DebugRenderOptions::BASELINE;
        }

        text::render_layout_with_opts(layout, debug_opts, &self.renderer, gfxtag!("text"))
    }

    fn get_draw_calls(
        &self,
        atom: &mut PropertyAtomicGuard,
        parent_rect: Rectangle,
    ) -> Option<DrawUpdate> {
        // Rect property is its own memo: compare before/after eval.
        let prev_rect = self.rect.get();
        self.rect.eval(atom, &parent_rect).ok()?;
        let rect = self.rect.get();
        let rect_changed = rect != prev_rect;

        // Layout depends on the width, so a rect change invalidates the
        // layout even if the text itself did not change. Compute under the
        // lock: the compute is synchronous, so concurrent invalidations
        // either land before (seen as None) or after (clear our result).
        let mut cache = self.draw_cache.lock();
        if cache.is_none() || rect_changed {
            let layout = self.make_layout();
            let mut instrs = vec![DrawInstruction::Move(rect.pos())];
            instrs.append(&mut self.regen_mesh(&layout));
            *cache = Some((layout, instrs));
        }
        let (layout, mut instrs) = cache.clone().unwrap();
        drop(cache);

        // Height output for parents that depend on it.
        self.height.set(atom, layout.height());

        if self.debug.get() {
            let rect = self.rect.get().with_zero_pos();
            let mut mesh = MeshBuilder::new(gfxtag!("text_debug-rect"));
            mesh.draw_outline(&rect, [0., 1., 0., 0.7], 1.);
            let mesh = mesh.alloc(&self.renderer).draw_untextured();
            instrs.push(DrawInstruction::Draw(mesh));
        }

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall::new(instrs, vec![], self.z_index.get(), "text"),
            )],
        })
    }
}

#[async_trait]
impl UIObject for Text {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let mut on_modify = OnModify::new(ex, self.node.clone(), me.clone());
        // Invalidate the cache, then request a pass. Internal-role echoes
        // (the pass's own evals) are skipped.
        on_modify.when_change_external(self.rect.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.z_index.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.text.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.text_align.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.font_size.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.text_color.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });
        on_modify.when_change_external(self.debug.prop(), |self_, _| async move {
            *self_.draw_cache.lock() = None;
            self_.redraw.trigger();
        });

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.draw_cache.lock() = None;
    }

    #[instrument(target = "ui::text")]
    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        self.get_draw_calls(atom, parent_rect)
    }

    fn set_i18n(&self, i18n_fish: &I18nBabelFish) {
        self.i18n_fish.set(i18n_fish);
    }
}

impl Drop for Text {
    fn drop(&mut self) {
        self.renderer.replace_draw_calls(vec![(self.dc_key, Default::default())]);
    }
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self.node.upgrade().unwrap())
    }
}
