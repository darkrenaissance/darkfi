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

pub fn create_down_bgtab(color1: Color, color2: Color, border_thickness: f32) -> VectorShape {
    let verts = vec![
        Point::new(-0.926801, -0.067501),
        Point::new(0.926801, -0.067501),
        Point::new(-0.56229, -0.885142),
        Point::new(0.56229, -0.885142),
        Point::new(-0.912444, 0.0406),
        Point::new(0.912444, 0.0406),
        Point::new(-0.089009, 0.868252),
        Point::new(0.082253, 0.868252),
    ];
    let mut shape = VectorShape::new();
    shape.add_line(verts[0], verts[2], border_thickness, color2);
    shape.add_line(verts[2], verts[3], border_thickness, color2);
    shape.add_line(verts[3], verts[1], border_thickness, color2);
    shape.add_line(verts[1], verts[5], border_thickness, color2);
    shape.add_line(verts[5], verts[7], border_thickness, color2);
    shape.add_line(verts[7], verts[6], border_thickness, color2);
    shape.add_line(verts[6], verts[4], border_thickness, color2);
    shape.add_line(verts[4], verts[0], border_thickness, color2);
    shape.join(VectorShape {
        verts: verts.iter().map(|v| ShapeVertex::from_xy(v.x, v.y, color1)).collect(),
        indices: vec![1, 2, 0, 0, 5, 1, 4, 7, 5, 1, 3, 2, 0, 4, 5, 4, 6, 7],
    });
    shape
}
