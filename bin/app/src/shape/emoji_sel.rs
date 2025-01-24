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

use crate::{
    mesh::Color,
    ui::{ShapeVertex, VectorShape},
};
pub fn create_emoji_selector(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.299137, -0.077869, color),
            ShapeVertex::from_xy(-0.247956, -0.006422, color),
            ShapeVertex::from_xy(-0.258195, -0.252576, color),
            ShapeVertex::from_xy(-0.192966, -0.231704, color),
            ShapeVertex::from_xy(-0.234915, -0.41554, color),
            ShapeVertex::from_xy(-0.164066, -0.441229, color),
            ShapeVertex::from_xy(-0.408315, -0.607404, color),
            ShapeVertex::from_xy(-0.377605, -0.67564, color),
            ShapeVertex::from_xy(-0.67564, -0.615431, color),
            ShapeVertex::from_xy(-0.705138, -0.685273, color),
            ShapeVertex::from_xy(-0.910051, -0.364965, color),
            ShapeVertex::from_xy(-0.983702, -0.390654, color),
            ShapeVertex::from_xy(-0.861884, -0.187551, color),
            ShapeVertex::from_xy(-0.941957, -0.212437, color),
            ShapeVertex::from_xy(-0.816929, -0.085598, color),
            ShapeVertex::from_xy(-0.871313, -0.035826, color),
            ShapeVertex::from_xy(-0.75672, -0.04867, color),
            ShapeVertex::from_xy(-0.809499, -0.004518, color),
            ShapeVertex::from_xy(-0.750298, -0.045459, color),
            ShapeVertex::from_xy(-0.796655, 0.025185, color),
            ShapeVertex::from_xy(-0.342487, -0.045459, color),
            ShapeVertex::from_xy(-0.302946, 0.02358, color),
            ShapeVertex::from_xy(-0.324826, -0.073557, color),
            ShapeVertex::from_xy(-0.285285, -0.004518, color),
            ShapeVertex::from_xy(0.294399, -0.075557, color),
            ShapeVertex::from_xy(0.243218, -0.00411, color),
            ShapeVertex::from_xy(0.253458, -0.250264, color),
            ShapeVertex::from_xy(0.188228, -0.229392, color),
            ShapeVertex::from_xy(0.230177, -0.413228, color),
            ShapeVertex::from_xy(0.159328, -0.438917, color),
            ShapeVertex::from_xy(0.403577, -0.605092, color),
            ShapeVertex::from_xy(0.372867, -0.673328, color),
            ShapeVertex::from_xy(0.670902, -0.613119, color),
            ShapeVertex::from_xy(0.7004, -0.682961, color),
            ShapeVertex::from_xy(0.905313, -0.362653, color),
            ShapeVertex::from_xy(0.978964, -0.388342, color),
            ShapeVertex::from_xy(0.857147, -0.185239, color),
            ShapeVertex::from_xy(0.93722, -0.210125, color),
            ShapeVertex::from_xy(0.812191, -0.083286, color),
            ShapeVertex::from_xy(0.866575, -0.033514, color),
            ShapeVertex::from_xy(0.751983, -0.046358, color),
            ShapeVertex::from_xy(0.804761, -0.002206, color),
            ShapeVertex::from_xy(0.74556, -0.043147, color),
            ShapeVertex::from_xy(0.791917, 0.027497, color),
            ShapeVertex::from_xy(0.337749, -0.043147, color),
            ShapeVertex::from_xy(0.298209, 0.025892, color),
            ShapeVertex::from_xy(0.320088, -0.071245, color),
            ShapeVertex::from_xy(0.280548, -0.002206, color),
            ShapeVertex::from_xy(-0.260481, 0.280672, color),
            ShapeVertex::from_xy(-0.302414, 0.210381, color),
            ShapeVertex::from_xy(-0.312982, 0.409139, color),
            ShapeVertex::from_xy(-0.378212, 0.390579, color),
            ShapeVertex::from_xy(-0.348979, 0.551295, color),
            ShapeVertex::from_xy(-0.423296, 0.571204, color),
            ShapeVertex::from_xy(-0.115467, 0.815986, color),
            ShapeVertex::from_xy(-0.146177, 0.883066, color),
            ShapeVertex::from_xy(0.11371, 0.81361, color),
            ShapeVertex::from_xy(0.144364, 0.885764, color),
            ShapeVertex::from_xy(0.342181, 0.557428, color),
            ShapeVertex::from_xy(0.419043, 0.572681, color),
            ShapeVertex::from_xy(0.31395, 0.417031, color),
            ShapeVertex::from_xy(0.378413, 0.390629, color),
            ShapeVertex::from_xy(0.256172, 0.276612, color),
            ShapeVertex::from_xy(0.299407, 0.207885, color),
            ShapeVertex::from_xy(0.204883, 0.276478, color),
            ShapeVertex::from_xy(0.25376, 0.206681, color),
            ShapeVertex::from_xy(0.192886, 0.245393, color),
            ShapeVertex::from_xy(0.245375, 0.175863, color),
            ShapeVertex::from_xy(-0.196323, 0.24595, color),
            ShapeVertex::from_xy(-0.246267, 0.175755, color),
            ShapeVertex::from_xy(-0.20358, 0.278671, color),
            ShapeVertex::from_xy(-0.259304, 0.205008, color),
            ShapeVertex::from_xy(-0.288443, 0.349093, color),
            ShapeVertex::from_xy(-0.350323, 0.309817, color),
        ],
        indices: vec![
            0, 3, 2, 3, 4, 2, 5, 6, 4, 7, 8, 6, 9, 10, 8, 10, 13, 12, 12, 15, 14, 14, 17, 16, 17,
            18, 16, 18, 21, 20, 20, 23, 22, 23, 0, 22, 24, 27, 26, 27, 28, 26, 29, 30, 28, 31, 32,
            30, 33, 34, 32, 34, 37, 36, 36, 39, 38, 38, 41, 40, 41, 42, 40, 42, 45, 44, 44, 47, 46,
            47, 24, 46, 48, 73, 72, 51, 52, 50, 52, 55, 54, 54, 57, 56, 57, 58, 56, 58, 61, 60, 61,
            62, 60, 62, 65, 64, 65, 66, 64, 66, 69, 68, 68, 71, 70, 71, 48, 70, 72, 51, 50, 0, 1,
            3, 3, 5, 4, 5, 7, 6, 7, 9, 8, 9, 11, 10, 10, 11, 13, 12, 13, 15, 14, 15, 17, 17, 19,
            18, 18, 19, 21, 20, 21, 23, 23, 1, 0, 24, 25, 27, 27, 29, 28, 29, 31, 30, 31, 33, 32,
            33, 35, 34, 34, 35, 37, 36, 37, 39, 38, 39, 41, 41, 43, 42, 42, 43, 45, 44, 45, 47, 47,
            25, 24, 48, 49, 73, 51, 53, 52, 52, 53, 55, 54, 55, 57, 57, 59, 58, 58, 59, 61, 61, 63,
            62, 62, 63, 65, 65, 67, 66, 66, 67, 69, 68, 69, 71, 71, 49, 48, 72, 73, 51,
        ],
    }
}
