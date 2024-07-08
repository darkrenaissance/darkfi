use rand::{rngs::OsRng, Rng};
use std::sync::Arc;

use crate::{
    error::Result,
    gfx2::{
        DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, Point, Rectangle,
        RenderApi, RenderApiPtr, Vertex,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, RenderedAtlas, SpritePtr, TextShaper, TextShaperPtr},
    util::zip3,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type ChatViewPtr = Arc<ChatView>;

pub struct ChatView {
    dc_key: u64,
}

impl ChatView {
    pub async fn new() -> Pimpl {
        let self_ = Arc::new(Self { dc_key: OsRng.gen() });

        Pimpl::ChatView(self_)
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(self.dc_key, DrawCall { instrs: vec![], dcs: vec![], z_index: 0 })],
        })
    }
}
