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
    gfx::{gfxtag, DrawMesh, Point, Renderer, Vertex},
    mesh::{Color, MeshBuilder, COLOR_GREEN},
};

const X_COLOR: Color = [0.9, 0.2, 0.34, 1.0];
const HAM_COLOR: Color = [0., 0.94, 1., 1.0];
const THICKNESS: f32 = 1.;

pub fn make_x(renderer: &Renderer, font_size: f32) -> DrawMesh {
    let x_size = font_size * 0.8;
    let half_size = x_size / 2.0;

    let mut mesh = MeshBuilder::new(gfxtag!("menu_x"));

    // First diagonal line (top-left to bottom-right)
    // Diagonal from (-half_size, -half_size) to (half_size, half_size)
    // Normal vector is (1, -1) normalized
    let mut diag_start = Point::new(-half_size, -half_size);
    let mut diag_end = Point::new(half_size, half_size);

    mesh.draw_line(diag_start, diag_end, X_COLOR, THICKNESS);

    // Second diagonal line (bottom-left to top-right)
    // Diagonal from (-half_size, half_size) to (half_size, -half_size)
    diag_start.x *= -1.;
    diag_end.x *= -1.;

    mesh.draw_line(diag_start, diag_end, X_COLOR, THICKNESS);

    mesh.alloc(renderer).draw_untextured()
}

pub fn make_hammy(renderer: &Renderer, font_size: f32) -> DrawMesh {
    let ham_size = font_size * 0.6;
    let gap_size = font_size * 0.2;

    let mut mesh = MeshBuilder::new(gfxtag!("menu_x"));

    for i in 0..2 {
        for j in -1..2 {
            let center = Point::new(gap_size * i as f32, gap_size * j as f32);
            let lhs = center - Point::new(ham_size, 0.);
            let rhs = center + Point::new(ham_size, 0.);
            let top = center - Point::new(0., ham_size);
            let bot = center + Point::new(0., ham_size);

            mesh.draw_line(lhs, top, HAM_COLOR, THICKNESS);
            mesh.draw_line(top, rhs, HAM_COLOR, THICKNESS);
            mesh.draw_line(rhs, bot, HAM_COLOR, THICKNESS);
            mesh.draw_line(bot, lhs, HAM_COLOR, THICKNESS);
        }
    }

    mesh.alloc(renderer).draw_untextured()
}
