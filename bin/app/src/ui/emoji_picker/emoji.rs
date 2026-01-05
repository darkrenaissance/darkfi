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
    sync::{Arc, LazyLock},
};

use crate::{
    gfx::{gfxtag, DrawInstruction, DrawMesh, Rectangle, RenderApi},
    mesh::{MeshBuilder, COLOR_WHITE},
    text2,
};

use super::default;

#[cfg(target_os = "android")]
pub fn get_emoji_list_path() -> PathBuf {
    crate::android::get_external_storage_path().join("emoji.txt")
}

#[cfg(not(target_os = "android"))]
pub fn get_emoji_list_path() -> PathBuf {
    dirs::data_local_dir().unwrap().join("darkfi/emoji.txt")
}

pub type EmojiMeshesPtr = Arc<AsyncMutex<EmojiMeshes>>;

pub struct EmojiMeshes {
    render_api: RenderApi,
    emoji_size: f32,
    emoji_list: LazyLock<Vec<String>>,
    meshes: Vec<DrawMesh>,
}

impl EmojiMeshes {
    pub fn new(render_api: RenderApi, emoji_size: f32) -> EmojiMeshesPtr {
        Arc::new(AsyncMutex::new(Self {
            render_api,
            emoji_size,
            emoji_list: LazyLock::new(load_emoji_list),
            meshes: vec![],
        }))
    }

    pub fn clear(&mut self) {
        self.meshes.clear();
    }

    pub async fn get(&mut self, i: usize) -> DrawMesh {
        let emoji_list = self.get_list();
        assert!(i < emoji_list.len());
        self.meshes.reserve_exact(emoji_list.len());

        if i >= self.meshes.len() {
            //d!("EmojiMeshes loading new glyphs");
            for j in self.meshes.len()..=i {
                let emoji = &self.emoji_list[j];
                let mesh = self.gen_emoji_mesh(emoji).await;
                self.meshes.push(mesh);
            }
        }

        self.meshes[i].clone()
    }

    /// Make mesh for this emoji centered at (0, 0)
    async fn gen_emoji_mesh(&self, emoji: &str) -> DrawMesh {
        //d!("rendering emoji: '{emoji}'");
        let mut txt_ctx = text2::TEXT_CTX.get().await;

        // The params here don't actually matter since we're talking about BMP fixed sizes
        let layout = txt_ctx.make_layout(emoji, COLOR_WHITE, self.emoji_size, 1., 1., None, &[]);
        drop(txt_ctx);

        let instrs = text2::render_layout(&layout, &self.render_api, gfxtag!("emoji_mesh"));

        // Extract the mesh from the draw instructions
        // For a single emoji, we should get exactly one Draw instruction with a mesh
        let mesh = match instrs.first() {
            Some(DrawInstruction::Draw(mesh)) => mesh.clone(),
            _ => panic!("Expected Draw instruction for emoji"),
        };

        // Emoji's vary in size. We make them all a consistent size.
        // We need to scale the mesh to match emoji_size.
        // TODO: Implement proper scaling for text2 API
        /*
        let bbox = layout.metrics().bounds;
        let orig_w = bbox.width().max(1.0);
        let orig_h = bbox.height().max(1.0);
        let w = self.emoji_size;
        let h = self.emoji_size * orig_h / orig_w;

        // Create a new mesh with the scaled size centered at origin
        let x = -w / 2.;
        let y = -h / 2.;
        let mut mesh_builder = MeshBuilder::new(gfxtag!("emoji_mesh"));
        */

        // For now, just return the original mesh since scaling is complex with textures
        mesh
    }

    pub fn get_list<'a>(&'a self) -> &'a Vec<String> {
        LazyLock::force(&self.emoji_list)
    }
}

fn load_emoji_list() -> Vec<String> {
    match load_custom_emoji_list(&get_emoji_list_path()) {
        Some(emojis) => emojis,
        None => default::create_default_emoji_list(),
    }
}

fn load_custom_emoji_list(path: &Path) -> Option<Vec<String>> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    let mut emojis = vec![];
    for mut line in reader.lines().map_while(Result::ok) {
        remove_whitespace(&mut line);
        let line = strip_comment(&line);
        if line.is_empty() {
            continue
        }
        let emoji = unescape_unicode(line)?;
        emojis.push(emoji);
    }
    Some(emojis)
}

fn unescape_unicode(input: &str) -> Option<String> {
    let re = regex::Regex::new(r"\\u\{([0-9A-Fa-f]+)\}").unwrap();

    // There is no way to bail from a failed regex so we use this workaround instead.
    let mut failed = false;
    let result = re
        .replace_all(input, |caps: &regex::Captures| {
            let Ok(code) = u32::from_str_radix(&caps[1], 16) else {
                failed = true;
                return String::new()
            };
            let Some(chr) = char::from_u32(code) else {
                failed = true;
                return String::new()
            };
            chr.to_string()
        })
        .into_owned();

    if failed {
        return None
    }

    Some(result)
}

fn remove_whitespace(s: &mut String) {
    s.retain(|c| !c.is_whitespace());
}

fn strip_comment(s: &str) -> &str {
    s.split('#').next().unwrap_or(s)
}
