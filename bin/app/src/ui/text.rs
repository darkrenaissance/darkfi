/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use crate::{
    gfx::{gfxtag, GfxDrawCall, GfxDrawInstruction, Rectangle, RenderApi},
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyRect,
        PropertyStr, PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
    text2::{self, TEXT_CTX},
    util::{i18n::I18nBabelFish, unixtime},
    ExecutorPtr,
};

use super::{DrawTrace, DrawUpdate, OnModify, UIObject};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::text", $($arg)*); } }

pub type TextPtr = Arc<Text>;

pub struct Text {
    node: SceneNodeWeak,
    render_api: RenderApi,
    i18n_fish: I18nBabelFish,
    tasks: SyncMutex<Vec<smol::Task<()>>>,

    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    lineheight: PropertyFloat32,
    use_i18n: PropertyBool,
    debug: PropertyBool,

    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Text {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        i18n_fish: I18nBabelFish,
    ) -> Pimpl {
        t!("Text::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let lineheight = PropertyFloat32::wrap(node_ref, Role::Internal, "lineheight", 0).unwrap();
        let use_i18n = PropertyBool::wrap(node_ref, Role::Internal, "use_i18n", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let self_ = Arc::new(Self {
            node,
            render_api,
            i18n_fish,
            tasks: SyncMutex::new(vec![]),
            dc_key: OsRng.gen(),

            rect,
            z_index,
            priority,
            text,
            font_size,
            text_color,
            lineheight,
            use_i18n,
            debug,

            window_scale,
            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Text(self_)
    }

    async fn regen_mesh(&self) -> Vec<GfxDrawInstruction> {
        let text = self.text.get();
        let font_size = self.font_size.get();
        let lineheight = self.lineheight.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();

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

        let layout = {
            let mut txt_ctx = TEXT_CTX.get().await;
            txt_ctx.make_layout(&text, text_color, font_size, lineheight, window_scale, None, &[])
        };

        let mut debug_opts = text2::DebugRenderOptions::OFF;
        if self.debug.get() {
            debug_opts |= text2::DebugRenderOptions::BASELINE;
        }

        text2::render_layout_with_opts(&layout, debug_opts, &self.render_api, gfxtag!("text"))
    }

    async fn redraw(self: Arc<Self>) {
        let trace: DrawTrace = rand::random();
        let timest = unixtime();
        t!("Text::redraw({:?}) [trace={trace}]", self.node.upgrade().unwrap());
        let Some(parent_rect) = self.parent_rect.lock().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::text", "Text failed to draw [trace={trace}]");
            return
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
        t!("Text::redraw() DONE [trace={trace}]");
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();

        let mut instrs = vec![GfxDrawInstruction::Move(rect.pos())];
        instrs.append(&mut self.regen_mesh().await);

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall::new(instrs, vec![], self.z_index.get(), "text"),
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
        on_modify.when_change(self.rect.prop(), Self::redraw);
        on_modify.when_change(self.z_index.prop(), Self::redraw);
        on_modify.when_change(self.text.prop(), Self::redraw);
        on_modify.when_change(self.font_size.prop(), Self::redraw);
        on_modify.when_change(self.text_color.prop(), Self::redraw);
        on_modify.when_change(self.debug.prop(), Self::redraw);

        *self.tasks.lock() = on_modify.tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace: DrawTrace,
        _atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("Text::draw({:?}) [trace={trace}]", self.node.upgrade().unwrap());
        *self.parent_rect.lock() = Some(parent_rect);
        self.get_draw_calls(parent_rect).await
    }

    fn set_i18n(&self, i18n_fish: &I18nBabelFish) {
        self.i18n_fish.set(i18n_fish);
    }
}

impl Drop for Text {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(unixtime(), vec![(self.dc_key, Default::default())]);
    }
}
