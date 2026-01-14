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
    error::Result,
    expr::{Op, SExprCode, SExprMachine, SExprVal},
    gfx::Vertex,
    mesh::Color,
};

#[derive(Debug)]
pub struct ShapeVertex {
    x: SExprCode,
    y: SExprCode,
    color: Color,
}

impl ShapeVertex {
    pub fn new(x: SExprCode, y: SExprCode, color: Color) -> Self {
        Self { x, y, color }
    }

    pub fn from_xy(x: f32, y: f32, color: Color) -> Self {
        Self { x: vec![Op::ConstFloat32(x)], y: vec![Op::ConstFloat32(y)], color }
    }

    pub fn scale(mut self, scale: f32) -> Self {
        let last_x = self.x.pop().unwrap();
        let last_y = self.y.pop().unwrap();
        let mut x = self.x;
        x.push(Op::Mul((Box::new(Op::ConstFloat32(scale)), Box::new(last_x))));
        let mut y = self.y;
        y.push(Op::Mul((Box::new(Op::ConstFloat32(scale)), Box::new(last_y))));
        Self { x, y, color: self.color }
    }
}

// This is bullshit. We need something in expr to support joining exprs somehow. Subroutines.
// Hacky temp workaround for now.
// s-expr surgery
fn sexpr_add(mut x: SExprCode, op: Op) -> Option<SExprCode> {
    let eqn = x.pop()?;
    x.push(Op::Add((Box::new(eqn), Box::new(op))));
    Some(x)
}
fn sexpr_mul(mut x: SExprCode, op: Op) -> Option<Op> {
    let eqn = x.pop()?;
    Some(Op::Mul((Box::new(eqn), Box::new(op))))
}

#[derive(Debug)]
pub struct VectorShape {
    pub verts: Vec<ShapeVertex>,
    pub indices: Vec<u16>,
}

impl VectorShape {
    pub fn new() -> Self {
        Self { verts: vec![], indices: vec![] }
    }

    pub fn eval(&self, w: f32, h: f32) -> Result<Vec<Vertex>> {
        let mut verts = vec![];
        for shape_vert in &self.verts {
            let mut pos = [0.; 2];
            for (i, shape_x) in [(0, &shape_vert.x), (1, &shape_vert.y)] {
                let mut machine = SExprMachine {
                    globals: vec![
                        ("w".to_string(), SExprVal::Float32(w)),
                        ("h".to_string(), SExprVal::Float32(h)),
                    ],
                    stmts: shape_x,
                };
                pos[i] = machine.call()?.as_f32()?;
            }

            let vert = Vertex { pos, color: shape_vert.color.clone(), uv: [0., 0.] };
            verts.push(vert);
        }
        Ok(verts)
    }

    pub fn join(&mut self, mut other: Self) {
        let off = self.verts.len() as u16;
        for index in &mut other.indices {
            *index += off;
        }
        self.verts.append(&mut other.verts);
        self.indices.append(&mut other.indices);
    }

    pub fn add_filled_box(
        &mut self,
        x1: SExprCode,
        y1: SExprCode,
        x2: SExprCode,
        y2: SExprCode,
        color: Color,
    ) {
        self.add_gradient_box(x1, y1, x2, y2, [color.clone(), color.clone(), color.clone(), color])
    }

    /// Colors go clockwise from top-left
    pub fn add_gradient_box(
        &mut self,
        x1: SExprCode,
        y1: SExprCode,
        x2: SExprCode,
        y2: SExprCode,
        color: [Color; 4],
    ) {
        let mut verts = vec![
            ShapeVertex::new(x1.clone(), y1.clone(), color[0]),
            ShapeVertex::new(x2.clone(), y1.clone(), color[1]),
            ShapeVertex::new(x1.clone(), y2.clone(), color[3]),
            ShapeVertex::new(x2, y2, color[2]),
        ];
        let i = self.verts.len() as u16;
        let mut indices = vec![i + 0, i + 2, i + 1, i + 1, i + 2, i + 3];
        self.verts.append(&mut verts);
        self.indices.append(&mut indices);
    }

    pub fn add_outline(
        &mut self,
        x1: SExprCode,
        y1: SExprCode,
        x2: SExprCode,
        y2: SExprCode,
        border_px: f32,
        color: Color,
    ) {
        // LHS
        self.add_filled_box(
            x1.clone(),
            y1.clone(),
            sexpr_add(x1.clone(), Op::ConstFloat32(border_px)).unwrap(),
            y2.clone(),
            color.clone(),
        );
        // THS
        self.add_filled_box(
            x1.clone(),
            y1.clone(),
            x2.clone(),
            sexpr_add(y1.clone(), Op::ConstFloat32(border_px)).unwrap(),
            color.clone(),
        );
        // RHS
        self.add_filled_box(
            sexpr_add(x2.clone(), Op::ConstFloat32(-border_px)).unwrap(),
            y1.clone(),
            x2.clone(),
            y2.clone(),
            color.clone(),
        );
        // BHS
        self.add_filled_box(
            x1.clone(),
            sexpr_add(y2.clone(), Op::ConstFloat32(-border_px)).unwrap(),
            x2.clone(),
            y2.clone(),
            color.clone(),
        );
    }

    pub fn add_radial_glow(
        &mut self,
        center_x: SExprCode,
        center_y: SExprCode,
        width: SExprCode,
        height: SExprCode,
        segments: usize,
        start_angle: f32,
        end_angle: f32,
        mut color: Color,
    ) {
        // Helper to create an x coordinate on the ellipse: center_x + width * cos_angle
        fn ellipse_x(center_x: &SExprCode, width: &SExprCode, cos_angle: f32) -> SExprCode {
            let width_calc = sexpr_mul(width.clone(), Op::ConstFloat32(cos_angle)).unwrap();
            sexpr_add(center_x.clone(), width_calc).unwrap()
        }

        // Helper to create a y coordinate on the ellipse: center_y + height * sin_angle
        fn ellipse_y(center_y: &SExprCode, height: &SExprCode, sin_angle: f32) -> SExprCode {
            let height_calc = sexpr_mul(height.clone(), Op::ConstFloat32(sin_angle)).unwrap();
            sexpr_add(center_y.clone(), height_calc).unwrap()
        }

        let mut new_verts = vec![];
        let mut new_indices = vec![];
        let base_idx = self.verts.len() as u16;

        // Create center vertex at index 0
        new_verts.push(ShapeVertex::new(center_x.clone(), center_y.clone(), color.clone()));

        color[3] = 0.;
        // Generate vertices along the arc
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start_angle + t * (end_angle - start_angle);
            let x = ellipse_x(&center_x, &width, angle.cos() * 0.5);
            let y = ellipse_y(&center_y, &height, angle.sin() * 0.5);
            new_verts.push(ShapeVertex::new(x, y, color.clone()));
        }

        // Create triangles: center vertex (0) with each pair of adjacent arc vertices
        for i in 0..segments {
            new_indices.push(base_idx); // center
            new_indices.push(base_idx + 1 + i as u16); // current arc vertex
            new_indices.push(base_idx + 2 + i as u16); // next arc vertex
        }

        self.verts.append(&mut new_verts);
        self.indices.append(&mut new_indices);
    }

    pub fn scaled(self, scale: f32) -> Self {
        Self {
            verts: self.verts.into_iter().map(|v| v.scale(scale)).collect(),
            indices: self.indices,
        }
    }
}
