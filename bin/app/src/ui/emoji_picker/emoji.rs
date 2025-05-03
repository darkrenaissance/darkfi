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

use parking_lot::Mutex as SyncMutex;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use crate::{
    gfx::{
        gfxtag, GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, ManagedTexturePtr,
        Point, Rectangle, RenderApi,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_WHITE},
    prop::{
        PropertyAtomicGuard, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, GlyphPositionIter, TextShaper, TextShaperPtr},
    util::unixtime,
    ExecutorPtr,
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

pub type EmojiMeshesPtr = Arc<SyncMutex<EmojiMeshes>>;

pub struct EmojiMeshes {
    render_api: RenderApi,
    text_shaper: TextShaperPtr,
    emoji_size: f32,
    emoji_list: LazyLock<Vec<String>>,
    meshes: Vec<GfxDrawMesh>,
}

impl EmojiMeshes {
    pub fn new(
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        emoji_size: f32,
    ) -> EmojiMeshesPtr {
        Arc::new(SyncMutex::new(Self {
            render_api,
            text_shaper,
            emoji_size,
            emoji_list: LazyLock::new(load_emoji_list),
            meshes: vec![],
        }))
    }

    pub fn clear(&mut self) {
        self.meshes.clear();
    }

    pub fn get(&mut self, i: usize) -> GfxDrawMesh {
        let emoji_list = self.get_list();
        assert!(i < emoji_list.len());
        self.meshes.reserve_exact(emoji_list.len());

        if i >= self.meshes.len() {
            //d!("EmojiMeshes loading new glyphs");
            for j in self.meshes.len()..=i {
                let emoji = &self.emoji_list[j];
                let mesh = self.gen_emoji_mesh(emoji);
                self.meshes.push(mesh);
            }
        }

        self.meshes[i].clone()
    }

    /// Make mesh for this emoji centered at (0, 0)
    fn gen_emoji_mesh(&self, emoji: &str) -> GfxDrawMesh {
        //d!("rendering emoji: '{emoji}'");
        // The params here don't actually matter since we're talking about BMP fixed sizes
        let glyphs = self.text_shaper.shape(emoji.to_string(), 10., 1.);
        assert_eq!(glyphs.len(), 1);
        let atlas = text::make_texture_atlas(&self.render_api, gfxtag!("emoji_mesh"), &glyphs);
        let glyph = glyphs.into_iter().next().unwrap();

        // Emoji's vary in size. We make them all a consistent size.
        let w = self.emoji_size;
        let h =
            (glyph.sprite.bmp_height as f32) * self.emoji_size / (glyph.sprite.bmp_width as f32);
        // Center at origin
        let x = -w / 2.;
        let y = -h / 2.;

        let uv = atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");
        let mut mesh = MeshBuilder::new(gfxtag!("emoji_mesh"));
        mesh.draw_box(&Rectangle::new(x, y, w, h), COLOR_WHITE, &uv);
        mesh.alloc(&self.render_api).draw_with_texture(atlas.texture)
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
