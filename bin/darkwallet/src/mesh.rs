use crate::{
    error::Result,
    gfx2::{Point, Rectangle, RenderApi, Vertex},
};
use miniquad::BufferId;

pub type Color = [f32; 4];

pub const COLOR_RED: Color = [1., 0., 0., 1.];
pub const COLOR_DARKGREY: Color = [0.2, 0.2, 0.2, 1.];
pub const COLOR_GREEN: Color = [0., 1., 0., 1.];
pub const COLOR_BLUE: Color = [0., 0., 1., 1.];
pub const COLOR_WHITE: Color = [1., 1., 1., 1.];

#[derive(Clone)]
pub struct MeshInfo {
    pub vertex_buffer: BufferId,
    pub index_buffer: BufferId,
    pub num_elements: i32,
}

pub struct MeshBuilder {
    verts: Vec<Vertex>,
    indices: Vec<u16>,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self { verts: vec![], indices: vec![] }
    }

    pub fn append(&mut self, mut verts: Vec<Vertex>, indices: Vec<u16>) {
        let mut indices = indices.into_iter().map(|i| i + self.verts.len() as u16).collect();
        self.verts.append(&mut verts);
        self.indices.append(&mut indices);
    }

    pub fn draw_box(&mut self, obj: &Rectangle, color: Color, uv: &Rectangle) {
        let (x1, y1) = obj.top_left().unpack();
        let (x2, y2) = obj.bottom_right().unpack();

        let (u1, v1) = uv.top_left().unpack();
        let (u2, v2) = uv.bottom_right().unpack();

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

    pub fn draw_outline(&mut self, obj: &Rectangle, color: Color, thickness: f32) {
        let uv = Rectangle { x: 0., y: 0., w: 0., h: 0. };

        let (x1, y1) = obj.top_left().unpack();
        let (dist_x, dist_y) = (obj.w, obj.h);
        let (x2, y2) = obj.bottom_right().unpack();

        // top
        self.draw_box(&Rectangle::new(x1, y1, dist_x, thickness), color, &uv);
        // left
        self.draw_box(&Rectangle::new(x1, y1, thickness, dist_y), color, &uv);
        // right
        self.draw_box(&Rectangle::new(x2 - thickness, y1, thickness, dist_y), color, &uv);
        // bottom
        self.draw_box(&Rectangle::new(x1, y2 - thickness, dist_x, thickness), color, &uv);
    }

    pub async fn alloc(self, render_api: &RenderApi) -> Result<MeshInfo> {
        //debug!(target: "mesh", "allocating {} verts:", self.verts.len());
        //for vert in &self.verts {
        //    debug!(target: "mesh", "  {:?}", vert);
        //}
        let num_elements = self.indices.len() as i32;
        let vertex_buffer = render_api.new_vertex_buffer(self.verts).await?;
        let index_buffer = render_api.new_index_buffer(self.indices).await?;
        Ok(MeshInfo { vertex_buffer, index_buffer, num_elements })
    }
}
