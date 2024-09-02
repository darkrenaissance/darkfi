/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use sled_overlay::sled;

use crate::{
    darkirc::{DarkIrcBackendPtr, Privmsg},
    error::Error,
    expr::Op,
    gfx::{GraphicsEventPublisherPtr, RenderApiPtr, Vertex, Rectangle},
    prop::{Property, PropertyBool, PropertyStr, PropertySubType, PropertyType, Role},
    scene::{
        CallArgType, MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId,
        SceneNodeType, Slot,
    },
    mesh::{Color, MeshBuilder},
    text::TextShaperPtr,
    ui::{chatview, Button, ChatView, EditBox, Image, Mesh, RenderLayer, Stoppable, Text, Window},
    ExecutorPtr,
};

use super::{App, populate_tree,
node::{create_mesh, create_layer, create_button, create_image, create_text, create_editbox, create_chatview}};

#[cfg(target_os = "android")]
const CHATDB_PATH: &str = "/data/data/darkfi.darkwallet/chatdb/";
#[cfg(target_os = "linux")]
const CHATDB_PATH: &str = "chatdb";

#[cfg(target_os = "android")]
const KING_PATH: &str = "king.png";
#[cfg(target_os = "linux")]
const KING_PATH: &str = "assets/king.png";

const LIGHTMODE: bool = false;

#[cfg(target_os = "android")]
const EDITCHAT_HEIGHT: f32 = 140.;
#[cfg(target_os = "linux")]
const EDITCHAT_HEIGHT: f32 = 50.;

#[cfg(target_os = "android")]
const NICKLABEL_WIDTH: f32 = 300.;
#[cfg(target_os = "linux")]
const NICKLABEL_WIDTH: f32 = 120.;

#[cfg(target_os = "android")]
const FONTSIZE: f32 = 40.;
#[cfg(target_os = "linux")]
const FONTSIZE: f32 = 20.;

pub(super) async fn make_old(
    app: &App
) {
    //let mut tasks = vec![];
    // Create a layer called view
    let mut sg = app.sg.lock().await;
    let layer_node_id = create_layer(&mut sg, "view");

    // Customize our layer
    let node = sg.get_node(layer_node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = vec![Op::LoadVar("h".to_string())];
    prop.set_expr(Role::App, 3, code).unwrap();
    node.set_property_bool(Role::App, "is_visible", true).unwrap();

    // Setup the pimpl
    let node_id = node.id;
    drop(sg);
    let pimpl =
        RenderLayer::new(app.ex.clone(), app.sg.clone(), node_id, app.render_api.clone())
            .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    let window_id = sg.lookup_node("/window").unwrap().id;
    sg.link(node_id, window_id).unwrap();

    // Create a bg mesh
    let node_id = create_mesh(&mut sg, "bg");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = vec![Op::LoadVar("h".to_string())];
    prop.set_expr(Role::App, 3, code).unwrap();

    let c = if LIGHTMODE { 1. } else { 0. };
    // Setup the pimpl
    let node_id = node.id;
    let (x1, y1) = (0., 0.);
    let (x2, y2) = (1., 1.);
    let verts = vec![
        // top left
        Vertex { pos: [x1, y1], color: [c, c, c, 1.], uv: [0., 0.] },
        // top right
        Vertex { pos: [x2, y1], color: [c, c, c, 1.], uv: [1., 0.] },
        // bottom left
        Vertex { pos: [x1, y2], color: [c, c, c, 1.], uv: [0., 1.] },
        // bottom right
        Vertex { pos: [x2, y2], color: [c, c, c, 1.], uv: [1., 1.] },
    ];
    let indices = vec![0, 2, 1, 1, 2, 3];
    drop(sg);
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        verts,
        indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create button bg
    let node_id = create_mesh(&mut sg, "btnbg");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("w".to_string())),
        Box::new(Op::ConstFloat32(220.)),
    ))];
    prop.set_expr(Role::App, 0, code).unwrap();
    prop.set_f32(Role::App, 1, 10.).unwrap();
    prop.set_f32(Role::App, 2, 200.).unwrap();
    prop.set_f32(Role::App, 3, 60.).unwrap();

    // Setup the pimpl
    let (x1, y1) = (0., 0.);
    let (x2, y2) = (1., 1.);
    let verts = if LIGHTMODE {
        vec![
            // top left
            Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [1., 0., 0., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [1., 0., 0., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [1., 0., 0., 1.], uv: [1., 1.] },
        ]
    } else {
        vec![
            // top left
            Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [1., 0., 1., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 0., 1., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [1., 1., 0., 1.], uv: [1., 1.] },
        ]
    };
    let indices = vec![0, 2, 1, 1, 2, 3];
    drop(sg);
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        verts,
        indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create the button
    let node_id = create_button(&mut sg, "btn");

    let node = sg.get_node_mut(node_id).unwrap();
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("w".to_string())),
        Box::new(Op::ConstFloat32(220.)),
    ))];
    prop.set_expr(Role::App, 0, code).unwrap();
    prop.set_f32(Role::App, 1, 10.).unwrap();
    prop.set_f32(Role::App, 2, 200.).unwrap();
    prop.set_f32(Role::App, 3, 60.).unwrap();

    let (sender, btn_click_recvr) = async_channel::unbounded();
    let slot_click = Slot { name: "button_clicked".to_string(), notify: sender };
    node.register("click", slot_click).unwrap();

    drop(sg);
    let pimpl =
        Button::new(app.ex.clone(), app.sg.clone(), node_id, app.event_pub.clone()).await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create another mesh
    let node_id = create_mesh(&mut sg, "box");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 10.).unwrap();
    prop.set_f32(Role::App, 1, 10.).unwrap();
    prop.set_f32(Role::App, 2, 60.).unwrap();
    prop.set_f32(Role::App, 3, 60.).unwrap();

    // Setup the pimpl
    let (x1, y1) = (0., 0.);
    let (x2, y2) = (1., 1.);
    let verts = if LIGHTMODE {
        vec![
            // top left
            Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [1., 0., 0., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [1., 0., 0., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [1., 0., 0., 1.], uv: [1., 1.] },
        ]
    } else {
        vec![
            // top left
            Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [1., 0., 1., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 0., 1., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [1., 1., 0., 1.], uv: [1., 1.] },
        ]
    };
    let indices = vec![0, 2, 1, 1, 2, 3];
    drop(sg);
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        verts,
        indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Debugging tool
    let node_id = create_mesh(&mut sg, "debugtool");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code =
        vec![Op::Div((Box::new(Op::LoadVar("h".to_string())), Box::new(Op::ConstFloat32(2.))))];
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, 5.).unwrap();

    node.set_property_u32(Role::App, "z_index", 2).unwrap();

    // Setup the pimpl
    let (x1, y1) = (0., 0.);
    let (x2, y2) = (1., 1.);
    let verts = vec![
        // top left
        Vertex { pos: [x1, y1], color: [0., 1., 0., 1.], uv: [0., 0.] },
        // top right
        Vertex { pos: [x2, y1], color: [0., 1., 0., 1.], uv: [1., 0.] },
        // bottom left
        Vertex { pos: [x1, y2], color: [0., 1., 0., 1.], uv: [0., 1.] },
        // bottom right
        Vertex { pos: [x2, y2], color: [0., 1., 0., 1.], uv: [1., 1.] },
    ];
    let indices = vec![0, 2, 1, 1, 2, 3];
    drop(sg);
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        verts,
        indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Debugging tool
    let node_id = create_mesh(&mut sg, "debugtool2");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(200.)),
    ))];
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, 5.).unwrap();

    node.set_property_u32(Role::App, "z_index", 2).unwrap();

    // Setup the pimpl
    let (x1, y1) = (0., 0.);
    let (x2, y2) = (1., 1.);
    let verts = vec![
        // top left
        Vertex { pos: [x1, y1], color: [0., 1., 0., 1.], uv: [0., 0.] },
        // top right
        Vertex { pos: [x2, y1], color: [0., 1., 0., 1.], uv: [1., 0.] },
        // bottom left
        Vertex { pos: [x1, y2], color: [0., 1., 0., 1.], uv: [0., 1.] },
        // bottom right
        Vertex { pos: [x2, y2], color: [0., 1., 0., 1.], uv: [1., 1.] },
    ];
    let indices = vec![0, 2, 1, 1, 2, 3];
    drop(sg);
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        verts,
        indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create KING GNU!
    let node_id = create_image(&mut sg, "king");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 80.).unwrap();
    prop.set_f32(Role::App, 1, 10.).unwrap();
    prop.set_f32(Role::App, 2, 60.).unwrap();
    prop.set_f32(Role::App, 3, 60.).unwrap();

    node.set_property_str(Role::App, "path", KING_PATH).unwrap();

    // Setup the pimpl
    drop(sg);
    let pimpl =
        Image::new(app.ex.clone(), app.sg.clone(), node_id, app.render_api.clone()).await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create some text
    let node_id = create_text(&mut sg, "label");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 100.).unwrap();
    prop.set_f32(Role::App, 1, 100.).unwrap();
    prop.set_f32(Role::App, 2, 800.).unwrap();
    prop.set_f32(Role::App, 3, 200.).unwrap();
    node.set_property_f32(Role::App, "baseline", 40.).unwrap();
    node.set_property_f32(Role::App, "font_size", 60.).unwrap();
    node.set_property_str(Role::App, "text", "anon1🍆").unwrap();
    //node.set_property_str(Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 1.).unwrap();
    prop.set_f32(Role::App, 2, 0.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();

    drop(sg);
    let pimpl = Text::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        app.text_shaper.clone(),
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Text edit
    let node_id = create_editbox(&mut sg, "editz");
    let node = sg.get_node(node_id).unwrap();
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 150.).unwrap();
    prop.set_f32(Role::App, 1, 150.).unwrap();
    prop.set_f32(Role::App, 2, 380.).unwrap();
    //let code = vec![Op::Sub((
    //    Box::new(Op::LoadVar("h".to_string())),
    //    Box::new(Op::ConstFloat32(60.)),
    //))];
    //prop.set_expr(Role::App, 1, code).unwrap();
    //let code = vec![Op::Sub((
    //    Box::new(Op::LoadVar("w".to_string())),
    //    Box::new(Op::ConstFloat32(120.)),
    //))];
    //prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, 60.).unwrap();
    node.set_property_f32(Role::App, "baseline", 40.).unwrap();
    node.set_property_f32(Role::App, "font_size", 20.).unwrap();
    node.set_property_f32(Role::App, "font_size", 40.).unwrap();
    node.set_property_str(Role::App, "text", "hello king!😁🍆jelly 🍆1234").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(Role::App, 0, 1.).unwrap();
    prop.set_f32(Role::App, 1, 0.5).unwrap();
    prop.set_f32(Role::App, 2, 0.5).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 0.5).unwrap();
    }
    let prop = node.get_property("selected").unwrap();
    prop.set_null(Role::App, 0).unwrap();
    prop.set_null(Role::App, 1).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    let editbox_text = PropertyStr::wrap(node, Role::App, "text", 0).unwrap();
    let editbox_focus = PropertyBool::wrap(node, Role::App, "is_focused", 0).unwrap();
    //let darkirc_backend = app.darkirc_backend.clone();
    //let task = app.ex.spawn(async move {
    //    while let Ok(_) = btn_click_recvr.recv().await {
    //        let text = editbox_text.get();
    //        editbox_text.prop().unset(Role::App, 0).unwrap();
    //        // Clicking outside the editbox makes it lose focus
    //        // So lets focus it again
    //        editbox_focus.set(true);

    //        debug!(target: "app", "sending text {text}");

    //        let privmsg =
    //            Privmsg { channel: "#random".to_string(), nick: "king".to_string(), msg: text };
    //        darkirc_backend.send(privmsg).await;
    //    }
    //});
    //tasks.push(task);

    drop(sg);
    let pimpl = EditBox::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        app.event_pub.clone(),
        app.text_shaper.clone(),
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // ChatView
    let (node_id, recvr) = create_chatview(&mut sg, "chatty");
    let node = sg.get_node(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code =
        vec![Op::Div((Box::new(Op::LoadVar("h".to_string())), Box::new(Op::ConstFloat32(2.))))];
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::Div((
            Box::new(Op::LoadVar("h".to_string())),
            Box::new(Op::ConstFloat32(2.)),
        ))),
        Box::new(Op::ConstFloat32(200.)),
    ))];
    prop.set_expr(Role::App, 3, code).unwrap();
    node.set_property_f32(Role::App, "font_size", 20.).unwrap();
    node.set_property_f32(Role::App, "line_height", 30.).unwrap();
    node.set_property_f32(Role::App, "baseline", 20.).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    let prop = node.get_property("timestamp_color").unwrap();
    prop.set_f32(Role::App, 0, 0.5).unwrap();
    prop.set_f32(Role::App, 1, 0.5).unwrap();
    prop.set_f32(Role::App, 2, 0.5).unwrap();
    prop.set_f32(Role::App, 3, 0.5).unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }

    let prop = node.get_property("nick_colors").unwrap();
    #[rustfmt::skip]
    let nick_colors = [
        0.00, 0.94, 1.00, 1.,
        0.36, 1.00, 0.69, 1.,
        0.29, 1.00, 0.45, 1.,
        0.00, 0.73, 0.38, 1.,
        0.21, 0.67, 0.67, 1.,
        0.56, 0.61, 1.00, 1.,
        0.84, 0.48, 1.00, 1.,
        1.00, 0.61, 0.94, 1.,
        1.00, 0.36, 0.48, 1.,
        1.00, 0.30, 0.00, 1.
    ];
    for c in nick_colors {
        prop.push_f32(Role::App, c).unwrap();
    }

    drop(sg);
    let db = sled::open(CHATDB_PATH).expect("cannot open sleddb");
    let chat_tree = db.open_tree(b"chat").unwrap();
    //if chat_tree.is_empty() {
    //    populate_tree(&chat_tree);
    //}
    debug!(target: "app", "db has {} lines", chat_tree.len());
    let pimpl = ChatView::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        app.event_pub.clone(),
        app.text_shaper.clone(),
        chat_tree,
        recvr,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // On android lets scale the UI up
    // TODO: add support for fractional scaling
    // This also affects mouse/touch input since coords need to be accurately translated
    // Also we need to think about nesting of layers.
    //let window_node = sg.get_node_mut(window_id).unwrap();
    //win_node.set_property_f32(Role::App, "scale", 1.6).unwrap();

    //*app.tasks.lock().unwrap() = tasks;
}

pub(super) async fn make(
    app: &App
) {
    // Main view
    let mut sg = app.sg.lock().await;
    let layer_node_id = create_layer(&mut sg, "view");
    let node = sg.get_node(layer_node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = vec![Op::LoadVar("h".to_string())];
    prop.set_expr(Role::App, 3, code).unwrap();
    node.set_property_bool(Role::App, "is_visible", true).unwrap();

    let node_id = node.id;
    drop(sg);
    let pimpl =
        RenderLayer::new(app.ex.clone(), app.sg.clone(), node_id, app.render_api.clone())
            .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    let window_id = sg.lookup_node("/window").unwrap().id;
    sg.link(node_id, window_id).unwrap();

    // Create a bg mesh
    let node_id = create_mesh(&mut sg, "bg");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = vec![Op::LoadVar("h".to_string())];
    prop.set_expr(Role::App, 3, code).unwrap();

    let c = if LIGHTMODE { 1. } else { 0.05 };
    // Setup the pimpl
    let node_id = node.id;
    let (x1, y1) = (0., 0.);
    let (x2, y2) = (1., 1.);
    let verts = vec![
        // top left
        Vertex { pos: [x1, y1], color: [c, c, c, 1.], uv: [0., 0.] },
        // top right
        Vertex { pos: [x2, y1], color: [c, c, c, 1.], uv: [1., 0.] },
        // bottom left
        Vertex { pos: [x1, y2], color: [c, c, c, 1.], uv: [0., 1.] },
        // bottom right
        Vertex { pos: [x2, y2], color: [c, c, c, 1.], uv: [1., 1.] },
    ];
    let indices = vec![0, 2, 1, 1, 2, 3];
    drop(sg);
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        verts,
        indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // ChatView
    let (node_id, recvr) = create_chatview(&mut sg, "chatty");
    let node = sg.get_node(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(EDITCHAT_HEIGHT)),
    ))];
    prop.set_expr(Role::App, 3, code).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_f32(Role::App, "line_height", FONTSIZE*1.5).unwrap();
    node.set_property_f32(Role::App, "baseline", 20.).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    let prop = node.get_property("timestamp_color").unwrap();
    prop.set_f32(Role::App, 0, 0.407).unwrap();
    prop.set_f32(Role::App, 1, 0.604).unwrap();
    prop.set_f32(Role::App, 2, 0.647).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }

    let prop = node.get_property("nick_colors").unwrap();
    #[rustfmt::skip]
    let nick_colors = [
        0.00, 0.94, 1.00, 1.,
        0.36, 1.00, 0.69, 1.,
        0.29, 1.00, 0.45, 1.,
        0.00, 0.73, 0.38, 1.,
        0.21, 0.67, 0.67, 1.,
        0.56, 0.61, 1.00, 1.,
        0.84, 0.48, 1.00, 1.,
        1.00, 0.61, 0.94, 1.,
        1.00, 0.36, 0.48, 1.,
        1.00, 0.30, 0.00, 1.
    ];
    for c in nick_colors {
        prop.push_f32(Role::App, c).unwrap();
    }

    drop(sg);
    let db = sled::open(CHATDB_PATH).expect("cannot open sleddb");
    let chat_tree = db.open_tree(b"chat").unwrap();
    if chat_tree.is_empty() {
        populate_tree(&chat_tree);
    }
    debug!(target: "app", "db has {} lines", chat_tree.len());
    let pimpl = ChatView::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        app.event_pub.clone(),
        app.text_shaper.clone(),
        chat_tree,
        recvr,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create the editbox bg
    let node_id = create_mesh(&mut sg, "editbox_bg");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, NICKLABEL_WIDTH).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(EDITCHAT_HEIGHT)),
    ))];
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("w".to_string())),
        Box::new(Op::ConstFloat32(NICKLABEL_WIDTH)),
    ))];
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_u32(Role::App, "z_index", 0).unwrap();
    drop(sg);
    let mut mesh = MeshBuilder::new();
    mesh.draw_box(&Rectangle { x: 0., y: 0., w: 1., h: 1. },
        [0., 0.13, 0.08, 1.], &Rectangle { x: 0., y: 0., w: 1., h: 1. });
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        mesh.verts,
        mesh.indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create the nick - editbox sep
    let node_id = create_mesh(&mut sg, "editbox_lhs_sep");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, NICKLABEL_WIDTH).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(EDITCHAT_HEIGHT)),
    ))];
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();
    drop(sg);
    let mut mesh = MeshBuilder::new();
    mesh.draw_box(&Rectangle { x: 0., y: 0., w: 1., h: 1. },
        [0.4, 0.4, 0.4, 1.], &Rectangle { x: 0., y: 0., w: 1., h: 1. });
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        mesh.verts,
        mesh.indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create the chatview - editbox sep
    let node_id = create_mesh(&mut sg, "chatview_bhs_sep");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(EDITCHAT_HEIGHT)),
    ))];
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = vec![Op::LoadVar("w".to_string())];
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();
    drop(sg);
    let mut mesh = MeshBuilder::new();
    mesh.draw_box(&Rectangle { x: 0., y: 0., w: 1., h: 1. },
        [0.4, 0.4, 0.4, 1.], &Rectangle { x: 0., y: 0., w: 1., h: 1. });
    let pimpl = Mesh::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        mesh.verts,
        mesh.indices,
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Create some text
    let node_id = create_text(&mut sg, "nick_label");

    let node = sg.get_node_mut(node_id).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 20.).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(EDITCHAT_HEIGHT)),
    ))];
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, NICKLABEL_WIDTH).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_f32(Role::App, "baseline", (EDITCHAT_HEIGHT + 20.)/2.).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(Role::App, "text", "anon").unwrap();
    //node.set_property_str(Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(Role::App, 0, 0.365).unwrap();
    prop.set_f32(Role::App, 1, 1.).unwrap();
    prop.set_f32(Role::App, 2, 0.694).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();

    drop(sg);
    let pimpl = Text::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        app.text_shaper.clone(),
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();

    // Text edit
    let node_id = create_editbox(&mut sg, "editz");
    let node = sg.get_node(node_id).unwrap();
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, NICKLABEL_WIDTH + 20.).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("h".to_string())),
        Box::new(Op::ConstFloat32(EDITCHAT_HEIGHT)),
    ))];
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = vec![Op::Sub((
        Box::new(Op::LoadVar("w".to_string())),
        Box::new(Op::ConstFloat32(NICKLABEL_WIDTH + 20.)),
    ))];
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, EDITCHAT_HEIGHT).unwrap();
    node.set_property_f32(Role::App, "baseline", (EDITCHAT_HEIGHT + 20.)/2.).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    //node.set_property_str(Role::App, "text", "hello king!😁🍆jelly 🍆1234").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(Role::App, 0, 0.816).unwrap();
    prop.set_f32(Role::App, 1, 0.627).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 1.).unwrap();
        prop.set_f32(Role::App, 3, 0.5).unwrap();
    }
    let prop = node.get_property("selected").unwrap();
    prop.set_null(Role::App, 0).unwrap();
    prop.set_null(Role::App, 1).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    let editbox_text = PropertyStr::wrap(node, Role::App, "text", 0).unwrap();
    let editbox_focus = PropertyBool::wrap(node, Role::App, "is_focused", 0).unwrap();
    //let darkirc_backend = app.darkirc_backend.clone();
    //let task = app.ex.spawn(async move {
    //    while let Ok(_) = btn_click_recvr.recv().await {
    //        let text = editbox_text.get();
    //        editbox_text.prop().unset(Role::App, 0).unwrap();
    //        // Clicking outside the editbox makes it lose focus
    //        // So lets focus it again
    //        editbox_focus.set(true);

    //        debug!(target: "app", "sending text {text}");

    //        let privmsg =
    //            Privmsg { channel: "#random".to_string(), nick: "king".to_string(), msg: text };
    //        darkirc_backend.send(privmsg).await;
    //    }
    //});
    //tasks.push(task);

    drop(sg);
    let pimpl = EditBox::new(
        app.ex.clone(),
        app.sg.clone(),
        node_id,
        app.render_api.clone(),
        app.event_pub.clone(),
        app.text_shaper.clone(),
    )
    .await;
    let mut sg = app.sg.lock().await;
    let node = sg.get_node_mut(node_id).unwrap();
    node.pimpl = pimpl;

    sg.link(node_id, layer_node_id).unwrap();
}

