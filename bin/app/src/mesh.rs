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
    error::Result,
    gfx::{GfxDrawMesh, ManagedBufferPtr, ManagedTexturePtr, Point, Rectangle, RenderApi, Vertex},
};

pub type Color = [f32; 4];

#[allow(dead_code)]
pub const COLOR_RED: Color = [1., 0., 0., 1.];
#[allow(dead_code)]
pub const COLOR_DARKGREY: Color = [0.2, 0.2, 0.2, 1.];
#[allow(dead_code)]
pub const COLOR_LIGHTGREY: Color = [0.7, 0.7, 0.7, 1.];
pub const COLOR_GREEN: Color = [0., 1., 0., 1.];
pub const COLOR_BLUE: Color = [0., 0., 1., 1.];
pub const COLOR_PINK: Color = [0.8, 0.3, 0.8, 1.];
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
    /// Convenience method
    pub fn draw_with_texture(self, texture: ManagedTexturePtr) -> GfxDrawMesh {
        GfxDrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            texture: Some(texture),
            num_elements: self.num_elements,
        }
    }
    /// Convenience method
    pub fn draw_untextured(self) -> GfxDrawMesh {
        GfxDrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            texture: None,
            num_elements: self.num_elements,
        }
    }
}

pub struct MeshBuilder {
    pub verts: Vec<Vertex>,
    pub indices: Vec<u16>,
    clipper: Option<Rectangle>,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self { verts: vec![], indices: vec![], clipper: None }
    }
    pub fn with_clip(clipper: Rectangle) -> Self {
        Self { verts: vec![], indices: vec![], clipper: Some(clipper) }
    }

    pub fn append(&mut self, mut verts: Vec<Vertex>, indices: Vec<u16>) {
        let mut indices = indices.into_iter().map(|i| i + self.verts.len() as u16).collect();
        self.verts.append(&mut verts);
        self.indices.append(&mut indices);
    }

    pub fn draw_box(&mut self, obj: &Rectangle, color: Color, uv: &Rectangle) {
        let clipped = match &self.clipper {
            Some(clipper) => {
                let Some(clipped) = clipper.clip(&obj) else { return };
                clipped
            }
            None => obj.clone(),
        };

        let (x1, y1) = clipped.top_left().unpack();
        let (x2, y2) = clipped.bottom_right().unpack();

        let (u1, v1) = uv.top_left().unpack();
        let (u2, v2) = uv.bottom_right().unpack();

        // Interpolate UV coords

        let i = (clipped.x - obj.x) / obj.w;
        let clip_u1 = u1 + i * (u2 - u1);

        let i = (clipped.rhs() - obj.x) / obj.w;
        let clip_u2 = u1 + i * (u2 - u1);

        let i = (clipped.y - obj.y) / obj.h;
        let clip_v1 = v1 + i * (v2 - v1);

        let i = (clipped.bhs() - obj.y) / obj.h;
        let clip_v2 = v1 + i * (v2 - v1);

        let (u1, u2) = (clip_u1, clip_u2);
        let (v1, v2) = (clip_v1, clip_v2);

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

    pub fn draw_outline(&mut self, obj: &Rectangle, color: Color, thickness: f32) {
        let (x1, y1) = obj.top_left().unpack();
        let (dist_x, dist_y) = (obj.w, obj.h);
        let (x2, y2) = obj.bottom_right().unpack();

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

    pub fn alloc(self, render_api: &RenderApi) -> MeshInfo {
        //debug!(target: "mesh", "allocating {} verts:", self.verts.len());
        //for vert in &self.verts {
        //    debug!(target: "mesh", "  {:?}", vert);
        //}
        let num_elements = self.indices.len() as i32;
        let vertex_buffer = render_api.new_vertex_buffer(self.verts);
        let index_buffer = render_api.new_index_buffer(self.indices);
        MeshInfo { vertex_buffer, index_buffer, num_elements }
    }
}
