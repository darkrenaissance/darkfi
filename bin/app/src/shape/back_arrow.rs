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

use crate::ui::{ShapeVertex, VectorShape};
pub fn create_back_arrow() -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.877643, -0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.992314, -0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.993081, 0.000168, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.154072, -0.752301, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.198105, -0.794808, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.877643, 0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.992314, 0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.993081, -0.000168, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.154072, 0.752301, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.198105, 0.794808, [0., 1., 1., 1.]),
        ],
        indices: vec![0, 4, 2, 1, 5, 0, 0, 5, 7, 5, 9, 7, 0, 3, 4, 1, 6, 5, 5, 8, 9],
    }
}
