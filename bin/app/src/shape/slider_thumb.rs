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

pub fn create_thumb(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.6, -1.4, color),
            ShapeVertex::from_xy(-0.382683, -0.92388, color),
            ShapeVertex::from_xy(-0.507107, -0.507107, color),
            ShapeVertex::from_xy(-0.92388, -0.382683, color),
            ShapeVertex::from_xy(-1.0, 0.0, color),
            ShapeVertex::from_xy(-0.92388, 0.382683, color),
            ShapeVertex::from_xy(-0.707107, 0.707107, color),
            ShapeVertex::from_xy(-0.382683, 0.92388, color),
            ShapeVertex::from_xy(0.5, 1.3, color),
            ShapeVertex::from_xy(0.382683, 0.92388, color),
            ShapeVertex::from_xy(0.507107, 0.507107, color),
            ShapeVertex::from_xy(0.92388, 0.382683, color),
            ShapeVertex::from_xy(1.0, 0.0, color),
            ShapeVertex::from_xy(0.92388, -0.382683, color),
            ShapeVertex::from_xy(0.707107, -0.707107, color),
            ShapeVertex::from_xy(0.382683, -0.92388, color),
            ShapeVertex::from_xy(0.0, -0.62436, color),
            ShapeVertex::from_xy(-0.238932, -0.576833, color),
            ShapeVertex::from_xy(-0.441489, -0.441489, color),
            ShapeVertex::from_xy(-0.576833, -0.238932, color),
            ShapeVertex::from_xy(-0.62436, 0.0, color),
            ShapeVertex::from_xy(-0.576833, 0.238932, color),
            ShapeVertex::from_xy(-0.441489, 0.441489, color),
            ShapeVertex::from_xy(-0.238932, 0.576833, color),
            ShapeVertex::from_xy(0.0, 0.62436, color),
            ShapeVertex::from_xy(0.238932, 0.576833, color),
            ShapeVertex::from_xy(0.441489, 0.441489, color),
            ShapeVertex::from_xy(0.576833, 0.238932, color),
            ShapeVertex::from_xy(0.62436, 0.0, color),
            ShapeVertex::from_xy(0.576833, -0.238932, color),
            ShapeVertex::from_xy(0.441489, -0.441489, color),
            ShapeVertex::from_xy(0.238932, -0.576833, color),
        ],
        indices: vec![
            10, 25, 26, 2, 19, 3, 10, 27, 11, 3, 20, 4, 11, 28, 12, 4, 21, 5, 12, 29, 13, 5, 22, 6,
            13, 30, 14, 6, 23, 7, 14, 31, 15, 7, 24, 8, 1, 16, 17, 15, 16, 0, 9, 24, 25, 2, 17, 18,
            10, 9, 25, 2, 18, 19, 10, 26, 27, 3, 19, 20, 11, 27, 28, 4, 20, 21, 12, 28, 29, 5, 21,
            22, 13, 29, 30, 6, 22, 23, 14, 30, 31, 7, 23, 24, 1, 0, 16, 15, 31, 16, 9, 8, 24, 2, 1,
            17,
        ],
    }
}
