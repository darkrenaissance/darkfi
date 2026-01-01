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
pub fn create_confirm(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.52, 0.6, color),
            ShapeVertex::from_xy(-0.52, -0.6, color),
            ShapeVertex::from_xy(0.5, 0.0, color),
            ShapeVertex::from_xy(-1.05, 1.5, color),
            ShapeVertex::from_xy(-1.05, -1.5, color),
            ShapeVertex::from_xy(1.5, 0.0, color),
            ShapeVertex::from_xy(-0.88, 1.2, color),
            ShapeVertex::from_xy(-0.88, -1.2, color),
            ShapeVertex::from_xy(1.16, 0.0, color),
        ],
        indices: vec![0, 2, 1, 5, 6, 3, 5, 7, 8, 3, 7, 4, 5, 8, 6, 5, 4, 7, 3, 6, 7],
    }
}
