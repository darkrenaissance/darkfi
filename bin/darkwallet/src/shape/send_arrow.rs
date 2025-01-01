use crate::ui::{ShapeVertex, VectorShape};
pub fn create_send_arrow() -> VectorShape {
    VectorShape {
        verts: vec![
            ShapeVertex::from_xy(-0.763722, -0.082607, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.137017, 0.190169, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(0.992481, -0.087373, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.137017, -0.368093, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.934894, -0.730526, [0., 1., 1., 1.]),
            ShapeVertex::from_xy(-0.89359, 0.560546, [0., 1., 1., 1.]),
        ],
        indices: vec![0, 1, 3, 3, 1, 2, 0, 3, 4, 1, 0, 5],
    }
}
