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

use crate::{gfx::Point, mesh::Color, ui::VectorShape};

pub fn create_x(center: Point, width: f32, thickness: f32, color: Color) -> VectorShape {
    let mut shape = VectorShape::new();
    shape.add_line(
        Point::new(center.x - width, center.y - width),
        Point::new(center.x + width, center.y + width),
        thickness,
        color.clone(),
    );
    shape.add_line(
        Point::new(center.x + width, center.y - width),
        Point::new(center.x - width, center.y + width),
        thickness,
        color,
    );
    shape
}
