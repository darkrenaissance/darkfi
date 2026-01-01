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

use crate::ui::{ShapeVertex, VectorShape};
pub fn create_close_icon() -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, -0.194555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.194555, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.55, -0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.744555, -0.55, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.194555, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.55, -0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.744555, -0.55, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.194555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.55, 0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.744555, 0.55, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.55, 0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.744555, 0.55, [0., 1., 1., 1.]),
        ],
        indices: vec![
            0, 2, 1, 2, 3, 1, 5, 6, 1, 6, 7, 1, 9, 2, 10, 2, 11, 10, 13, 6, 10, 6, 14, 10, 2, 4, 3,
            6, 8, 7, 2, 12, 11, 6, 15, 14,
        ],
    }
}
