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

use crate::{
    gfx::Point,
    mesh::Color,
    ui::VectorShape,
};

pub fn create_cross(center: Point, thickness: f32, color: Color) -> VectorShape {
    let mut shape = VectorShape::new();
    shape.add_line(Point::new(center.x - 1., center.y), Point::new(center.x + 1., center.y), thickness, color.clone());
    shape.add_line(Point::new(center.x, center.y - 1.), Point::new(center.x, center.y + 1.), thickness, color);
    shape
}
