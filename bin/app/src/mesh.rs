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

use crate::gfx::{
    DebugTag, DrawMesh, ManagedBufferPtr, ManagedTexturePtr, Point, Rectangle, RenderApi, Vertex,
};

pub type Color = [f32; 4];

#[allow(dead_code)]
pub const COLOR_RED: Color = [1., 0., 0., 1.];
#[allow(dead_code)]
pub const COLOR_DARKGREY: Color = [0.2, 0.2, 0.2, 1.];
#[allow(dead_code)]
pub const COLOR_LIGHTGREY: Color = [0.7, 0.7, 0.7, 1.];
#[allow(dead_code)]
pub const COLOR_GREEN: Color = [0., 1., 0., 1.];
#[allow(dead_code)]
pub const COLOR_BLUE: Color = [0., 0., 1., 1.];
#[allow(dead_code)]
pub const COLOR_PINK: Color = [0.8, 0.3, 0.8, 1.];
pub const COLOR_CYAN: Color = [0., 1., 1., 1.];
#[allow(dead_code)]
pub const COLOR_PURPLE: Color = [1., 0., 1., 1.];
pub const COLOR_WHITE: Color = [1., 1., 1., 1.];
#[allow(dead_code)]
pub const COLOR_BLACK: Color = [1., 1., 1., 1.];
#[allow(dead_code)]
pub const COLOR_GREY: Color = [0.5, 0.5, 0.5, 1.];

#[derive(Clone)]
pub struct MeshInfo {
    pub vertex_buffer: ManagedBufferPtr,
    pub index_buffer: ManagedBufferPtr,
    pub num_elements: i32,
}

impl MeshInfo {
    /// Convenience method for textured mesh
    pub fn draw_with_textures(self, textures: Vec<ManagedTexturePtr>) -> DrawMesh {
        DrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            textures: Some(textures),
            num_elements: self.num_elements,
        }
    }

    /// Convenience method
    pub fn draw_untextured(self) -> DrawMesh {
        DrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            textures: None,
            num_elements: self.num_elements,
        }
    }
}

pub struct MeshBuilder {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u16>,
    tag: DebugTag,
}

impl MeshBuilder {
    pub fn new(tag: DebugTag) -> Self {
        Self { verts: vec![], indices: vec![], tag }
    }

    pub fn append(&mut self, mut verts: Vec<Vertex>, indices: Vec<u16>) {
        let mut indices = indices.into_iter().map(|i| i + self.verts.len() as u16).collect();
        self.verts.append(&mut verts);
        self.indices.append(&mut indices);
    }

    pub fn draw_box(&mut self, obj: &Rectangle, color: Color, uv: &Rectangle) {
        let (x1, y1) = obj.pos().unpack();
        let (x2, y2) = obj.corner().unpack();

        let (u1, v1) = uv.pos().unpack();
        let (u2, v2) = uv.corner().unpack();

        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color, uv: [u1, v1] },
            // top right
            Vertex { pos: [x2, y1], color, uv: [u2, v1] },
            // bottom left
            Vertex { pos: [x1, y2], color, uv: [u1, v2] },
            // bottom right
            Vertex { pos: [x2, y2], color, uv: [u2, v2] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];

        self.append(verts, indices);
    }

    pub fn draw_filled_box(&mut self, obj: &Rectangle, color: Color) {
        let uv = Rectangle::zero();
        self.draw_box(obj, color, &uv);
    }

    pub fn draw_box_shadow(&mut self, obj: &Rectangle, color: Color, spread: f32) {
        let (x1, y1) = obj.pos().unpack();
        let (x2, y2) = obj.corner().unpack();

        let uv = Rectangle::zero();
        let (u1, v1) = uv.pos().unpack();
        let (u2, v2) = uv.corner().unpack();

        let color2 = [color[0], color[1], color[2], 0.];

        // left
        self.append(
            vec![
                Vertex { pos: [x1, y1], color, uv: [u1, v1] },
                Vertex { pos: [x1 - spread, y1 - spread], color: color2, uv: [u2, v1] },
                Vertex { pos: [x1, y2], color, uv: [u1, v2] },
                Vertex { pos: [x1 - spread, y2 + spread], color: color2, uv: [u2, v2] },
            ],
            vec![0, 2, 1, 1, 2, 3],
        );

        // top
        self.append(
            vec![
                Vertex { pos: [x1, y1], color, uv: [u1, v1] },
                Vertex { pos: [x1 - spread, y1 - spread], color: color2, uv: [u2, v1] },
                Vertex { pos: [x2, y1], color, uv: [u1, v2] },
                Vertex { pos: [x2 + spread, y1 - spread], color: color2, uv: [u2, v2] },
            ],
            vec![0, 2, 1, 1, 2, 3],
        );

        // right
        self.append(
            vec![
                Vertex { pos: [x2, y1], color, uv: [u1, v1] },
                Vertex { pos: [x2 + spread, y1 - spread], color: color2, uv: [u2, v1] },
                Vertex { pos: [x2, y2], color, uv: [u1, v2] },
                Vertex { pos: [x2 + spread, y2 + spread], color: color2, uv: [u2, v2] },
            ],
            vec![0, 2, 1, 1, 2, 3],
        );

        // bottom
        self.append(
            vec![
                Vertex { pos: [x1, y2], color, uv: [u1, v1] },
                Vertex { pos: [x1 - spread, y2 + spread], color: color2, uv: [u2, v1] },
                Vertex { pos: [x2, y2], color, uv: [u1, v2] },
                Vertex { pos: [x2 + spread, y2 + spread], color: color2, uv: [u2, v2] },
            ],
            vec![0, 2, 1, 1, 2, 3],
        );
    }

    pub fn draw_outline(&mut self, obj: &Rectangle, color: Color, thickness: f32) {
        let (x1, y1) = obj.pos().unpack();
        let (dist_x, dist_y) = (obj.w, obj.h);
        let (x2, y2) = obj.corner().unpack();

        // top
        self.draw_filled_box(&Rectangle::new(x1, y1, dist_x, thickness), color);
        // left
        self.draw_filled_box(&Rectangle::new(x1, y1, thickness, dist_y), color);
        // right
        self.draw_filled_box(&Rectangle::new(x2 - thickness, y1, thickness, dist_y), color);
        // bottom
        self.draw_filled_box(&Rectangle::new(x1, y2 - thickness, dist_x, thickness), color);
    }

    pub fn draw_line(&mut self, start: Point, end: Point, color: Color, thickness: f32) {
        let mut dir = end - start;
        dir.normalize();
        let left = dir.perp_left() * (thickness / 2.);
        let right = dir.perp_right() * (thickness / 2.);

        let p1 = start + left;
        let p2 = end + left;
        let p3 = start + right;
        let p4 = end + right;

        let uv = [0., 0.];

        let verts = vec![
            // top left
            Vertex { pos: [p1.x, p1.y], color, uv },
            // top right
            Vertex { pos: [p2.x, p2.y], color, uv },
            // bottom left
            Vertex { pos: [p3.x, p3.y], color, uv },
            // bottom right
            Vertex { pos: [p4.x, p4.y], color, uv },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];

        self.append(verts, indices);
    }

    pub fn alloc<R: RenderApi>(self, renderer: &R) -> MeshInfo {
        //debug!(target: "mesh", "allocating {} verts:", self.verts.len());
        //for vert in &self.verts {
        //    debug!(target: "mesh", "  {:?}", vert);
        //}
        let num_elements = self.indices.len() as i32;
        let vertex_buffer = renderer.new_vertex_buffer(self.verts, self.tag);
        let index_buffer = renderer.new_index_buffer(self.indices, self.tag);
        MeshInfo { vertex_buffer, index_buffer, num_elements }
    }
}
