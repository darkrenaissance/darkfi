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
pub fn create_send_arrow() -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.763722, -0.082607, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.137017, 0.190169, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.992481, -0.087373, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.137017, -0.368093, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.934894, -0.730526, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.89359, 0.560546, [0., 1., 1., 1.]),
        ],
        indices: vec![0, 1, 3, 3, 1, 2, 0, 3, 4, 1, 0, 5],
    }
}
