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
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use std::sync::Arc;

use crate::{
    gfx::{DrawCall, Point, Rectangle, RendererSync},
    prop::{BatchGuardPtr, PropertyAtomicGuard},
    scene::{Pimpl, SceneNodeWeak},
    util::i18n::I18nBabelFish,
    ExecutorPtr,
};

use super::{DrawUpdate, Layer, LayerPtr, UIObject};

pub type ScrollLayerPtr = Arc<ScrollLayer>;

pub struct ScrollLayer {
    inner: LayerPtr,
}

impl ScrollLayer {
    pub async fn new(node: SceneNodeWeak, renderer: crate::gfx::Renderer) -> Pimpl {
        let layer = Layer::new(node.clone(), renderer).await;
        let inner = match layer {
            Pimpl::Layer(l) => l,
            _ => unreachable!(),
        };

        Pimpl::ScrollLayer(Arc::new(Self { inner }))
    }
}

#[async_trait]
impl UIObject for ScrollLayer {
    fn priority(&self) -> u32 {
        self.inner.priority()
    }

    fn init(&self) {
        self.inner.init();
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        self.inner.clone().start(ex).await;
    }

    fn stop(&self) {
        self.inner.stop();
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        self.inner.draw(parent_rect, atom).await
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        self.inner.handle_char(key, mods, repeat).await
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        self.inner.handle_key_down(key, mods, repeat).await
    }

    async fn handle_key_up(&self, key: KeyCode, mods: KeyMods) -> bool {
        self.inner.handle_key_up(key, mods).await
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        self.inner.handle_mouse_btn_down(btn, mouse_pos).await
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        self.inner.handle_mouse_btn_up(btn, mouse_pos).await
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        self.inner.handle_mouse_move(mouse_pos).await
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        self.inner.handle_mouse_wheel(wheel_pos).await
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        self.inner.handle_touch(phase, id, touch_pos).await
    }

    fn handle_touch_sync(
        &self,
        renderer: &RendererSync,
        phase: TouchPhase,
        id: u64,
        touch_pos: Point,
    ) -> bool {
        self.inner.handle_touch_sync(renderer, phase, id, touch_pos)
    }

    fn set_i18n(&self, i18n_fish: &I18nBabelFish) {
        self.inner.set_i18n(i18n_fish);
    }
}

impl std::fmt::Debug for ScrollLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}
