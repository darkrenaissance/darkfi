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

use std::ops::{Add, AddAssign, Sub, SubAssign};

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn unpack(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    pub fn as_arr(&self) -> [f32; 2] {
        [self.x, self.y]
    }

    pub fn offset(&self, off_x: f32, off_y: f32) -> Self {
        Self { x: self.x + off_x, y: self.y + off_y }
    }

    pub fn to_rect(&self, w: f32, h: f32) -> Rectangle {
        Rectangle { x: self.x, y: self.y, w, h }
    }
}

impl From<[f32; 2]> for Point {
    fn from(pos: [f32; 2]) -> Self {
        Self { x: pos[0], y: pos[1] }
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self { x: self.x + other.x, y: self.y + other.y }
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self { x: self.x - other.x, y: self.y - other.y }
    }
}

impl AddAssign for Point {
    fn add_assign(&mut self, other: Self) {
        *self = Self { x: self.x + other.x, y: self.y + other.y };
    }
}

impl SubAssign for Point {
    fn sub_assign(&mut self, other: Self) {
        *self = Self { x: self.x - other.x, y: self.y - other.y };
    }
}

#[derive(Debug, Clone)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rectangle {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn zero() -> Self {
        Self { x: 0., y: 0., w: 0., h: 0. }
    }

    pub fn from_array(arr: [f32; 4]) -> Self {
        Self { x: arr[0], y: arr[1], w: arr[2], h: arr[3] }
    }

    pub fn clip(&self, other: &Self) -> Option<Self> {
        if other.x + other.w < self.x ||
            other.x > self.x + self.w ||
            other.y + other.h < self.y ||
            other.y > self.y + self.h
        {
            return None
        }

        let mut clipped = other.clone();
        if clipped.x < self.x {
            clipped.x = self.x;
            clipped.w = other.x + other.w - clipped.x;
        }
        if clipped.y < self.y {
            clipped.y = self.y;
            clipped.h = other.y + other.h - clipped.y;
        }
        if clipped.x + clipped.w > self.x + self.w {
            clipped.w = self.x + self.w - clipped.x;
        }
        if clipped.y + clipped.h > self.y + self.h {
            clipped.h = self.y + self.h - clipped.y;
        }
        Some(clipped)
    }

    pub fn clip_point(&self, point: &mut Point) {
        if point.x < self.x {
            point.x = self.x;
        }
        if point.y < self.y {
            point.y = self.y;
        }
        if point.x > self.x + self.w {
            point.x = self.x + self.w;
        }
        if point.y > self.y + self.h {
            point.y = self.y + self.h;
        }
    }

    pub fn contains(&self, point: Point) -> bool {
        self.x <= point.x &&
            point.x <= self.x + self.w &&
            self.y <= point.y &&
            point.y <= self.y + self.h
    }

    pub fn rhs(&self) -> f32 {
        self.x + self.w
    }
    pub fn bhs(&self) -> f32 {
        self.y + self.h
    }

    pub fn pos(&self) -> Point {
        Point { x: self.x, y: self.y }
    }
    pub fn corner(&self) -> Point {
        Point { x: self.x + self.w, y: self.y + self.h }
    }

    #[deprecated]
    pub fn top_left(&self) -> Point {
        Point { x: self.x, y: self.y }
    }
    #[deprecated]
    pub fn bottom_right(&self) -> Point {
        Point { x: self.x + self.w, y: self.y + self.h }
    }

    pub fn includes(&self, child: &Self) -> bool {
        self.contains(child.pos()) && self.contains(child.corner())
    }
}

impl Add<Point> for Rectangle {
    type Output = Rectangle;

    fn add(self, other: Point) -> Self::Output {
        Self { x: self.x + other.x, y: self.y + other.y, w: self.w, h: self.h }
    }
}

impl Sub<Point> for Rectangle {
    type Output = Rectangle;

    fn sub(self, other: Point) -> Self::Output {
        Self { x: self.x - other.x, y: self.y - other.y, w: self.w, h: self.h }
    }
}
