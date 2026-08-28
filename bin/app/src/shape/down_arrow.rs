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
    ui::{ShapeVertex, VectorShape},
};

pub fn create_down_arrow(color: Color, thickness: f32) -> VectorShape {
    let verts = vec![
        Point::new(-0., -22.5),
        Point::new(-0.0, 12.),
        Point::new(-15., -7.5),
        Point::new(15., -7.5),
    ];
    let mut shape = VectorShape::new();
    shape.add_line(verts[0], verts[1], thickness, color);
    shape.add_line(verts[1], verts[2], thickness, color);
    shape.add_line(verts[1], verts[3], thickness, color);
    shape
}
