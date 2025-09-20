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

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use parking_lot::Mutex as SyncMutex;
use std::sync::Arc;

use crate::{
    gfx::{Point, Rectangle},
    prop::{PropertyAtomicGuard, PropertyFloat32, PropertyPtr, PropertyRect, Role},
    text2::Editor,
};

use super::EditorHandle;

pub enum BaseEditType {
    SingleLine,
    MultiLine,
}

#[async_trait]
pub(super) trait EditorBehavior: Send + Sync {
    async fn eval_rect(&self, atom: &mut PropertyAtomicGuard);

    //fn scroll(&self) -> Point;

    /// Inner position used for rendering
    fn inner_pos(&self) -> Point;

    fn allow_endl(&self) -> bool;
}

pub(super) struct MultiLine {
    pub min_height: PropertyFloat32,
    pub max_height: PropertyFloat32,
    pub content_height: PropertyFloat32,
    pub rect: PropertyRect,
    pub padding: PropertyPtr,
    pub parent_rect: Arc<SyncMutex<Option<Rectangle>>>,
    pub editor: Arc<AsyncMutex<Option<Editor>>>,
}

impl MultiLine {
    /// Lazy-initializes the editor and returns a handle to it
    async fn lock_editor<'a>(&'a self) -> EditorHandle<'a> {
        EditorHandle { guard: self.editor.lock().await }
    }

    fn bounded_height(&self, height: f32) -> f32 {
        height.clamp(self.min_height.get(), self.max_height.get())
    }

    fn padding_top(&self) -> f32 {
        self.padding.get_f32(0).unwrap()
    }
    fn padding_bottom(&self) -> f32 {
        self.padding.get_f32(1).unwrap()
    }
}

#[async_trait]
impl EditorBehavior for MultiLine {
    async fn eval_rect(&self, atom: &mut PropertyAtomicGuard) {
        let parent_rect = self.parent_rect.lock().clone().unwrap();
        // First we evaluate the width based off the parent dimensions
        self.rect
            .eval_with(
                atom,
                vec![2],
                vec![
                    ("parent_w".to_string(), parent_rect.w),
                    ("parent_h".to_string(), parent_rect.h),
                ],
            )
            .unwrap();

        // Use the width to adjust the height calcs
        let rect_w = self.rect.get_width();
        let content_height = {
            let mut editor = self.lock_editor().await;
            editor.set_width(rect_w);
            editor.refresh().await;
            editor.height()
        };
        self.content_height.set(atom, content_height);
        let outer_height = content_height + self.padding_top() + self.padding_bottom();
        let rect_h = self.bounded_height(outer_height);
        self.rect.prop().set_f32(atom, Role::Internal, 3, rect_h).unwrap();

        // Finally calculate the position
        self.rect
            .eval_with(
                atom,
                vec![0, 1],
                vec![
                    ("parent_w".to_string(), parent_rect.w),
                    ("parent_h".to_string(), parent_rect.h),
                    ("rect_w".to_string(), rect_w),
                    ("rect_h".to_string(), rect_h),
                ],
            )
            .unwrap();
    }

    /// Inner position used for rendering
    fn inner_pos(&self) -> Point {
        let pad_top = self.padding_top();
        let pad_bot = self.padding_bottom();
        let content_height = self.content_height.get();
        let outer_height = content_height + pad_top + pad_bot;
        let rect_h = self.rect.get_height();
        let mut inner_pos = Point::zero();
        if outer_height < rect_h {
            // Min was applied to clip. Center content inside the rect.
            inner_pos.y = (rect_h - content_height) / 2.;
        } else {
            inner_pos.y = pad_top;
        }
        inner_pos
    }

    fn allow_endl(&self) -> bool {
        true
    }
}

pub(super) struct SingleLine {
    pub content_height: PropertyFloat32,
    pub rect: PropertyRect,
    pub parent_rect: Arc<SyncMutex<Option<Rectangle>>>,
    pub editor: Arc<AsyncMutex<Option<Editor>>>,
}

impl SingleLine {
    /// Lazy-initializes the editor and returns a handle to it
    async fn lock_editor<'a>(&'a self) -> EditorHandle<'a> {
        EditorHandle { guard: self.editor.lock().await }
    }
}

#[async_trait]
impl EditorBehavior for SingleLine {
    async fn eval_rect(&self, atom: &mut PropertyAtomicGuard) {
        {
            let mut editor = self.lock_editor().await;
            editor.refresh().await;
        }

        let content_height = {
            let mut editor = self.lock_editor().await;
            editor.refresh().await;
            editor.height()
        };
        self.content_height.set(atom, content_height);

        let parent_rect = self.parent_rect.lock().clone().unwrap();
        //self.rect.eval(atom, &parent_rect).unwrap();
        self.rect
            .eval_with(
                atom,
                (0..4).collect(),
                vec![
                    ("parent_w".to_string(), parent_rect.w),
                    ("parent_h".to_string(), parent_rect.h),
                ],
            )
            .unwrap();
    }

    fn inner_pos(&self) -> Point {
        let content_height = self.content_height.get();
        let rect_h = self.rect.get_height();
        let mut inner_pos = Point::zero();
        inner_pos.y = (rect_h - content_height) / 2.;
        inner_pos
    }

    fn allow_endl(&self) -> bool {
        false
    }
}
