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
    gfx::{GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, Rectangle, RenderApi},
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, GlyphPositionIter, TextShaper, TextShaperPtr},
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

pub type TextPtr = Arc<Text>;

#[derive(Clone)]
struct TextRenderInfo {
    mesh: MeshInfo,
    texture_id: GfxTextureId,
}

pub struct Text {
    node: SceneNodeWeak,
    render_api: RenderApi,
    text_shaper: TextShaperPtr,
    tasks: OnceLock<Vec<smol::Task<()>>>,

    render_info: SyncMutex<TextRenderInfo>,
    dc_key: u64,

    rect: PropertyRect,
    z_index: PropertyUint32,
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
        debug!(target: "ui::text", "Text::new()");

        let node_ref = &node.upgrade().unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let render_info = Self::regen_mesh(
            &render_api,
            &text_shaper,
            text.get(),
            font_size.get(),
            text_color.get(),
            baseline.get(),
            debug.get(),
            window_scale.get(),
        );

        let self_ = Arc::new(Self {
            node,
            render_api,
            text_shaper,
            tasks: OnceLock::new(),
            render_info: SyncMutex::new(render_info),
            dc_key: OsRng.gen(),

            rect,
            z_index,
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

    fn regen_mesh(
        render_api: &RenderApi,
        text_shaper: &TextShaper,
        text: String,
        font_size: f32,
        text_color: Color,
        baseline: f32,
        debug: bool,
        window_scale: f32,
    ) -> TextRenderInfo {
        debug!(target: "ui::text", "Rendering label '{}'", text);
        let glyphs = text_shaper.shape(text, font_size, window_scale);
        let atlas = text::make_texture_atlas(render_api, &glyphs);

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

        let mesh = mesh.alloc(&render_api);

        TextRenderInfo { mesh, texture_id: atlas.texture_id }
    }

    async fn redraw(self: Arc<Self>) {
        let Some(parent_rect) = self.parent_rect.lock().unwrap().clone() else { return };

        let Some(draw_update) = self.get_draw_calls(parent_rect).await else {
            error!(target: "ui::text", "Text failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls);
        debug!(target: "ui::text", "replace draw calls done");

        // We're finished with these so clean up.
        for texture in draw_update.freed_textures {
            self.render_api.delete_texture(texture);
        }
        for buff in draw_update.freed_buffers {
            self.render_api.delete_buffer(buff);
        }
    }

    async fn get_draw_calls(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::text", "Text::get_draw_calls()");
        self.rect.eval(&parent_rect).ok()?;
        let rect = self.rect.get();

        let old_render_info = self.render_info.lock().unwrap().clone();

        let render_info = Self::regen_mesh(
            &self.render_api,
            &self.text_shaper,
            self.text.get(),
            self.font_size.get(),
            self.text_color.get(),
            self.baseline.get(),
            self.debug.get(),
            self.window_scale.get(),
        );

        *self.render_info.lock().unwrap() = render_info.clone();

        let mesh = GfxDrawMesh {
            vertex_buffer: render_info.mesh.vertex_buffer,
            index_buffer: render_info.mesh.index_buffer,
            texture: Some(render_info.texture_id),
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
            freed_textures: vec![old_render_info.texture_id],
            freed_buffers: vec![
                old_render_info.mesh.vertex_buffer,
                old_render_info.mesh.index_buffer,
            ],
        })
    }
}

#[async_trait]
impl UIObject for Text {
    fn z_index(&self) -> u32 {
        self.z_index.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let node_ref = &self.node.upgrade().unwrap();
        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
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
        debug!(target: "ui::text", "Text::draw()");
        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.get_draw_calls(parent_rect).await
    }
}

/*
impl Stoppable for Text {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        let render_info = self.render_info.lock().unwrap().clone();
        let vertex_buffer = render_info.mesh.vertex_buffer;
        let index_buffer = render_info.mesh.index_buffer;
        let texture_id = render_info.texture_id;
        self.render_api.delete_buffer(vertex_buffer);
        self.render_api.delete_buffer(index_buffer);
        self.render_api.delete_texture(texture_id);
    }
}
*/
