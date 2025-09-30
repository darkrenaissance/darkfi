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

use async_trait::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, Sub, SubAssign};

#[derive(Clone, Copy, Debug, SerialEncodable, SerialDecodable)]
pub struct Dimension {
    pub w: f32,
    pub h: f32,
}

impl Dimension {
    pub fn contains(&self, other: &Dimension) -> bool {
        other.w <= self.w && other.h <= self.h
    }
}

impl From<[f32; 2]> for Dimension {
    fn from(dim: [f32; 2]) -> Self {
        Self { w: dim[0], h: dim[1] }
    }
}

impl Mul<f32> for Dimension {
    type Output = Dimension;

    fn mul(self, scale: f32) -> Self::Output {
        Self { w: self.w * scale, h: self.h * scale }
    }
}

impl Div<f32> for Dimension {
    type Output = Dimension;

    fn div(self, scale: f32) -> Self::Output {
        Self { w: self.w / scale, h: self.h / scale }
    }
}

#[derive(Clone, Copy, Default, SerialEncodable, SerialDecodable)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0., y: 0. }
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

    pub fn dist_sq(&self, other: Point) -> f32 {
        (self.x - other.x).powi(2) + (self.y - other.y).powi(2)
    }
    pub fn dist(&self, other: Point) -> f32 {
        self.dist_sq(other).sqrt()
    }

    pub fn normalize(&mut self) {
        let scale = self.dist(Point::zero());
        self.x /= scale;
        self.y /= scale;
        assert!((self.dist(Point::zero()) - 1.) < f32::EPSILON);
    }

    /// Counterclockwise perp vector (with -y up)
    pub fn perp_left(&self) -> Point {
        Point::new(self.y, -self.x)
    }
    /// Clockwise perp vector (with +y down)
    pub fn perp_right(&self) -> Point {
        Point::new(-self.y, self.x)
    }
}

impl From<[f32; 2]> for Point {
    fn from(pos: [f32; 2]) -> Self {
        Self { x: pos[0], y: pos[1] }
    }
}

impl std::fmt::Debug for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
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

impl Mul<f32> for Point {
    type Output = Self;

    fn mul(self, scale: f32) -> Self {
        Point::new(scale * self.x, scale * self.y)
    }
}

#[derive(Clone, Copy, SerialEncodable, SerialDecodable)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rectangle {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn zero() -> Self {
        Self { x: 0., y: 0., w: 0., h: 0. }
    }

    /// Use from() instead
    #[deprecated]
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

    pub fn with_zero_pos(&self) -> Self {
        Self::new(0., 0., self.w, self.h)
    }

    pub fn clip_point(&self, mut point: Point) -> Point {
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
        point
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
    pub fn center(&self) -> Point {
        Point { x: self.x + self.w / 2., y: self.y + self.h / 2. }
    }
    pub fn top_right(&self) -> Point {
        Point { x: self.rhs(), y: self.y }
    }
    pub fn bot_left(&self) -> Point {
        Point { x: self.x, y: self.y + self.h }
    }

    pub fn dim(&self) -> Dimension {
        Dimension { w: self.w, h: self.h }
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

impl From<[f32; 4]> for Rectangle {
    fn from(rect: [f32; 4]) -> Self {
        Self { x: rect[0], y: rect[1], w: rect[2], h: rect[3] }
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

impl Mul<f32> for Rectangle {
    type Output = Rectangle;

    fn mul(self, scale: f32) -> Self::Output {
        Self { x: self.x * scale, y: self.y * scale, w: self.w * scale, h: self.h * scale }
    }
}

impl Div<f32> for Rectangle {
    type Output = Rectangle;

    fn div(self, scale: f32) -> Self::Output {
        Self { x: self.x / scale, y: self.y / scale, w: self.w / scale, h: self.h / scale }
    }
}

impl DivAssign<f32> for Rectangle {
    fn div_assign(&mut self, scale: f32) {
        self.x /= scale;
        self.y /= scale;
        self.w /= scale;
        self.h /= scale;
    }
}

impl std::fmt::Debug for Rectangle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {}, {})", self.x, self.y, self.w, self.h)
    }
}

impl From<parley::Rect> for Rectangle {
    fn from(rect: parley::Rect) -> Self {
        Self::new(rect.x0 as f32, rect.y0 as f32, rect.width() as f32, rect.height() as f32)
    }
}
