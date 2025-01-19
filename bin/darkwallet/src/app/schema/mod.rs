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

use darkfi_serial::Encodable;
use sled_overlay::sled;
use std::fs::File;

use crate::{
    app::{
        node::{
            create_button, create_chatedit, create_chatview, create_editbox, create_image,
            create_layer, create_shortcut, create_text, create_vector_art,
        },
        populate_tree, App,
    },
    error::Error,
    expr::{self, Compiler, Op},
    gfx::{GraphicsEventPublisherPtr, Rectangle, RenderApi, Vertex},
    mesh::{Color, MeshBuilder},
    prop::{
        Property, PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyStr, PropertySubType,
        PropertyType, Role,
    },
    scene::{SceneNodePtr, Slot},
    shape,
    text::TextShaperPtr,
    ui::{
        emoji_picker, Button, ChatEdit, ChatView, EditBox, Image, Layer, ShapeVertex, Shortcut,
        Text, VectorArt, VectorShape, Window,
    },
    ExecutorPtr,
};

mod chat;
mod menu;
pub mod test;

pub const COLOR_SCHEME: ColorScheme = ColorScheme::DarkMode;
//pub const COLOR_SCHEME: ColorScheme = ColorScheme::PaperLight;

mod android_ui_consts {
    pub const NETSTATUS_ICON_SIZE: f32 = 140.;
    pub const NETLOGO_SCALE: f32 = 50.;
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 100.;
}

#[cfg(target_os = "android")]
mod ui_consts {
    use crate::android::get_appdata_path;
    use std::path::PathBuf;

    pub const BG_PATH: &str = "bg.png";
    pub use super::android_ui_consts::*;

    pub fn get_chatdb_path() -> PathBuf {
        get_appdata_path().join("chatdb")
    }
    pub fn get_first_time_filename() -> PathBuf {
        get_appdata_path().join("first_time")
    }

    pub fn get_window_scale_filename() -> PathBuf {
        get_appdata_path().join("window_scale")
    }
}

#[cfg(not(target_os = "android"))]
mod desktop_paths {
    use std::path::PathBuf;

    pub const BG_PATH: &str = "assets/bg.png";

    pub fn get_chatdb_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/wallet/chatdb")
    }
    pub fn get_first_time_filename() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/wallet/first_time")
    }

    pub fn get_window_scale_filename() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/wallet/window_scale")
    }
}

#[cfg(feature = "emulate-android")]
mod ui_consts {
    pub use super::{android_ui_consts::*, desktop_paths::*};
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const NETSTATUS_ICON_SIZE: f32 = 60.;
    pub const NETLOGO_SCALE: f32 = 25.;
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 40.;
    pub use super::desktop_paths::*;
}

pub use ui_consts::*;

pub static CHANNELS: &'static [&str] =
    &["dev", "media", "hackers", "memes", "philosophy", "markets", "math", "random"];

#[derive(PartialEq)]
enum ColorScheme {
    DarkMode,
    PaperLight,
}

pub async fn make(app: &App, window: SceneNodePtr) {
    let mut cc = Compiler::new();
    cc.add_const_f32("NETSTATUS_ICON_SIZE", NETSTATUS_ICON_SIZE);

    let atom = &mut PropertyAtomicGuard::new();

    let node = create_shortcut("zoom_out_shortcut");
    node.set_property_str(atom, Role::App, "key", "ctrl+-").unwrap();
    // Not sure what was eating my keys. This is a workaround.
    node.set_property_u32(atom, Role::App, "priority", 10).unwrap();
    let (slot, recvr) = Slot::new("zoom_out_pressed");
    node.register("shortcut", slot).unwrap();
    let window_scale = PropertyFloat32::wrap(&window, Role::App, "scale", 0).unwrap();
    let window_scale2 = window_scale.clone();
    let listen_zoom = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let scale = 0.9 * window_scale2.get();

            let filename = get_window_scale_filename();
            if let Some(parent) = filename.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = File::create(filename) {
                scale.encode(&mut file).unwrap();
            }

            let atom = &mut PropertyAtomicGuard::new();
            window_scale2.set(atom, scale);
        }
    });
    app.tasks.lock().unwrap().push(listen_zoom);
    let node = node.setup(|me| Shortcut::new(me)).await;
    window.clone().link(node);

    let node = create_shortcut("zoom_in_shortcut");
    node.set_property_str(atom, Role::App, "key", "ctrl+=").unwrap();
    // Not sure what was eating my keys. This is a workaround.
    node.set_property_u32(atom, Role::App, "priority", 10).unwrap();
    let (slot, recvr) = Slot::new("zoom_in_pressed");
    node.register("shortcut", slot).unwrap();
    let window_scale = PropertyFloat32::wrap(&window, Role::App, "scale", 0).unwrap();
    let window_scale2 = window_scale.clone();
    let listen_zoom = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let scale = 1.1 * window_scale2.get();

            let filename = get_window_scale_filename();
            if let Some(parent) = filename.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = File::create(filename) {
                scale.encode(&mut file).unwrap();
            }

            let atom = &mut PropertyAtomicGuard::new();
            window_scale2.set(atom, scale);
        }
    });
    app.tasks.lock().unwrap().push(listen_zoom);
    let node = node.setup(|me| Shortcut::new(me)).await;
    window.clone().link(node);

    if COLOR_SCHEME == ColorScheme::DarkMode {
        // Bg layer
        let layer_node = create_layer("bg_layer");
        let prop = layer_node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
        layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        layer_node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        let layer_node =
            layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
        window.clone().link(layer_node.clone());

        // Create a bg image
        let node = create_image("bg_image");
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();

        // Image aspect ratio
        //let R = 1.78;
        let R = 1.555;
        cc.add_const_f32("R", R);

        let prop = node.get_property("uv").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
        #[rustfmt::skip]
    let code = cc.compile("
        r = w / h;
        if r < R {
            r / R
        } else {
            1
        }
    ").unwrap();
        prop.clone().set_expr(atom, Role::App, 2, code).unwrap();
        #[rustfmt::skip]
    let code = cc.compile("
        r = w / h;
        if r < R {
            1
        } else {
            R / r
        }
    ").unwrap();
        prop.clone().set_expr(atom, Role::App, 3, code).unwrap();

        node.set_property_str(atom, Role::App, "path", BG_PATH).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        let node = node.setup(|me| Image::new(me, app.render_api.clone(), app.ex.clone())).await;
        layer_node.clone().link(node);

        // Create a bg mesh on top to fade the bg image
        let node = create_vector_art("bg");
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

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
        let node = node
            .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
            .await;
        layer_node.clone().link(node);
    } else if COLOR_SCHEME == ColorScheme::PaperLight {
        let node = create_vector_art("bg");
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

        let c = 1.;
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
        let node = node
            .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
            .await;
        window.clone().link(node);
    }

    let netlayer_node = create_layer("netstatus_layer");
    let prop = netlayer_node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE").unwrap();
    prop.clone().set_expr(atom, Role::App, 0, code).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
    //prop.clone().set_f32(atom, Role::App, 2, NETSTATUS_ICON_SIZE).unwrap();
    //prop.clone().set_f32(atom, Role::App, 3, NETSTATUS_ICON_SIZE).unwrap();
    prop.clone().set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.clone().set_f32(atom, Role::App, 3, 1000.).unwrap();
    netlayer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    netlayer_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let netlayer_node =
        netlayer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.clone().link(netlayer_node.clone());

    let node = create_vector_art("net0");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = shape::create_netlogo1([1., 0., 0.25, 1.]).scaled(NETLOGO_SCALE);
    shape.join(shape::create_netlogo2([0.27, 0.4, 0.4, 1.]).scaled(NETLOGO_SCALE));
    shape.join(shape::create_netlogo3([0.27, 0.4, 0.4, 1.]).scaled(NETLOGO_SCALE));
    let net0_node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    netlayer_node.clone().link(net0_node);

    let node = create_vector_art("net1");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = shape::create_netlogo1([0.49, 0.57, 1., 1.]).scaled(NETLOGO_SCALE);
    shape.join(shape::create_netlogo2([0.49, 0.57, 1., 1.]).scaled(NETLOGO_SCALE));
    shape.join(shape::create_netlogo3([0.27, 0.4, 0.4, 1.]).scaled(NETLOGO_SCALE));
    let net1_node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    netlayer_node.clone().link(net1_node);

    let node = create_vector_art("net2");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = shape::create_netlogo1([0., 0.94, 1., 1.]).scaled(NETLOGO_SCALE);
    shape.join(shape::create_netlogo2([0., 0.94, 1., 1.]).scaled(NETLOGO_SCALE));
    shape.join(shape::create_netlogo3([0., 0.94, 1., 1.]).scaled(NETLOGO_SCALE));
    let net2_node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    netlayer_node.clone().link(net2_node);

    let node = create_vector_art("net3");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = shape::create_netlogo1([0., 0.94, 1., 1.]).scaled(NETLOGO_SCALE);
    shape.join(shape::create_netlogo2([0., 0.94, 1., 1.]).scaled(NETLOGO_SCALE));
    shape.join(shape::create_netlogo3([0., 0.94, 1., 1.]).scaled(NETLOGO_SCALE));
    let net3_node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    netlayer_node.clone().link(net3_node);

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

    let is_first_time = !get_first_time_filename().exists();
    if is_first_time {
        let filename = get_first_time_filename();
        if let Some(parent) = filename.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = File::create(filename);
    }

    let chatdb_path = get_chatdb_path();
    let db = sled::open(chatdb_path).expect("cannot open sleddb");
    for channel in CHANNELS {
        chat::make(app, window.clone(), channel, &db, emoji_meshes.clone(), is_first_time).await;
    }
    menu::make(app, window.clone()).await;

    // @@@ Debug stuff @@@
    //let chatview_node = app.sg_root.clone().lookup_node("/window/dev_chat_layer").unwrap();
    //chatview_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    //let menu_node = app.sg_root.clone().lookup_node("/window/menu_layer").unwrap();
    //menu_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
}
