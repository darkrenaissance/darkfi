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
    app::{
        node::{
            create_button, create_chatedit, create_chatview, create_editbox, create_image,
            create_layer, create_text, create_vector_art,
        },
        populate_tree, App,
    },
    error::Error,
    expr::{self, Compiler, Op},
    gfx::{GraphicsEventPublisherPtr, Rectangle, RenderApi, Vertex},
    mesh::{Color, MeshBuilder},
    prop::{
        Property, PropertyBool, PropertyFloat32, PropertyStr, PropertySubType, PropertyType, Role,
    },
    scene::{SceneNodePtr, Slot},
    shape,
    text::TextShaperPtr,
    ui::{
        emoji_picker, Button, ChatEdit, ChatView, EditBox, Image, Layer, ShapeVertex, Text,
        VectorArt, VectorShape, Window,
    },
    ExecutorPtr,
};

mod chat;
mod menu;
//mod test;

#[cfg(target_os = "android")]
mod ui_consts {
    pub const CHATDB_PATH: &str = "/data/data/darkfi.darkwallet/chatdb/";
    pub const BG_PATH: &str = "bg.png";
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 140.;
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const CHATDB_PATH: &str = "chatdb";
    pub const BG_PATH: &str = "assets/bg.png";
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 40.;
}

use ui_consts::*;

pub static CHANNELS: &'static [&str] =
    &["dev", "media", "hackers", "memes", "philosophy", "markets", "math", "random"];

pub async fn make(app: &App, window: SceneNodePtr) {
    let mut cc = Compiler::new();

    // Bg layer
    let layer_node = create_layer("bg_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(Role::App, "z_index", 0).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.clone().link(layer_node.clone());

    // Create a bg image
    let node = create_image("bg_image");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();

    // Image aspect ratio
    //let R = 1.78;
    let R = 1.555;
    cc.add_const_f32("R", R);

    let prop = node.get_property("uv").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    #[rustfmt::skip]
    let code = cc.compile("
        r = w / h;
        if r < R {
            r / R
        } else {
            1
        }
    ").unwrap();
    prop.set_expr(Role::App, 2, code).unwrap();
    #[rustfmt::skip]
    let code = cc.compile("
        r = w / h;
        if r < R {
            1
        } else {
            R / r
        }
    ").unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();

    node.set_property_str(Role::App, "path", BG_PATH).unwrap();
    node.set_property_u32(Role::App, "z_index", 0).unwrap();
    let node = node.setup(|me| Image::new(me, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create a bg mesh on top to fade the bg image
    let node = create_vector_art("bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();

    //let c = if LIGHTMODE { 1. } else { 0. };
    let c = 0.;
    // Setup the pimpl
    let node_id = node.id;
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [c, c, c, 0.3],
    );
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    let emoji_meshes = emoji_picker::EmojiMeshes::new(
        app.render_api.clone(),
        app.text_shaper.clone(),
        EMOJI_PICKER_ICON_SIZE,
    );

    let emoji_meshes2 = emoji_meshes.clone();
    std::thread::spawn(move || {
        for i in (0..500).step_by(20) {
            let mut emoji = emoji_meshes2.lock().unwrap();
            for j in i..(i + 20) {
                emoji.get(j);
            }
        }
    });

    let db = sled::open(CHATDB_PATH).expect("cannot open sleddb");
    for channel in CHANNELS {
        chat::make(app, window.clone(), channel, &db, emoji_meshes.clone()).await;
    }
    menu::make(app, window.clone()).await;

    // @@@ Debug stuff @@@
    let chatview_node = app.sg_root.clone().lookup_node("/window/dev_chat_layer").unwrap();
    chatview_node.set_property_bool(Role::App, "is_visible", true).unwrap();
    let menu_node = app.sg_root.clone().lookup_node("/window/menu_layer").unwrap();
    menu_node.set_property_bool(Role::App, "is_visible", false).unwrap();
}
