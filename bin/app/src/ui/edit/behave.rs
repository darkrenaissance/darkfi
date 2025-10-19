/* This file is part of DarkFi (https://dark.fi)
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
use atomic_float::AtomicF32;
use parking_lot::Mutex as SyncMutex;
use std::sync::{atomic::Ordering, Arc};

use crate::{
    gfx::{Point, Rectangle},
    prop::{PropertyAtomicGuard, PropertyFloat32, PropertyPtr, PropertyRect, Role},
    text2::Editor,
};

use super::EditorHandle;

//macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::edit::behave", $($arg)*); } }

pub enum BaseEditType {
    SingleLine,
    MultiLine,
}

#[async_trait]
pub(super) trait EditorBehavior: Send + Sync {
    async fn eval_rect(&self, atom: &mut PropertyAtomicGuard);

    /// Whenever the cursor is modified this MUST be called
    /// to recalculate the scroll value.
    /// Must call redraw after this.
    async fn apply_cursor_scroll(&self);

    fn scroll(&self) -> Point;

    /// Maximum allowed scroll value
    async fn max_scroll(&self) -> f32;

    /// Inner position used for rendering
    fn inner_pos(&self) -> Point;

    fn allow_endl(&self) -> bool;

    fn scroll_ctrl(&self) -> ScrollDir;
}

pub(super) enum ScrollDir {
    Vert,
    Horiz,
}

impl ScrollDir {
    pub fn cmp(&self, grad: f32) -> bool {
        match self {
            Self::Vert => grad.abs() > 0.5,
            Self::Horiz => grad.abs() < 0.5,
        }
    }

    pub fn travel(&self, start_pos: Point, touch_pos: Point) -> f32 {
        match self {
            Self::Vert => start_pos.y - touch_pos.y,
            Self::Horiz => start_pos.x - touch_pos.x,
        }
    }
}

pub(super) struct MultiLine {
    pub min_height: PropertyFloat32,
    pub max_height: PropertyFloat32,
    pub rect: PropertyRect,
    pub baseline: PropertyFloat32,
    pub padding: PropertyPtr,
    pub cursor_descent: PropertyFloat32,
    pub parent_rect: Arc<SyncMutex<Option<Rectangle>>>,
    pub editor: Arc<AsyncMutex<Option<Editor>>>,
    pub content_height: AtomicF32,
    pub scroll: Arc<AtomicF32>,
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

    /// Gets the real cursor pos within the rect.
    async fn get_cursor_pos(&self) -> Point {
        // This is the position within the content.
        let cursor_pos = self.lock_editor().await.get_cursor_pos();
        // Apply the inner padding
        cursor_pos + self.inner_pos()
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

        let pad_right = self.padding.get_f32(1).unwrap();
        let pad_left = self.padding.get_f32(3).unwrap();

        // Use the width to adjust the height calcs
        let rect_w = self.rect.get_width() - pad_left - pad_right;
        let content_height = {
            let mut editor = self.lock_editor().await;
            editor.set_width(rect_w);
            editor.refresh().await;
            editor.height()
        };
        self.content_height.store(content_height, Ordering::Relaxed);
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

    async fn apply_cursor_scroll(&self) {
        //let pad_top = self.padding_top();
        let pad_bot = self.padding_bottom();

        let mut scroll = self.scroll.load(Ordering::Relaxed);
        let rect_h = self.max_height.get() - pad_bot;
        let cursor_y0 = self.get_cursor_pos().await.y;
        let cursor_h = self.baseline.get() + self.cursor_descent.get();
        // The bottom
        let cursor_y1 = cursor_y0 + cursor_h;
        //t!("apply_cursor_scrolling() cursor = [{cursor_y0}, {cursor_y1}] rect_h={rect_h} scroll={scroll}");

        if cursor_y1 > rect_h + scroll {
            let max_scroll = self.max_scroll().await;
            //t!("  cursor bottom below rect");
            // We want cursor_y1 = rect_h + scroll by adjusting scroll
            scroll = (cursor_y1 - rect_h).clamp(0., max_scroll);
            self.scroll.store(scroll, Ordering::Release);
        } else if cursor_y0 < scroll {
            //t!("  cursor top above rect");
            scroll = cursor_y0.max(0.);
            assert!(scroll >= 0.);
            self.scroll.store(scroll, Ordering::Release);
        }
    }

    fn scroll(&self) -> Point {
        Point::new(0., -self.scroll.load(Ordering::Relaxed))
    }

    /// Maximum allowed scroll value
    /// * `content_height` measures the height of the actual content.
    /// * `outer_height` applies the inner padding.
    /// * `rect_h` then clips the `outer_height` to min/max values.
    /// We only allow scrolling when max clipping has been applied.
    async fn max_scroll(&self) -> f32 {
        let content_height = self.content_height.load(Ordering::Relaxed);
        let outer_height = content_height + self.padding_top() + self.padding_bottom();
        let rect_h = self.rect.get_height();
        //t!("max_scroll content_height={content_height}, rect_h={rect_h}");
        (outer_height - rect_h).max(0.)
    }

    /// Inner position used for rendering
    fn inner_pos(&self) -> Point {
        let pad_top = self.padding_top();
        let pad_bot = self.padding_bottom();
        let pad_left = self.padding.get_f32(3).unwrap();

        let content_height = self.content_height.load(Ordering::Relaxed);
        let outer_height = content_height + pad_top + pad_bot;
        let rect_h = self.rect.get_height();
        let mut inner_pos = Point::zero();
        inner_pos.x = pad_left;
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

    fn scroll_ctrl(&self) -> ScrollDir {
        ScrollDir::Vert
    }
}

pub(super) struct SingleLine {
    pub rect: PropertyRect,
    pub padding: PropertyPtr,
    pub cursor_width: PropertyFloat32,
    pub parent_rect: Arc<SyncMutex<Option<Rectangle>>>,
    pub editor: Arc<AsyncMutex<Option<Editor>>>,
    pub content_height: AtomicF32,
    pub scroll: Arc<AtomicF32>,
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
        let content_height = {
            let mut editor = self.lock_editor().await;
            editor.refresh().await;
            editor.height()
        };
        self.content_height.store(content_height, Ordering::Relaxed);

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

    async fn apply_cursor_scroll(&self) {
        let pad_right = self.padding.get_f32(1).unwrap();
        let pad_left = self.padding.get_f32(3).unwrap();
        let scroll = self.scroll.load(Ordering::Relaxed);
        let rect_w = self.rect.get_width() - pad_right;
        let cursor_x0 = self.lock_editor().await.get_cursor_pos().x + pad_left;
        let cursor_x1 = cursor_x0 + self.cursor_width.get();

        if cursor_x0 < scroll {
            assert!(cursor_x0 >= 0.);
            self.scroll.store(cursor_x0, Ordering::Release);
        } else if cursor_x1 > rect_w + scroll {
            let max_scroll = self.max_scroll().await;
            let scroll = (cursor_x1 - rect_w).clamp(0., max_scroll);
            self.scroll.store(scroll, Ordering::Release);
        }
    }

    fn scroll(&self) -> Point {
        Point::new(-self.scroll.load(Ordering::Relaxed), 0.)
    }

    async fn max_scroll(&self) -> f32 {
        let pad_right = self.padding.get_f32(1).unwrap();
        let pad_left = self.padding.get_f32(3).unwrap();
        let rect_w = self.rect.get_width();
        let content_w = self.lock_editor().await.width() + self.cursor_width.get();
        (pad_left + pad_right + content_w - rect_w).max(0.)
    }

    fn inner_pos(&self) -> Point {
        let pad_left = self.padding.get_f32(3).unwrap();
        let content_height = self.content_height.load(Ordering::Relaxed);
        let rect_h = self.rect.get_height();
        let mut inner_pos = Point::zero();
        inner_pos.x = pad_left;
        inner_pos.y = (rect_h - content_height) / 2.;
        inner_pos
    }

    fn allow_endl(&self) -> bool {
        false
    }

    fn scroll_ctrl(&self) -> ScrollDir {
        ScrollDir::Horiz
    }
}
