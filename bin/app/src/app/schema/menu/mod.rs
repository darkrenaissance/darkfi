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
use kvdb_overlay::Database as KvDb;
use std::io::Write;
use ui_consts::*;

use super::{read_joined_channels, write_joined_channels, ColorScheme, COLOR_SCHEME};
use crate::{
    app::{
        node::{create_button, create_layer, create_menu, create_text, create_vector_art},
        App,
    },
    db::AppDbPtr,
    expr,
    gfx::gfxtag,
    mesh::COLOR_CYAN,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    sfx, shape,
    ui::{
        emoji_picker::EmojiMeshesPtr, Button, Layer, Menu, ShapeVertex, Text, VectorArt,
        VectorShape,
    },
    util::i18n::I18nBabelFish,
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const CHANNEL_LABEL_X: f32 = 40.;
    pub const CHANNEL_LABEL_Y: f32 = 35.;
    pub const CHANNEL_HEADER_HEIGHT: f32 = 140.;
    pub const CHANNEL_ITEM_HEIGHT: f32 = 115.;
    pub const CHANNEL_LABEL_FONTSIZE: f32 = 46.;
    pub const MENU_SEP_SIZE: f32 = 3.;
    pub const MENU_HANDLE_PAD: f32 = 200.;
    pub const MENU_FADE: f32 = 1200.;
    pub const VERBLOCK_SCALE: f32 = 150.;
    pub const VERBLOCK_X: f32 = 140.;
    pub const VERBLOCK_Y: f32 = 130.;
    pub const NETLOGO_SCALE: f32 = 16.;
    // Button constants
    pub const MENU_BTN_W_L: f32 = 250.;
    pub const MENU_BTN_W_R: f32 = 200.;
    pub const MENU_BTN_H: f32 = 180.;
    pub const EDIT_BTN_OUTLINE_T: f32 = 2.;
    pub const BTN_TEXT_FONTSIZE: f32 = 50.;
    pub const BTN_TEXT_Y: f32 = 30.;
    pub const LABEL_LINESPACE: f32 = 60.;
    pub const MENU_ICON_SCALE: f32 = 220.;
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
    pub const CHANNEL_HEADER_HEIGHT: f32 = 60.;
    pub const CHANNEL_ITEM_HEIGHT: f32 = 40.;
    pub const CHANNEL_LABEL_FONTSIZE: f32 = 18.;
    pub const MENU_SEP_SIZE: f32 = 1.;
    pub const MENU_HANDLE_PAD: f32 = 100.;
    pub const MENU_FADE: f32 = 600.;
    pub const VERBLOCK_SCALE: f32 = 80.;
    pub const VERBLOCK_X: f32 = 80.;
    pub const OUTLINE_MINT: [f32; 4] = [0.467, 1.0, 0.745, 1.0];
    pub const VERBLOCK_Y: f32 = 75.;
    pub const NETLOGO_SCALE: f32 = 7.;
    // Button constants
    pub const MENU_BTN_W_L: f32 = 110.;
    pub const MENU_BTN_W_R: f32 = 85.;
    pub const MENU_BTN_H: f32 = 100.;
    pub const EDIT_BTN_OUTLINE_T: f32 = 1.;
    pub const BTN_TEXT_FONTSIZE: f32 = 20.;
    pub const BTN_TEXT_Y: f32 = 14.;
    pub const LABEL_LINESPACE: f32 = 140.;
    pub const MENU_ICON_SCALE: f32 = 100.;
}

pub mod channel;
pub mod contact;
mod edit_buttons;
mod edit_switch;

pub async fn make(
    app: &App,
    chat_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    app_db: AppDbPtr,
    kv_db: &KvDb,
    emoji_meshes: EmojiMeshesPtr,
) {
    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/window").unwrap(),
        Role::Internal,
        "scale",
        0,
    )
    .unwrap();

    let atom = &mut PropertyAtomicGuard::none();

    // Create contact screen
    let contact_layer = create_layer("contact_screen_layer");
    let prop = contact_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    contact_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    contact_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let contact_layer = contact_layer
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    chat_layer.link(contact_layer.clone());
    let contact_is_visible =
        PropertyBool::wrap(&contact_layer, Role::App, "is_visible", 0).unwrap();

    // Create channel screen
    let channel_layer = create_layer("channel_screen_layer");
    let prop = channel_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    channel_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    channel_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let channel_layer = channel_layer
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    chat_layer.link(channel_layer.clone());
    let channel_is_visible =
        PropertyBool::wrap(&channel_layer, Role::App, "is_visible", 0).unwrap();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("VERBLOCK_X", VERBLOCK_X);
    cc.add_const_f32("VERBLOCK_Y", VERBLOCK_Y);
    cc.add_const_f32("CHANNEL_HEADER_HEIGHT", CHANNEL_HEADER_HEIGHT);
    cc.add_const_f32("CHANNEL_ITEM_HEIGHT", CHANNEL_ITEM_HEIGHT);
    cc.add_const_f32("CHANNEL_LABEL_X", CHANNEL_LABEL_X);
    cc.add_const_f32("MENU_BTN_W_L", MENU_BTN_W_L);
    cc.add_const_f32("MENU_BTN_W_R", MENU_BTN_W_R);
    cc.add_const_f32("MENU_BTN_H", MENU_BTN_H);
    cc.add_const_f32("MENU_ICON_SCALE", MENU_ICON_SCALE);

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
    let layer_node = layer_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    chat_layer.link(layer_node.clone());

    let menulayer_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();

    // Build contact screen UI
    contact::make(
        app,
        contact_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        contact_is_visible.clone(),
        channel_is_visible.clone(),
        app_db.clone(),
        kv_db,
        emoji_meshes.clone(),
    )
    .await;

    // Build channel screen UI
    channel::make(
        app,
        channel_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        contact_is_visible.clone(),
        channel_is_visible.clone(),
        app_db.clone(),
        kv_db,
        emoji_meshes.clone(),
    )
    .await;

    // Channels label bg
    let node = create_vector_art("channels_label_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, CHANNEL_HEADER_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    let (bg_color, sep_color) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0., 0., 1.], [0.41, 0.6, 0.65, 1.]),
        ColorScheme::PaperLight => ([1., 1., 1., 1.], [0., 0.6, 0.65, 1.]),
    };
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        bg_color,
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::load_var("h"),
        expr::load_var("w"),
        cc.compile("h + 0.5").unwrap(),
        sep_color,
    );
    let color1 = [0., 0.17, 0.18, 0.5];
    let color2 = [0., 0.88, 1., 0.];
    shape.add_smooth_vertical_gradient(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        cc.compile("h / 2").unwrap(),
        color1,
        color2,
        8,
        0.2,
    );

    let node = node
        .setup(|me| VectorArt::new(me, shape, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
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
        .setup(|me| {
            Text::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                i18n_fish.clone(),
                app.redraw_trigger.clone(),
            )
        })
        .await;
    layer_node.link(node);

    // Main button layer
    let node = create_layer("mainbtn_layer");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    let code = cc.compile("h - MENU_BTN_H - CHANNEL_LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - 2 * CHANNEL_LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, MENU_BTN_H).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let mainlayer_node =
        node.setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    let mainlayer_is_visible =
        PropertyBool::wrap(&mainlayer_node, Role::App, "is_visible", 0).unwrap();
    layer_node.link(mainlayer_node.clone());

    let node = create_vector_art("version_block");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("CHANNEL_LABEL_X + VERBLOCK_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - MENU_BTN_H - CHANNEL_LABEL_X + VERBLOCK_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_f32(atom, Role::App, "scale", VERBLOCK_SCALE).unwrap();
    let shape = shape::create_version_block([1., 0., 0.25, 1.]);

    let node = node
        .setup(|me| VectorArt::new(me, shape, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    layer_node.link(node);

    // Write / Menu button
    let node = create_button("write_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - MENU_BTN_W_R").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_R).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    //Uncomment this to see the button outline
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();
    //node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let (slot, recvr) = Slot::new("write_clicked");
    node.register("click", slot).unwrap();
    let redraw = app.redraw_trigger.clone();
    let contact_is_visible =
        PropertyBool::wrap(&contact_layer, Role::App, "is_visible", 0).unwrap();
    let menulayer_is_visible1 = menulayer_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut redraw.make_guard(gfxtag!("write_click"));
            contact_is_visible.set(atom, true);
            menulayer_is_visible1.set(atom, false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    mainlayer_node.link(node);

    let node = create_vector_art("write_icon");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - MENU_BTN_W_R / 2 - MENU_ICON_SCALE * 0.45").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, MENU_BTN_H / 2. + MENU_ICON_SCALE * 0.36).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_ICON_SCALE).unwrap();
    prop.set_f32(atom, Role::App, 3, MENU_ICON_SCALE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let shape = shape::create_menu_icon(COLOR_CYAN).scaled(MENU_ICON_SCALE);
    let node = node
        .setup(|me| VectorArt::new(me, shape, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    mainlayer_node.link(node);

    // Create cancel/done edit buttons
    let btns =
        edit_buttons::create_edit_buttons(app, layer_node.clone(), &window_scale, i18n_fish).await;

    // Menu
    let node = create_menu("main_menu");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_HEADER_HEIGHT).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h - CHANNEL_HEADER_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 0).unwrap();
    node.set_property_f32(atom, Role::App, "padding", CHANNEL_ITEM_HEIGHT).unwrap();

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

    let prop = node.get_property("sep_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.4).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_ITEM_HEIGHT / 2.).unwrap();
    node.set_property_f32(atom, Role::App, "handle_padding", MENU_HANDLE_PAD).unwrap();
    node.set_property_f32(atom, Role::App, "fade_zone", MENU_FADE).unwrap();

    let prop = node.get_property("items").unwrap();
    for name in read_joined_channels() {
        prop.push_str(atom, Role::App, &name).unwrap();
    }

    let (slot, recvr) = Slot::new("menu_clicked");
    node.register("select", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let menu_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();
    let redraw = app.redraw_trigger.clone();
    let role1_group = node.get_property("role1_group").unwrap();
    let role2_group = node.get_property("role2_group").unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let channel: String = deserialize(&data).unwrap();
            let path = format!("/window/content/chat/{}_chat_layer", channel);
            if let Some(node) = sg_root.lookup_node(path) {
                let atom = &mut redraw.make_guard(gfxtag!("channel_clicked"));
                info!(target: "app::menu", "clicked: {channel}!");
                sfx::play_click();
                role1_group.remove_str_item(atom, Role::App, &channel);
                role2_group.remove_str_item(atom, Role::App, &channel);
                node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                menu_is_visible.set(atom, false);
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let menu_node = node
        .setup(|me| {
            Menu::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                app.redraw_trigger.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.link(menu_node.clone());

    // Subscribe to edit_done signal to persist the joined list and unlink removed layers
    let (edit_done_slot, edit_done_recvr) = Slot::new("edit_done");
    menu_node.register("edit_done", edit_done_slot).unwrap();
    let sg_root = app.sg_root.clone();
    let menu_node2 = menu_node.clone();
    let redraw = app.redraw_trigger.clone();
    let edit_done_listen = app.ex.spawn(async move {
        while let Ok(data) = edit_done_recvr.recv().await {
            let deleted_items: Vec<String> = deserialize(&data).unwrap();
            // Persist the post-edit joined list (captures removals + reorders)
            let current: Vec<String> =
                menu_node2.get_property("items").unwrap().get_str_vec().unwrap();
            write_joined_channels(&current);

            for item in &deleted_items {
                let path = format!("/window/content/chat/{}_chat_layer", item);
                if let Some(node) = sg_root.lookup_node(&path) {
                    node.clear_tasks();
                    debug!(target: "app::menu", "deleted item: {item}");
                    node.unlink();
                }
            }

            // Unlinking changes no property, so request a pass explicitly:
            // the parent layer re-collects child references next pass and
            // the removed layer's subtree is dropped from the output.
            redraw.trigger();

            // TODO: reload plugin so it drops un-joined channels/contacts from its maps.
            // if let Some(darkirc) = sg_root.lookup_node("/plugin/darkirc") {
            //     for item in &deleted_items {
            //         let data = serialize(item);
            //         let _ = darkirc.call_method("rescan", data).await;
            //     }
            // }
        }
    });
    app.tasks.lock().unwrap().push(edit_done_listen);

    // Connect cancel/done buttons and edit_active signal
    btns.connect_edit_handlers(app, &menu_node, Some(mainlayer_is_visible.clone()));
}

pub async fn setup_wallet_button(app: &App, chat_layer: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let atom = &mut PropertyAtomicGuard::none();
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("MENU_BTN_W_R", MENU_BTN_W_R);
    cc.add_const_f32("MENU_BTN_H", MENU_BTN_H);
    cc.add_const_f32("CHANNEL_LABEL_X", CHANNEL_LABEL_X);

    let _window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/window").unwrap(),
        Role::Internal,
        "scale",
        0,
    )
    .unwrap();

    let chat_is_visible = PropertyBool::wrap(&chat_layer, Role::App, "is_visible", 0).unwrap();
    let menu_layer = chat_layer.lookup_node("/menu_layer").unwrap();

    // Wallet status icon (blockchain netlogo, all cyan = fully connected)
    // Positioned at center of wallet button area, above write_btn
    let node = create_vector_art("wallet_icon");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - MENU_BTN_W_R / 2 - CHANNEL_LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - MENU_BTN_H - MENU_BTN_W_R / 2 - CHANNEL_LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1(COLOR_CYAN);
    shape.join(shape::create_blockchain_netlogo2(COLOR_CYAN));
    shape.join(shape::create_blockchain_netlogo3(COLOR_CYAN));
    shape.join(shape::create_blockchain_netlogo4(COLOR_CYAN));
    let node = node
        .setup(|me| VectorArt::new(me, shape, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    menu_layer.link(node);

    // Wallet button (above write_btn, square: w = h = MENU_BTN_W_R)
    let node = create_button("wallet_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - MENU_BTN_W_R - CHANNEL_LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - MENU_BTN_H - MENU_BTN_W_R - CHANNEL_LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_R).unwrap();
    prop.set_f32(atom, Role::App, 3, MENU_BTN_W_R).unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();

    let (slot, recvr) = Slot::new("wallet_clicked");
    node.register("click", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let wallet_is_visible = PropertyBool::wrap(
        &sg_root.lookup_node("/window/content/wallet").unwrap(),
        Role::App,
        "is_visible",
        0,
    )
    .unwrap();
    let redraw = app.redraw_trigger.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut redraw.make_guard(gfxtag!("wallet_click"));
            wallet_is_visible.set(atom, true);
            chat_is_visible.set(atom, false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let renderer = app.renderer.clone();
    let redraw = app.redraw_trigger.clone();

    let node = node.setup(|me| Button::new(me, renderer, redraw)).await;
    menu_layer.link(node);
}
