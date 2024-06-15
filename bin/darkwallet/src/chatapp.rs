use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use std::sync::mpsc;

use crate::{
    error::Result,
    expr::Op,
    gfx::Rectangle,
    prop::{Property, PropertySubType, PropertyType},
    res::{ResourceId, ResourceManager},
    scene::{
        MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType,
    },
};

struct Buffer {
    verts: Vec<u8>,
    faces: Vec<u8>,
    verts_len: u32,
}

impl Buffer {
    pub fn new() -> Self {
        Self { verts: vec![], faces: vec![], verts_len: 0 }
    }

    fn vertex(&mut self, x: f32, y: f32, r: f32, g: f32, b: f32, a: f32, u: f32, v: f32) {
        // xy
        x.encode(&mut self.verts).unwrap();
        y.encode(&mut self.verts).unwrap();
        // rgba
        r.encode(&mut self.verts).unwrap();
        g.encode(&mut self.verts).unwrap();
        b.encode(&mut self.verts).unwrap();
        a.encode(&mut self.verts).unwrap();
        // uv
        u.encode(&mut self.verts).unwrap();
        v.encode(&mut self.verts).unwrap();
        self.verts_len += 1
    }

    fn face(&mut self, i1: u32, i2: u32, i3: u32) {
        i1.encode(&mut self.faces).unwrap();
        i2.encode(&mut self.faces).unwrap();
        i3.encode(&mut self.faces).unwrap();
    }

    pub fn draw_box(&mut self, rect: Rectangle<f32>, color: [f32; 4]) {
        let k = self.verts_len;
        let x = rect.x;
        let y = rect.y;
        let w = rect.w;
        let h = rect.h;
        let r = color[0];
        let g = color[1];
        let b = color[2];
        let a = color[3];

        self.vertex(x, y, r, g, b, a, 0., 0.);
        self.vertex(x + w, y, r, g, b, a, 1., 0.);
        self.vertex(x, y + h, r, g, b, a, 0., 1.);
        self.vertex(x + w, y + h, r, g, b, a, 1., 1.);

        self.face(k, k + 2, k + 1);
        self.face(k + 1, k + 2, k + 3);
    }

    pub fn draw_outline(
        &mut self,
        rect: Rectangle<f32>,
        color: [f32; 4],
        pad: f32,
        layer_w: f32,
        layer_h: f32,
    ) {
        let pad_x = pad / layer_w;
        let pad_y = pad / layer_h;

        let x = rect.x;
        let y = rect.y;
        let w = rect.w;
        let h = rect.h;

        // left
        self.draw_box(Rectangle { x, y, w: pad_x, h }, color);
        // top
        self.draw_box(Rectangle { x, y, w, h: pad_y }, color);
        // right
        let rhs = x + w;
        self.draw_box(Rectangle { x: rhs - pad_x, y, w: pad_x, h }, color);
        // bottom
        let bhs = y + h;
        self.draw_box(Rectangle { x, y: bhs - pad_y, w, h: pad_y }, color);
    }
}

pub fn create_layer(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    let win_id = sg.lookup_node("/window").unwrap().id;

    let node = sg.add_node(name, SceneNodeType::RenderLayer);
    let mut prop = Property::new("is_visible", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Uint32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    node.id
}

fn create_mesh(sg: &mut SceneGraph, name: &str, layer_node_id: SceneNodeId) -> SceneNodeId {
    let node = sg.add_node(name, SceneNodeType::RenderMesh);
    let mut prop = Property::new("data", PropertyType::Buffer, PropertySubType::Null);
    prop.set_array_len(2);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let node_id = node.id;
    sg.link(node_id, layer_node_id).unwrap();
    node_id
}

fn create_text(sg: &mut SceneGraph, name: &str, layer_node_id: SceneNodeId) -> SceneNodeId {
    let node = sg.add_node(name, SceneNodeType::RenderText);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let node_id = node.id;
    sg.link(node_id, layer_node_id).unwrap();
    node_id
}

fn create_editbox(sg: &mut SceneGraph, name: &str, layer_node_id: SceneNodeId) -> SceneNodeId {
    let node = sg.add_node(name, SceneNodeType::EditBox);

    let mut prop = Property::new("is_active", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_pos", PropertyType::Uint32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("hi_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("selected", PropertyType::Uint32, PropertySubType::Color);
    prop.set_array_len(2);
    prop.allow_null_values();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let node_id = node.id;
    sg.link(node_id, layer_node_id).unwrap();
    node_id
}

fn create_chatview(sg: &mut SceneGraph, name: &str, layer_node_id: SceneNodeId) -> SceneNodeId {
    let node = sg.add_node(name, SceneNodeType::ChatView);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let node_id = node.id;

    let mut arg_data = vec![];
    node_id.encode(&mut arg_data).unwrap();
    //let (tx, rx) = mpsc::sync_channel::<Result<Vec<u8>>>(0);
    let response_fn = Box::new(move |result| {
        //tx.send(result).unwrap();
    });
    let win_node = sg.lookup_node_mut("/window").unwrap();
    win_node.call_method("create_chat_view", arg_data, response_fn).unwrap();
    //let _result = rx.recv().unwrap();

    sg.link(node_id, layer_node_id).unwrap();
    node_id
}

pub fn setup(sg: &mut SceneGraph) {
    let layer_node_id = create_layer(sg, "view");
    let node = sg.get_node(layer_node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_u32(0, 0).unwrap();
    prop.set_u32(1, 0).unwrap();
    let code = vec![Op::Float32ToUint32((Box::new(Op::LoadVar("sw".to_string()))))];
    prop.set_expr(2, code).unwrap();
    let code = vec![Op::Float32ToUint32((Box::new(Op::LoadVar("sh".to_string()))))];
    prop.set_expr(3, code).unwrap();

    // Make the black background
    // Maybe we should use a RenderPass for this instead
    let node_id = create_mesh(sg, "bg", layer_node_id);
    let node = sg.get_node(node_id).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(0, 0.).unwrap();
    prop.set_f32(1, 0.).unwrap();
    let code = vec![Op::LoadVar("lw".to_string())];
    prop.set_expr(2, code).unwrap();
    let code = vec![Op::LoadVar("lh".to_string())];
    prop.set_expr(3, code).unwrap();

    let prop = node.get_property("data").unwrap();

    let mut buff = Buffer::new();
    buff.draw_box(Rectangle { x: 0., y: 0., w: 1., h: 1. }, [0.05, 0.05, 0.05, 1.]);
    prop.set_buf(0, buff.verts).unwrap();
    prop.set_buf(1, buff.faces).unwrap();

    // Make the chatedit bg
    let node_id = create_mesh(sg, "chateditbg", layer_node_id);
    let node = sg.get_node(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(0, 140.).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lh".to_string())), Box::new(Op::ConstFloat32(60.))))];
    prop.set_expr(1, code).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lw".to_string())), Box::new(Op::ConstFloat32(140.))))];
    prop.set_expr(2, code).unwrap();
    prop.set_f32(3, 60.).unwrap();

    let prop = node.get_property("data").unwrap();

    let mut buff = Buffer::new();
    buff.draw_box(Rectangle { x: 0., y: 0., w: 1., h: 1. }, [0., 0.13, 0.08, 1.]);
    // FIXME: layer dim is passed here manually!
    // we should just use separate objs
    buff.draw_outline(
        Rectangle { x: 0., y: 0., w: 1., h: 1. },
        [0.22, 0.22, 0.22, 1.],
        1.,
        1000.,
        50.,
    );
    prop.set_buf(0, buff.verts).unwrap();
    prop.set_buf(1, buff.faces).unwrap();

    // Make the nicktext border
    let node_id = create_mesh(sg, "nickbg", layer_node_id);
    let node = sg.get_node(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(0, 0.).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lh".to_string())), Box::new(Op::ConstFloat32(60.))))];
    prop.set_expr(1, code).unwrap();
    prop.set_f32(2, 130.).unwrap();
    prop.set_f32(3, 60.).unwrap();

    let prop = node.get_property("data").unwrap();

    let mut buff = Buffer::new();
    // FIXME: layer dim is passed here manually!
    // we should just use separate objs
    buff.draw_outline(
        Rectangle { x: 0., y: 0., w: 1., h: 1. },
        [0., 0.13, 0.08, 1.],
        1.,
        1000.,
        50.,
    );
    prop.set_buf(0, buff.verts).unwrap();
    prop.set_buf(1, buff.faces).unwrap();

    // Nickname
    let node_id = create_text(sg, "nick", layer_node_id);
    let node = sg.get_node(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(0, 20.).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lh".to_string())), Box::new(Op::ConstFloat32(60.))))];
    prop.set_expr(1, code).unwrap();
    prop.set_f32(2, 120.).unwrap();
    prop.set_f32(3, 60.).unwrap();
    node.set_property_f32("baseline", 40.).unwrap();
    node.set_property_f32("font_size", 20.).unwrap();
    node.set_property_str("text", "anon1").unwrap();
    let prop = node.get_property("color").unwrap();
    prop.set_f32(0, 0.).unwrap();
    prop.set_f32(1, 1.).unwrap();
    prop.set_f32(2, 0.).unwrap();
    prop.set_f32(3, 1.).unwrap();
    node.set_property_u32("z_index", 1).unwrap();

    // Text edit
    let node_id = create_editbox(sg, "editz", layer_node_id);
    let node = sg.get_node(node_id).unwrap();
    node.set_property_bool("is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(0, 150.).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lh".to_string())), Box::new(Op::ConstFloat32(60.))))];
    prop.set_expr(1, code).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lw".to_string())), Box::new(Op::ConstFloat32(120.))))];
    prop.set_expr(2, code).unwrap();
    prop.set_f32(3, 60.).unwrap();
    node.set_property_f32("baseline", 40.).unwrap();
    node.set_property_f32("font_size", 20.).unwrap();
    //node.set_property_str("text", "hello king!😁🍆jelly 🍆1234").unwrap();
    node.set_property_str("text", "hello king! jelly 1234").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(0, 1.).unwrap();
    prop.set_f32(1, 1.).unwrap();
    prop.set_f32(2, 1.).unwrap();
    prop.set_f32(3, 1.).unwrap();
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(0, 1.).unwrap();
    prop.set_f32(1, 0.5).unwrap();
    prop.set_f32(2, 0.5).unwrap();
    prop.set_f32(3, 1.).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    prop.set_f32(0, 1.).unwrap();
    prop.set_f32(1, 1.).unwrap();
    prop.set_f32(2, 1.).unwrap();
    prop.set_f32(3, 0.5).unwrap();
    let prop = node.get_property("selected").unwrap();
    prop.set_null(0).unwrap();
    prop.set_null(1).unwrap();
    node.set_property_u32("z_index", 1).unwrap();

    let node_id = node.id;
    let mut arg_data = vec![];
    node_id.encode(&mut arg_data).unwrap();
    //let (tx, rx) = mpsc::sync_channel::<Result<Vec<u8>>>(0);
    let response_fn = Box::new(move |result| {
        //tx.send(result).unwrap();
    });
    let win_node = sg.lookup_node_mut("/window").unwrap();
    win_node.call_method("create_edit_box", arg_data, response_fn).unwrap();
    //let _result = rx.recv().unwrap();

    // ChatView
    let node_id = create_chatview(sg, "chatty", layer_node_id);
    let node = sg.get_node(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(0, 0.).unwrap();
    prop.set_f32(1, 0.).unwrap();
    let code = vec![Op::LoadVar("lw".to_string())];
    prop.set_expr(2, code).unwrap();
    let code =
        vec![Op::Sub((Box::new(Op::LoadVar("lh".to_string())), Box::new(Op::ConstFloat32(50.))))];
    prop.set_expr(3, code).unwrap();
    node.set_property_u32("z_index", 1).unwrap();

    // On android lets scale the UI up
    let win_node = sg.lookup_node_mut("/window").unwrap();
    win_node.set_property_f32("scale", 1.6).unwrap();

    let layer_node = sg.get_node(layer_node_id).unwrap();
    layer_node.set_property_bool("is_visible", true).unwrap();
}
