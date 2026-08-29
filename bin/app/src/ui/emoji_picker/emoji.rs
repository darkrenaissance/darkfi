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

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use atomic_float::AtomicF32;
use parking_lot::Mutex as SyncMutex;

use crate::{
    gfx::{gfxtag, DrawMesh, EpochTracker, Rectangle, Renderer},
    mesh::{MeshBuilder, COLOR_WHITE},
    text,
    text::atlas::MAX_TEXTURE_DIMENSION,
    util::spawn_thread,
};

use super::default::DEFAULT_EMOJI_LIST;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui:emoji_picker", $($arg)*); } }

/// The fully-built atlas cache: one prebuilt quad mesh per emoji,
/// index-aligned with `DEFAULT_EMOJI_LIST`, all referencing a single
/// shared atlas texture.
pub struct EmojiAtlasData {
    meshes: Vec<(DrawMesh, Rectangle)>,
}

pub struct EmojiMeshes {
    renderer: Renderer,
    emoji_size: AtomicF32,
    epoch_tracker: SyncMutex<EpochTracker>,
    inner: SyncMutex<Option<EmojiAtlasData>>,
    building: AtomicBool,
}

pub type EmojiMeshesPtr = Arc<EmojiMeshes>;

impl EmojiMeshes {
    pub fn new(renderer: Renderer, emoji_size: f32) -> EmojiMeshesPtr {
        let epoch_tracker = EpochTracker::new(&renderer);
        Arc::new(Self {
            renderer,
            emoji_size: AtomicF32::new(emoji_size),
            epoch_tracker: SyncMutex::new(epoch_tracker),
            inner: SyncMutex::new(None),
            building: AtomicBool::new(false),
        })
    }

    /// Build the atlas at the current emoji size and swap it in. The
    /// whole build (layouts, rasters, single texture upload, quad
    /// meshes) runs outside the mutex, so it never blocks teardown or
    /// the draw pass. A build whose epoch went stale mid-flight is
    /// discarded.
    pub fn make(&self) {
        let now = Instant::now();
        let epoch_before = self.renderer.epoch.load(Ordering::Relaxed);
        let emoji_size = self.emoji_size.load(Ordering::Relaxed);

        let strings: Vec<&str> = DEFAULT_EMOJI_LIST.to_vec();
        let string_atlas = text::make_string_atlas(
            &strings,
            emoji_size,
            1.,
            MAX_TEXTURE_DIMENSION,
            &self.renderer,
            gfxtag!("emoji_atlas"),
        );

        let mut meshes = Vec::with_capacity(string_atlas.entries.len());
        for entry in &string_atlas.entries {
            let mut mesh = MeshBuilder::new(gfxtag!("emoji_atlas"));
            for glyph in &entry.glyphs {
                mesh.draw_box(&glyph.rect, COLOR_WHITE, &glyph.uv_rect);
            }
            let draw_mesh = mesh
                .alloc(&self.renderer)
                .draw_with_textures(vec![string_atlas.rendered.texture.clone()]);
            meshes.push((draw_mesh, entry.ink_bounds));
        }

        let data = EmojiAtlasData { meshes };

        let epoch_after = self.renderer.epoch.load(Ordering::Relaxed);
        if epoch_before != epoch_after {
            d!("Discarding emoji atlas built for a stale epoch");
            return
        }

        *self.inner.lock() = Some(data);
        d!("Built emoji atlas ({} emoji) in {:?}", DEFAULT_EMOJI_LIST.len(), now.elapsed());
    }

    /// Start building the atlas if it is not available yet. Returns
    /// `true` only when the atlas is available right now; `false`
    /// means a build was started (or is already in flight) and the
    /// caller should retry later.
    pub fn start_make(self: Arc<Self>) -> bool {
        if self.inner.lock().is_some() {
            return true
        }
        if self.building.swap(true, Ordering::SeqCst) {
            return false
        }

        spawn_thread("emoji-atlas", move || {
            self.make();
            self.building.store(false, Ordering::SeqCst);
        });

        false
    }

    /// Prebuilt quad mesh and ink bounds for emoji `i`, or `None` while
    /// the atlas is unbuilt (or was dropped by a size change, epoch
    /// bump, or teardown).
    pub fn get(&self, i: usize) -> Option<(DrawMesh, Rectangle)> {
        assert!(i < DEFAULT_EMOJI_LIST.len());

        if self.epoch_tracker.lock().changed() {
            *self.inner.lock() = None;
        }

        let guard = self.inner.lock();
        let data = guard.as_ref()?;
        data.meshes.get(i).cloned()
    }

    /// Drop the atlas; it is rebuilt lazily via `start_make`.
    pub fn clear(&self) {
        *self.inner.lock() = None;
    }

    /// Change the emoji size and drop the atlas so the next build uses
    /// the new size.
    pub fn set_size(&self, emoji_size: f32) {
        self.emoji_size.store(emoji_size, Ordering::Relaxed);
        self.clear();
    }
}
