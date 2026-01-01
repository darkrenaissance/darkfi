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
pub fn create_circle(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(0.0, -1.0, color),
            ShapeVertex::from_xy(-0.19509, -0.980785, color),
            ShapeVertex::from_xy(-0.382683, -0.92388, color),
            ShapeVertex::from_xy(-0.55557, -0.83147, color),
            ShapeVertex::from_xy(-0.707107, -0.707107, color),
            ShapeVertex::from_xy(-0.83147, -0.55557, color),
            ShapeVertex::from_xy(-0.92388, -0.382683, color),
            ShapeVertex::from_xy(-0.980785, -0.19509, color),
            ShapeVertex::from_xy(-1.0, 0.0, color),
            ShapeVertex::from_xy(-0.980785, 0.19509, color),
            ShapeVertex::from_xy(-0.92388, 0.382683, color),
            ShapeVertex::from_xy(-0.83147, 0.55557, color),
            ShapeVertex::from_xy(-0.707107, 0.707107, color),
            ShapeVertex::from_xy(-0.55557, 0.83147, color),
            ShapeVertex::from_xy(-0.382683, 0.92388, color),
            ShapeVertex::from_xy(-0.19509, 0.980785, color),
            ShapeVertex::from_xy(0.0, 1.0, color),
            ShapeVertex::from_xy(0.19509, 0.980785, color),
            ShapeVertex::from_xy(0.382683, 0.92388, color),
            ShapeVertex::from_xy(0.55557, 0.83147, color),
            ShapeVertex::from_xy(0.707107, 0.707107, color),
            ShapeVertex::from_xy(0.83147, 0.55557, color),
            ShapeVertex::from_xy(0.92388, 0.382683, color),
            ShapeVertex::from_xy(0.980785, 0.19509, color),
            ShapeVertex::from_xy(1.0, 0.0, color),
            ShapeVertex::from_xy(0.980785, -0.19509, color),
            ShapeVertex::from_xy(0.92388, -0.382683, color),
            ShapeVertex::from_xy(0.83147, -0.55557, color),
            ShapeVertex::from_xy(0.707107, -0.707107, color),
            ShapeVertex::from_xy(0.55557, -0.83147, color),
            ShapeVertex::from_xy(0.382683, -0.92388, color),
            ShapeVertex::from_xy(0.19509, -0.980785, color),
        ],
        indices: vec![
            16, 24, 0, 0, 1, 4, 1, 2, 4, 2, 3, 4, 4, 5, 6, 6, 7, 4, 7, 8, 4, 8, 9, 12, 9, 10, 12,
            10, 11, 12, 12, 13, 14, 14, 15, 12, 15, 16, 12, 16, 17, 20, 17, 18, 20, 18, 19, 20, 20,
            21, 22, 22, 23, 20, 23, 24, 20, 24, 25, 28, 25, 26, 28, 26, 27, 28, 28, 29, 30, 30, 31,
            28, 31, 0, 28, 0, 4, 8, 8, 12, 16, 16, 20, 24, 24, 28, 0, 0, 8, 16,
        ],
    }
}
