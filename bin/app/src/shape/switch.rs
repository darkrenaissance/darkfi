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
    mesh::Color,
    ui::{ShapeVertex, VectorShape},
};
pub fn create_switch(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-1.1, -0.5, color),
            ShapeVertex::from_xy(1.7, -0.2, color),
            ShapeVertex::from_xy(-1.7, -0.2, color),
            ShapeVertex::from_xy(0.9, -0.5, color),
            ShapeVertex::from_xy(0.3, -1.4, color),
            ShapeVertex::from_xy(1.1, 0.5, color),
            ShapeVertex::from_xy(-1.7, 0.2, color),
            ShapeVertex::from_xy(1.7, 0.2, color),
            ShapeVertex::from_xy(-0.9, 0.5, color),
            ShapeVertex::from_xy(-0.3, 1.4, color),
        ],
        indices: vec![3, 2, 1, 8, 7, 6, 1, 4, 3, 3, 0, 2, 8, 5, 7, 6, 9, 8],
    }
}
