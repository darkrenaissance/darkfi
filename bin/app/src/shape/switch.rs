use crate::{
    mesh::Color,
    ui::{ShapeVertex, VectorShape},
};
pub fn create_switch(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-1.1, -0.5, color),
            ShapeVertex::from_xy(1.7, -0.2, color),
            ShapeVertex::from_xy(-1.7, -0.2, color),
            ShapeVertex::from_xy(0.9, -0.5, color),
            ShapeVertex::from_xy(0.3, -1.4, color),
            ShapeVertex::from_xy(1.1, 0.5, color),
            ShapeVertex::from_xy(-1.7, 0.2, color),
            ShapeVertex::from_xy(1.7, 0.2, color),
            ShapeVertex::from_xy(-0.9, 0.5, color),
            ShapeVertex::from_xy(-0.3, 1.4, color),
        ],
        indices: vec![3, 2, 1, 8, 7, 6, 1, 4, 3, 3, 0, 2, 8, 5, 7, 6, 9, 8],
    }
}
