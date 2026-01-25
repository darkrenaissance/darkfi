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
    gfx::{gfxtag, DrawMesh, Renderer, Vertex},
    mesh::{Color, MeshBuilder},
};

const X_COLOR: Color = [1.0, 0.0, 0.0, 1.0];

pub fn make_x(renderer: &Renderer, font_size: f32) -> DrawMesh {
    let x_size = font_size * 0.8;
    let half_size = x_size / 2.0;
    let thickness = 2.0;
    let half_thick = thickness / 2.0;

    let mut mesh = MeshBuilder::new(gfxtag!("menu_x"));

    // First diagonal line (top-left to bottom-right)
    // Diagonal from (-half_size, -half_size) to (half_size, half_size)
    // Normal vector is (1, -1) normalized
    let mut diag_start = [-half_size - half_thick, -half_size + half_thick];
    let mut diag_end = [half_size - half_thick, half_size + half_thick];
    let mut diag_start2 = [-half_size + half_thick, -half_size - half_thick];
    let mut diag_end2 = [half_size + half_thick, half_size - half_thick];

    let verts1 = vec![
        Vertex { pos: diag_start, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_start2, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end2, color: X_COLOR, uv: [0., 0.] },
    ];
    mesh.append(verts1, vec![0, 2, 1, 1, 2, 3]);

    // Second diagonal line (bottom-left to top-right)
    // Diagonal from (-half_size, half_size) to (half_size, -half_size)
    diag_start[0] *= -1.;
    diag_end[0] *= -1.;
    diag_start2[0] *= -1.;
    diag_end2[0] *= -1.;

    let verts2 = vec![
        Vertex { pos: diag_start, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_start2, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end2, color: X_COLOR, uv: [0., 0.] },
    ];
    mesh.append(verts2, vec![0, 2, 1, 1, 2, 3]);

    mesh.alloc(renderer).draw_untextured()
}

pub fn make_hammy(renderer: &Renderer, font_size: f32) -> DrawMesh {
    let x_size = font_size * 0.8;
    let half_size = x_size / 2.0;
    let thickness = 2.0;
    let half_thick = thickness / 2.0;

    let mut mesh = MeshBuilder::new(gfxtag!("menu_x"));

    // First diagonal line (top-left to bottom-right)
    // Diagonal from (-half_size, -half_size) to (half_size, half_size)
    // Normal vector is (1, -1) normalized
    let mut diag_start = [-half_size - half_thick, -half_size + half_thick];
    let mut diag_end = [half_size - half_thick, half_size + half_thick];
    let mut diag_start2 = [-half_size + half_thick, -half_size - half_thick];
    let mut diag_end2 = [half_size + half_thick, half_size - half_thick];

    let verts1 = vec![
        Vertex { pos: diag_start, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_start2, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end2, color: X_COLOR, uv: [0., 0.] },
    ];
    mesh.append(verts1, vec![0, 2, 1, 1, 2, 3]);

    // Second diagonal line (bottom-left to top-right)
    // Diagonal from (-half_size, half_size) to (half_size, -half_size)
    diag_start[0] *= -1.;
    diag_end[0] *= -1.;
    diag_start2[0] *= -1.;
    diag_end2[0] *= -1.;

    let verts2 = vec![
        Vertex { pos: diag_start, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_start2, color: X_COLOR, uv: [0., 0.] },
        Vertex { pos: diag_end2, color: X_COLOR, uv: [0., 0.] },
    ];
    mesh.append(verts2, vec![0, 2, 1, 1, 2, 3]);

    mesh.alloc(renderer).draw_untextured()
}
