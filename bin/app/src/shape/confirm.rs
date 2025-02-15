use crate::{
    mesh::Color,
    ui::{ShapeVertex, VectorShape},
};
pub fn create_confirm(color: Color) -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.52, 0.6, color),
            ShapeVertex::from_xy(-0.52, -0.6, color),
            ShapeVertex::from_xy(0.5, 0.0, color),
            ShapeVertex::from_xy(-1.05, 1.5, color),
            ShapeVertex::from_xy(-1.05, -1.5, color),
            ShapeVertex::from_xy(1.5, 0.0, color),
            ShapeVertex::from_xy(-0.88, 1.2, color),
            ShapeVertex::from_xy(-0.88, -1.2, color),
            ShapeVertex::from_xy(1.16, 0.0, color),
        ],
        indices: vec![0, 2, 1, 5, 6, 3, 5, 7, 8, 3, 7, 4, 5, 8, 6, 5, 4, 7, 3, 6, 7],
    }
}
