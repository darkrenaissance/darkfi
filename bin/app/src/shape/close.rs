use crate::ui::{ShapeVertex, VectorShape};
pub fn create_close_icon() -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, -0.194555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.194555, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.55, -0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.744555, -0.55, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.194555, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.55, -0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.744555, -0.55, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.194555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.55, 0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.744555, 0.55, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.0, 0.0, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.55, 0.744555, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.744555, 0.55, [0., 1., 1., 1.]),
        ],
        indices: vec![
            0, 2, 1, 2, 3, 1, 5, 6, 1, 6, 7, 1, 9, 2, 10, 2, 11, 10, 13, 6, 10, 6, 14, 10, 2, 4, 3,
            6, 8, 7, 2, 12, 11, 6, 15, 14,
        ],
    }
}
