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
pub fn create_reset(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.998539, -0.013554, color),
            ShapeVertex::from_xy(-0.943221, 0.31985, color),
            ShapeVertex::from_xy(-0.862653, 0.497547, color),
            ShapeVertex::from_xy(-0.748859, 0.656365, color),
            ShapeVertex::from_xy(-0.627199, 0.772373, color),
            ShapeVertex::from_xy(-0.442289, 0.891786, color),
            ShapeVertex::from_xy(-0.260726, 0.960204, color),
            ShapeVertex::from_xy(-0.109272, 0.988958, color),
            ShapeVertex::from_xy(0.130526, 0.991445, color),
            ShapeVertex::from_xy(0.321439, 0.94693, color),
            ShapeVertex::from_xy(0.5, 0.866025, color),
            ShapeVertex::from_xy(0.659346, 0.75184, color),
            ShapeVertex::from_xy(0.793353, 0.608762, color),
            ShapeVertex::from_xy(0.896873, 0.442289, color),
            ShapeVertex::from_xy(0.965926, 0.258819, color),
            ShapeVertex::from_xy(0.997859, 0.065403, color),
            ShapeVertex::from_xy(0.991445, -0.130526, color),
            ShapeVertex::from_xy(0.94693, -0.321439, color),
            ShapeVertex::from_xy(0.866025, -0.5, color),
            ShapeVertex::from_xy(0.75184, -0.659346, color),
            ShapeVertex::from_xy(0.608762, -0.793353, color),
            ShapeVertex::from_xy(0.442289, -0.896873, color),
            ShapeVertex::from_xy(0.258819, -0.965926, color),
            ShapeVertex::from_xy(0.065403, -0.997859, color),
            ShapeVertex::from_xy(-0.130526, -0.991445, color),
            ShapeVertex::from_xy(-0.321439, -0.94693, color),
            ShapeVertex::from_xy(-0.5, -0.866025, color),
            ShapeVertex::from_xy(-0.656697, -0.75131, color),
            ShapeVertex::from_xy(-0.788055, -0.608762, color),
            ShapeVertex::from_xy(-0.892104, -0.442289, color),
            ShapeVertex::from_xy(-0.795385, -0.449426, color),
            ShapeVertex::from_xy(-0.671606, -0.580689, color),
            ShapeVertex::from_xy(-0.4817, -0.634431, color),
            ShapeVertex::from_xy(-0.4, -0.69282, color),
            ShapeVertex::from_xy(-0.257151, -0.757544, color),
            ShapeVertex::from_xy(-0.104421, -0.793156, color),
            ShapeVertex::from_xy(0.052323, -0.798287, color),
            ShapeVertex::from_xy(0.207055, -0.772741, color),
            ShapeVertex::from_xy(0.353831, -0.717498, color),
            ShapeVertex::from_xy(0.487009, -0.634683, color),
            ShapeVertex::from_xy(0.601472, -0.527477, color),
            ShapeVertex::from_xy(0.69282, -0.4, color),
            ShapeVertex::from_xy(0.757544, -0.257151, color),
            ShapeVertex::from_xy(0.793156, -0.104421, color),
            ShapeVertex::from_xy(0.798287, 0.052323, color),
            ShapeVertex::from_xy(0.772741, 0.207055, color),
            ShapeVertex::from_xy(0.717498, 0.353831, color),
            ShapeVertex::from_xy(0.634683, 0.487009, color),
            ShapeVertex::from_xy(0.527477, 0.601472, color),
            ShapeVertex::from_xy(0.4, 0.69282, color),
            ShapeVertex::from_xy(0.257152, 0.757544, color),
            ShapeVertex::from_xy(0.104421, 0.793156, color),
            ShapeVertex::from_xy(-0.163584, 0.793582, color),
            ShapeVertex::from_xy(-0.308589, 0.771626, color),
            ShapeVertex::from_xy(-0.457404, 0.716377, color),
            ShapeVertex::from_xy(-0.586841, 0.648433, color),
            ShapeVertex::from_xy(-0.750937, 0.532477, color),
            ShapeVertex::from_xy(-0.861636, 0.42303, color),
            ShapeVertex::from_xy(-0.820416, -0.177486, color),
            ShapeVertex::from_xy(-0.647226, -0.175467, color),
            ShapeVertex::from_xy(0.151993, -0.418944, color),
            ShapeVertex::from_xy(-0.405869, -0.119132, color),
            ShapeVertex::from_xy(-0.951622, -0.287706, color),
            ShapeVertex::from_xy(-0.912832, -0.12241, color),
            ShapeVertex::from_xy(-0.02437, -0.360761, color),
            ShapeVertex::from_xy(-0.69709, 0.713458, color),
            ShapeVertex::from_xy(-0.694715, 0.576998, color),
            ShapeVertex::from_xy(-0.838847, 0.539945, color),
            ShapeVertex::from_xy(-0.832561, 0.44765, color),
            ShapeVertex::from_xy(-0.554577, 0.831641, color),
            ShapeVertex::from_xy(-0.565199, 0.666385, color),
            ShapeVertex::from_xy(-0.382321, 0.923269, color),
            ShapeVertex::from_xy(-0.34969, 0.758206, color),
            ShapeVertex::from_xy(-0.000801, 0.999738, color),
            ShapeVertex::from_xy(-0.001275, 0.800807, color),
            ShapeVertex::from_xy(-0.025913, -0.275497, color),
            ShapeVertex::from_xy(-0.198952, -0.191694, color),
            ShapeVertex::from_xy(-0.180252, -0.356309, color),
        ],
        indices: vec![
            11, 47, 12, 24, 34, 25, 15, 43, 16, 10, 48, 11, 28, 30, 29, 23, 35, 24, 2, 1, 57, 7,
            74, 73, 10, 50, 49, 15, 45, 44, 23, 37, 36, 27, 31, 28, 19, 39, 20, 8, 50, 9, 5, 72,
            71, 21, 37, 22, 13, 45, 14, 18, 40, 19, 20, 38, 21, 27, 33, 32, 4, 70, 69, 17, 41, 18,
            13, 47, 46, 25, 33, 26, 16, 42, 17, 0, 62, 63, 31, 61, 59, 59, 30, 31, 29, 30, 58, 77,
            75, 76, 65, 55, 4, 67, 56, 3, 71, 53, 6, 8, 74, 51, 11, 48, 47, 24, 35, 34, 15, 44, 43,
            10, 49, 48, 28, 31, 30, 23, 36, 35, 7, 52, 74, 10, 9, 50, 15, 14, 45, 23, 22, 37, 27,
            32, 31, 19, 40, 39, 8, 51, 50, 5, 54, 72, 21, 38, 37, 13, 46, 45, 18, 41, 40, 20, 39,
            38, 27, 26, 33, 4, 55, 70, 17, 42, 41, 13, 12, 47, 25, 34, 33, 16, 43, 42, 31, 32, 61,
            59, 58, 30, 77, 61, 32, 58, 63, 62, 62, 29, 58, 65, 66, 55, 67, 68, 56, 71, 72, 53, 8,
            73, 74, 6, 52, 7, 54, 69, 70, 66, 3, 56, 68, 2, 57, 75, 64, 60, 77, 64, 75, 77, 76, 61,
            6, 53, 52, 54, 5, 69, 66, 65, 3, 68, 67, 2,
        ],
    }
}
