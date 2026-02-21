/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_serial::deserialize;

use super::{ColorScheme, CHANNELS, COLOR_SCHEME};
use crate::{
    app::{
        node::{
            create_button, create_layer, create_menu, create_shortcut, create_text,
            create_vector_art,
        },
        App,
    },
    expr,
    gfx::gfxtag,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{Button, Layer, Menu, ShapeVertex, Shortcut, Text, VectorArt, VectorShape},
    util::i18n::I18nBabelFish,
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const CHANNEL_LABEL_X: f32 = 40.;
    pub const CHANNEL_LABEL_Y: f32 = 35.;
    pub const CHANNEL_LABEL_LINESPACE: f32 = 140.;
    pub const CHANNEL_LABEL_FONTSIZE: f32 = 44.;
    pub const MENU_SEP_SIZE: f32 = 3.;
    pub const MENU_HANDLE_PAD: f32 = 200.;
    pub const MENU_FADE: f32 = 1200.;
    pub const VERBLOCK_SCALE: f32 = 150.;
    pub const VERBLOCK_X: f32 = 180.;
    pub const VERBLOCK_Y: f32 = 80.;
}

#[cfg(target_os = "android")]
mod ui_consts {
    pub use super::android_ui_consts::*;
}

#[cfg(feature = "emulate-android")]
mod ui_consts {
    pub use super::android_ui_consts::*;
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const CHANNEL_LABEL_X: f32 = 20.;
    pub const CHANNEL_LABEL_Y: f32 = 14.;
    pub const CHANNEL_LABEL_LINESPACE: f32 = 60.;
    pub const CHANNEL_LABEL_FONTSIZE: f32 = 22.;
    pub const MENU_SEP_SIZE: f32 = 1.;
    pub const MENU_HANDLE_PAD: f32 = 100.;
    pub const MENU_FADE: f32 = 600.;
    pub const VERBLOCK_SCALE: f32 = 80.;
    pub const VERBLOCK_X: f32 = 110.;
    pub const VERBLOCK_Y: f32 = 50.;
}

use ui_consts::*;

pub async fn make(app: &App, content: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("VERBLOCK_Y", VERBLOCK_Y);
    cc.add_const_f32("CHANNEL_LABEL_LINESPACE", CHANNEL_LABEL_LINESPACE);

    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/setting/scale").unwrap(),
        Role::Internal,
        "value",
        0,
    )
    .unwrap();
    let atom = &mut PropertyAtomicGuard::none();

    // Main view
    let layer_node = create_layer("menu_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let layer_node = layer_node.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(layer_node.clone());

    // Channels label bg
    let node = create_vector_art("channels_label_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let x1 = expr::const_f32(0.);
    let y1 = expr::const_f32(0.);
    let x2 = expr::load_var("w");
    let y2 = expr::const_f32(CHANNEL_LABEL_LINESPACE);
    let (color1, color2) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0., 0., 1.]),
        ColorScheme::PaperLight => ([1., 1., 1., 1.], [1., 1., 1., 1.]),
    };
    let mut verts = vec![
        ShapeVertex::new(x1.clone(), y1.clone(), color1),
        ShapeVertex::new(x2.clone(), y1.clone(), color1),
        ShapeVertex::new(x1.clone(), y2.clone(), color2),
        ShapeVertex::new(x2, y2, color2),
    ];
    let mut indices = vec![0, 2, 1, 1, 2, 3];
    shape.verts.append(&mut verts);
    shape.indices.append(&mut indices);

    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(CHANNEL_LABEL_LINESPACE - 1.),
        expr::load_var("w"),
        expr::const_f32(CHANNEL_LABEL_LINESPACE),
        [0.15, 0.2, 0.19, 1.],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer_node.link(node);

    // Create some text
    let node = create_text("channels_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", CHANNEL_LABEL_FONTSIZE).unwrap();
    node.set_property_bool(atom, Role::App, "use_i18n", true).unwrap();
    node.set_property_str(atom, Role::App, "text", "channels-label").unwrap();
    //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.65).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.87).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.83).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    layer_node.link(node);

    let node = create_vector_art("version_block");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, VERBLOCK_X).unwrap();
    let code = cc.compile("h - VERBLOCK_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "scale", VERBLOCK_SCALE).unwrap();
    let shape = shape::create_version_block([1., 0., 0.25, 1.]);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    let version_block_is_visible = PropertyBool::wrap(&node, Role::App, "is_visible", 0).unwrap();
    layer_node.link(node);

    // Make buttons for cancel and done

    let node = create_layer("editbtn_layer");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 50.).unwrap();
    let code = cc.compile("h - 100 - 50").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - 100").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, 100.).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let editlayer_node = node.setup(|me| Layer::new(me, app.renderer.clone())).await;
    layer_node.link(editlayer_node.clone());

    let editlayer_is_visible =
        PropertyBool::wrap(&editlayer_node, Role::App, "is_visible", 0).unwrap();

    let node = create_vector_art("btns_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    shape.add_gradient_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(200.),
        expr::load_var("h"),
        [[0., 0., 0., 1.], [0., 0., 0., 1.], [0.1, 0., 0., 1.], [0.1, 0., 0., 1.]],
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(200.),
        expr::load_var("h"),
        1.,
        [1., 0., 0., 1.],
    );

    shape.add_gradient_box(
        cc.compile("w - 200").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [[0., 0.1, 0.15, 1.], [0., 0.1, 0.15, 1.], [0., 0., 0., 1.], [0., 0., 0., 1.]],
    );
    shape.add_outline(
        cc.compile("w - 200").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        [0., 0.94, 1., 1.],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    editlayer_node.link(node);

    // Menu

    let node = create_menu("main_menu");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_LINESPACE).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h - CHANNEL_LABEL_LINESPACE").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 0).unwrap();
    node.set_property_f32(atom, Role::App, "padding", CHANNEL_LABEL_LINESPACE).unwrap();

    let prop = node.get_property("bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.5).unwrap();

    node.set_property_f32(atom, Role::App, "font_size", CHANNEL_LABEL_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "sep_size", MENU_SEP_SIZE).unwrap();

    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("active_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.36).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.51).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("alert_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.56).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.61).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("sep_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.4).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_LINESPACE / 2.).unwrap();

    node.set_property_f32(atom, Role::App, "handle_padding", MENU_HANDLE_PAD).unwrap();
    node.set_property_f32(atom, Role::App, "fade_zone", MENU_FADE).unwrap();

    let prop = node.get_property("items").unwrap();
    for channel in CHANNELS {
        prop.push_str(atom, Role::App, *channel).unwrap();
    }
    for channel in [
        "@john", "@stacy", "@barry", "@steve", "@obombo", "@xyz", "@lunar", "@fren", "@anon",
        "@anon1",
    ] {
        prop.push_str(atom, Role::App, channel).unwrap();
    }

    let (slot, recvr) = Slot::new("menu_clicked");
    node.register("select", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let menu_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let channel: String = deserialize(&data).unwrap();
            let path = format!("/window/content/{}_chat_layer", channel);
            let node = sg_root.lookup_node(path).unwrap();

            let atom = &mut renderer.make_guard(gfxtag!("channel_clicked"));
            info!(target: "app::menu", "clicked: {channel}!");
            node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
            menu_is_visible.set(atom, false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Subscribe to edit_active signal to hide version block
    let (edit_slot, edit_recvr) = Slot::new("edit_activated");
    node.register("edit_active", edit_slot).unwrap();
    let renderer = app.renderer.clone();
    let editlayer_is_visible2 = editlayer_is_visible.clone();
    let edit_listen = app.ex.spawn(async move {
        while let Ok(_) = edit_recvr.recv().await {
            debug!(target: "app::menu", "menu edit active");
            let atom = &mut renderer.make_guard(gfxtag!("edit_active"));
            version_block_is_visible.set(atom, false);
            editlayer_is_visible2.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(edit_listen);

    let menu_node =
        node.setup(|me| Menu::new(me, window_scale.clone(), app.renderer.clone())).await;
    layer_node.link(menu_node.clone());

    // Subscribe to edit_done signal to log deleted items
    let (edit_done_slot, edit_done_recvr) = Slot::new("edit_done");
    menu_node.register("edit_done", edit_done_slot).unwrap();
    let sg_root = app.sg_root.clone();
    let edit_done_listen = app.ex.spawn(async move {
        while let Ok(data) = edit_done_recvr.recv().await {
            let deleted_items: Vec<String> = deserialize(&data).unwrap();
            for item in deleted_items {
                let path = format!("/window/content/{}_chat_layer", item);
                let node = sg_root.lookup_node(path).unwrap();
                node.clear_tasks();
                debug!(target: "app::menu", "deleted item: {item}");
                node.unlink();
            }
        }
    });
    app.tasks.lock().unwrap().push(edit_done_listen);

    // Create the cancel button
    let node = create_button("cancel_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 200.).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();

    let (slot, recvr) = Slot::new("cancel_clicked");
    node.register("click", slot).unwrap();
    let menu_node2 = menu_node.clone();
    let renderer = app.renderer.clone();
    let editlayer_is_visible2 = editlayer_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            menu_node2.call_method("cancel_edit", vec![]).await.unwrap();
            let atom = &mut renderer.make_guard(gfxtag!("cancel_clicked"));
            editlayer_is_visible2.set(atom, false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(Button::new).await;
    editlayer_node.link(node);

    // Create the done button
    let node = create_button("done_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - 200").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 200.).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();

    let (slot, recvr) = Slot::new("done_clicked");
    node.register("click", slot).unwrap();
    let menu_node2 = menu_node.clone();
    let renderer = app.renderer.clone();
    let editlayer_is_visible2 = editlayer_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            menu_node2.call_method("done_edit", vec![]).await.unwrap();
            let atom = &mut renderer.make_guard(gfxtag!("done_clicked"));
            editlayer_is_visible2.set(atom, false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(Button::new).await;
    editlayer_node.link(node);
}
