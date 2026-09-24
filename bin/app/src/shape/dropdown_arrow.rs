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
pub fn create_dropdown_arrow(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(0.0530483, 0.176771, color),
            ShapeVertex::from_xy(0.5, -0.300827, color),
            ShapeVertex::from_xy(0.122327, 0.0792875, color),
            ShapeVertex::from_xy(0.228135, -2.28135e-06, color),
            ShapeVertex::from_xy(0.31053, -0.0339292, color),
            ShapeVertex::from_xy(0.472996, -0.226559, color),
            ShapeVertex::from_xy(-0.000909804, 0.300827, color),
            ShapeVertex::from_xy(0.0, 0.0221565, color),
            ShapeVertex::from_xy(0.0, 0.242554, color),
            ShapeVertex::from_xy(0.0, 0.189644, color),
            ShapeVertex::from_xy(0.0, 0.136879, color),
            ShapeVertex::from_xy(0.0, 0.0712836, color),
            ShapeVertex::from_xy(0.0146983, 0.25979, color),
            ShapeVertex::from_xy(0.145499, -0.104961, color),
            ShapeVertex::from_xy(0.0321256, 0.166834, color),
            ShapeVertex::from_xy(0.115049, 0.0547958, color),
            ShapeVertex::from_xy(0.0988191, 0.032815, color),
            ShapeVertex::from_xy(0.149904, -0.049154, color),
            ShapeVertex::from_xy(0.269698, -0.134017, color),
            ShapeVertex::from_xy(0.26857, -0.0899332, color),
            ShapeVertex::from_xy(0.410895, -0.132188, color),
            ShapeVertex::from_xy(0.368609, -0.142256, color),
            ShapeVertex::from_xy(0.350214, -0.162341, color),
            ShapeVertex::from_xy(0.0646034, 0.0193792, color),
            ShapeVertex::from_xy(0.088266, -0.0413194, color),
            ShapeVertex::from_xy(0.0, -0.144118, color),
            ShapeVertex::from_xy(0.0406537, -0.0917327, color),
            ShapeVertex::from_xy(0.0, -0.107919, color),
            ShapeVertex::from_xy(0.0433827, -0.175769, color),
            ShapeVertex::from_xy(0.0652148, -0.156854, color),
            ShapeVertex::from_xy(0.0, -0.0344233, color),
            ShapeVertex::from_xy(0.01214, -0.0589844, color),
            ShapeVertex::from_xy(0.0212367, -0.213065, color),
            ShapeVertex::from_xy(0.0, -0.275832, color),
            ShapeVertex::from_xy(0.0988725, -0.135209, color),
            ShapeVertex::from_xy(0.0276039, -0.110951, color),
            ShapeVertex::from_xy(0.0194171, -0.140176, color),
            ShapeVertex::from_xy(-0.0530483, 0.176771, color),
            ShapeVertex::from_xy(-0.5, -0.300827, color),
            ShapeVertex::from_xy(-0.122327, 0.0792875, color),
            ShapeVertex::from_xy(-0.228135, -2.28135e-06, color),
            ShapeVertex::from_xy(-0.31053, -0.0339292, color),
            ShapeVertex::from_xy(-0.472996, -0.226559, color),
            ShapeVertex::from_xy(0.000909804, 0.300827, color),
            ShapeVertex::from_xy(-0.0146983, 0.25979, color),
            ShapeVertex::from_xy(-0.145499, -0.104961, color),
            ShapeVertex::from_xy(-0.0321256, 0.166834, color),
            ShapeVertex::from_xy(-0.115049, 0.0547958, color),
            ShapeVertex::from_xy(-0.0988191, 0.032815, color),
            ShapeVertex::from_xy(-0.149904, -0.049154, color),
            ShapeVertex::from_xy(-0.269698, -0.134017, color),
            ShapeVertex::from_xy(-0.26857, -0.0899332, color),
            ShapeVertex::from_xy(-0.410895, -0.132188, color),
            ShapeVertex::from_xy(-0.368609, -0.142256, color),
            ShapeVertex::from_xy(-0.350214, -0.162341, color),
            ShapeVertex::from_xy(-0.0646034, 0.0193792, color),
            ShapeVertex::from_xy(-0.088266, -0.0413194, color),
            ShapeVertex::from_xy(-0.0406537, -0.0917327, color),
            ShapeVertex::from_xy(-0.0433827, -0.175769, color),
            ShapeVertex::from_xy(-0.0652148, -0.156854, color),
            ShapeVertex::from_xy(-0.01214, -0.0589844, color),
            ShapeVertex::from_xy(-0.0212367, -0.213065, color),
            ShapeVertex::from_xy(-0.0988725, -0.135209, color),
            ShapeVertex::from_xy(-0.0276039, -0.110951, color),
            ShapeVertex::from_xy(-0.0194171, -0.140176, color),
        ],
        indices: vec![
            19, 13, 17, 0, 14, 12, 2, 15, 14, 3, 16, 15, 4, 17, 16, 16, 11, 10, 15, 10, 9, 14, 9,
            8, 12, 8, 6, 23, 7, 11, 4, 21, 19, 18, 21, 22, 22, 5, 1, 21, 20, 5, 24, 17, 13, 23, 16,
            17, 36, 28, 32, 27, 36, 25, 31, 27, 30, 25, 32, 33, 26, 35, 31, 35, 29, 36, 19, 18, 13,
            0, 2, 14, 2, 3, 15, 3, 4, 16, 4, 19, 17, 16, 23, 11, 15, 16, 10, 14, 15, 9, 12, 14, 8,
            23, 24, 7, 4, 20, 21, 18, 19, 21, 22, 21, 5, 24, 23, 17, 36, 29, 28, 27, 35, 36, 31,
            35, 27, 25, 36, 32, 26, 34, 35, 35, 34, 29, 51, 49, 45, 37, 44, 46, 39, 46, 47, 40, 47,
            48, 41, 48, 49, 48, 10, 11, 47, 9, 10, 46, 8, 9, 44, 43, 8, 55, 11, 7, 41, 51, 53, 50,
            54, 53, 54, 38, 42, 53, 42, 52, 56, 45, 49, 55, 49, 48, 64, 61, 58, 27, 25, 64, 60, 30,
            27, 25, 33, 61, 57, 60, 63, 63, 64, 59, 51, 45, 50, 37, 46, 39, 39, 47, 40, 40, 48, 41,
            41, 49, 51, 48, 11, 55, 47, 10, 48, 46, 9, 47, 44, 8, 46, 55, 7, 56, 41, 53, 52, 50,
            53, 51, 54, 42, 53, 56, 49, 55, 64, 58, 59, 27, 64, 63, 60, 27, 63, 25, 61, 64, 57, 63,
            62, 63, 59, 62,
        ],
    }
}
