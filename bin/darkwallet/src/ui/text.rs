/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Mutex as SyncMutex, OnceLock, Weak};

use crate::{
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, ManagedTexturePtr, Rectangle,
        RenderApi,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_RED, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, GlyphPositionIter, TextShaper, TextShaperPtr},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::text", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::text", $($arg)*); } }

pub type TextPtr = Arc<Text>;

#[derive(Clone)]
struct TextRenderInfo {
    mesh: MeshInfo,
    texture: ManagedTexturePtr,
}

pub struct Text {
    node: SceneNodeWeak,
    render_api: RenderApi,
    text_shaper: TextShaperPtr,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    baseline: PropertyFloat32,
    debug: PropertyBool,

    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl Text {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        t!("Text::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let self_ = Arc::new(Self {
            node,
            render_api,
            text_shaper,
            tasks: OnceLock::new(),
            dc_key: OsRng.gen(),

            rect,
            z_index,
            priority,
            text,
            font_size,
            text_color,
            baseline,
            debug,

            window_scale,
            parent_rect: SyncMutex::new(None),
        });

        Pimpl::Text(self_)
    }

    fn regen_mesh(&self) -> TextRenderInfo {
        let text = self.text.get();
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let baseline = self.baseline.get();
        let debug = self.debug.get();
        let window_scale = self.window_scale.get();

        t!("Rendering label '{}'", text);
        let glyphs = self.text_shaper.shape(text, font_size, window_scale);
        let atlas = text::make_texture_atlas(&self.render_api, &glyphs);

        let mut mesh = MeshBuilder::new();
        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);
        for (mut glyph_rect, glyph) in glyph_pos_iter.zip(glyphs.iter()) {
            let uv_rect = atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");

            if debug {
                mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            }

            let mut color = text_color.clone();
            if glyph.sprite.has_color {
                color = COLOR_WHITE;
            }
            mesh.draw_box(&glyph_rect, color, uv_rect);
        }

        if debug {
            let mut rect = self.rect.get();
            rect.x = 0.;
            rect.y = 0.;
            mesh.draw_outline(&rect, COLOR_RED, 1.);
        }

        let mesh = mesh.alloc(&self.render_api);

        TextRenderInfo { mesh, texture: atlas.texture }
    }

    async fn redraw(self: Arc<Self>) {
        let timest = unixtime();
        t!("Text::redraw({:?})", self.node.upgrade().unwrap());
        let Some(parent_rect) = self.parent_rect.lock().unwrap().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::text", "Text failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
        t!("replace draw calls done");
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();

        let render_info = self.regen_mesh();

        let mesh = GfxDrawMesh {
            vertex_buffer: render_info.mesh.vertex_buffer,
            index_buffer: render_info.mesh.index_buffer,
            texture: Some(render_info.texture),
            num_elements: render_info.mesh.num_elements,
        };

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall {
                    instrs: vec![
                        GfxDrawInstruction::Move(rect.pos()),
                        GfxDrawInstruction::Draw(mesh),
                    ],
                    dcs: vec![],
                    z_index: self.z_index.get(),
                },
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
        on_modify.when_change(self.baseline.prop(), Self::redraw);

        self.tasks.set(on_modify.tasks);
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        t!("Text::draw({:?})", self.node.upgrade().unwrap());
        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.get_draw_calls(parent_rect).await
    }
}

impl Drop for Text {
    fn drop(&mut self) {
        self.render_api.replace_draw_calls(unixtime(), vec![(self.dc_key, Default::default())]);
    }
}
