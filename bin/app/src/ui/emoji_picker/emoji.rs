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

use async_lock::Mutex as AsyncMutex;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    gfx::{gfxtag, DrawInstruction, DrawMesh, Rectangle, RenderApi},
    mesh::{MeshBuilder, COLOR_WHITE},
    text,
};

use super::default::DEFAULT_EMOJI_LIST;

pub type EmojiMeshesPtr = Arc<AsyncMutex<EmojiMeshes>>;

pub struct EmojiMeshes {
    render_api: RenderApi,
    emoji_size: f32,
    meshes: Vec<DrawMesh>,
}

impl EmojiMeshes {
    pub fn new(render_api: RenderApi, emoji_size: f32) -> EmojiMeshesPtr {
        Arc::new(AsyncMutex::new(Self { render_api, emoji_size, meshes: vec![] }))
    }

    pub fn clear(&mut self) {
        self.meshes.clear();
    }

    pub async fn get(&mut self, i: usize) -> DrawMesh {
        assert!(i < DEFAULT_EMOJI_LIST.len());
        self.meshes.reserve_exact(DEFAULT_EMOJI_LIST.len());

        if i >= self.meshes.len() {
            //d!("EmojiMeshes loading new glyphs");
            for j in self.meshes.len()..=i {
                let emoji = DEFAULT_EMOJI_LIST[j];
                let mesh = self.gen_emoji_mesh(emoji).await;
                self.meshes.push(mesh);
            }
        }

        self.meshes[i].clone()
    }

    /// Make mesh for this emoji centered at (0, 0)
    async fn gen_emoji_mesh(&self, emoji: &str) -> DrawMesh {
        //d!("rendering emoji: '{emoji}'");
        let mut txt_ctx = text::TEXT_CTX.get().await;

        // The params here don't actually matter since we're talking about BMP fixed sizes
        let layout = txt_ctx.make_layout(emoji, COLOR_WHITE, self.emoji_size, 1., 1., None, &[]);
        drop(txt_ctx);

        let instrs = text::render_layout(&layout, &self.render_api, gfxtag!("emoji_mesh"));

        // Extract the mesh from the draw instructions
        // For a single emoji, we should get exactly one Draw instruction with a mesh
        let mesh = match instrs.first() {
            Some(DrawInstruction::Draw(mesh)) => mesh.clone(),
            _ => panic!("Expected Draw instruction for emoji"),
        };

        // For now, just return the original mesh since scaling is complex with textures
        mesh
    }
}
