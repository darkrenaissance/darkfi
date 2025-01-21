use crate::ui::{ShapeVertex, VectorShape};
pub fn create_back_arrow() -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.877643, -0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.992314, -0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.993081, 0.000168, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.154072, -0.752301, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.198105, -0.794808, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.877643, 0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.992314, 0.03111, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.993081, -0.000168, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.154072, 0.752301, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.198105, 0.794808, [0., 1., 1., 1.]),
        ],
        indices: vec![0, 4, 2, 1, 5, 0, 0, 5, 7, 5, 9, 7, 0, 3, 4, 1, 6, 5, 5, 8, 9],
    }
}
