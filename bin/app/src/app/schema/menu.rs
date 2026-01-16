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
    pub const VERBLOCK_SCALE: f32 = 25.;
    pub const VERBLOCK_X: f32 = 50.;
    pub const VERBLOCK_Y: f32 = 50.;
}

use ui_consts::*;

pub async fn make(app: &App, content: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("VERBLOCK_X", VERBLOCK_X);
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
    let layer_node = layer_node.setup(|me| Layer::new(me, app.render_api.clone())).await;
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

    let node = node.setup(|me| VectorArt::new(me, shape, app.render_api.clone())).await;
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
        .setup(|me| Text::new(me, window_scale.clone(), app.render_api.clone(), i18n_fish.clone()))
        .await;
    layer_node.link(node);

    let node = create_menu("main_menu");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_LINESPACE).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h - CHANNEL_LABEL_LINESPACE").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "padding", CHANNEL_LABEL_LINESPACE).unwrap();

    let prop = node.get_property("bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.5).unwrap();

    node.set_property_f32(atom, Role::App, "font_size", CHANNEL_LABEL_FONTSIZE).unwrap();

    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("sep_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.4).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_LINESPACE / 2.).unwrap();

    let prop = node.get_property("items").unwrap();
    for channel in CHANNELS {
        let label = "#".to_string() + channel;
        prop.push_str(atom, Role::App, label).unwrap();
    }
    //for channel in
    //    ["john", "stacy", "barry", "steve", "obombo", "xyz", "lunar", "fren", "anon", "anon1"]
    //{
    //    prop.push_str(atom, Role::App, channel).unwrap();
    //}

    let (slot, recvr) = Slot::new("menu_clicked");
    node.register("select", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let menu_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();
    let render_api = app.render_api.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let item_name: String = deserialize(&data).unwrap();
            // use if let here
            let channel = item_name.strip_prefix('#').unwrap();
            let chatview_path = format!("/window/content/{}_chat_layer", channel);
            let chatview_node = sg_root.lookup_node(chatview_path).unwrap();

            let atom = &mut render_api.make_guard(gfxtag!("channel_clicked"));
            info!(target: "app::menu", "clicked: {channel}!");
            chatview_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
            menu_is_visible.set(atom, false);
            //set_normal_color(atom);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Menu::new(me, window_scale.clone(), app.render_api.clone())).await;
    layer_node.link(node);

    /*
    let mut channel_y = CHANNEL_LABEL_LINESPACE;
    for (i, channel) in CHANNELS.iter().enumerate() {
        let node = create_vector_art(&(channel.to_string() + "_channel_label_bg"));
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, channel_y).unwrap();
        prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

        let mut shape = VectorShape::new();
        let bg_color = match COLOR_SCHEME {
            ColorScheme::DarkMode => [0., 0.1, 0.07, 0.7],
            ColorScheme::PaperLight => [1., 1., 1., 1.],
        };
        shape.add_filled_box(
            expr::const_f32(0.),
            expr::const_f32(0.),
            expr::load_var("w"),
            expr::const_f32(CHANNEL_LABEL_LINESPACE),
            bg_color,
        );
        let sep_color = match COLOR_SCHEME {
            ColorScheme::DarkMode => [0.4, 0.4, 0.4, 1.],
            ColorScheme::PaperLight => [0.2, 0.2, 0.2, 1.],
        };
        shape.add_filled_box(
            expr::const_f32(0.),
            expr::const_f32(CHANNEL_LABEL_LINESPACE - 1.),
            expr::load_var("w"),
            expr::const_f32(CHANNEL_LABEL_LINESPACE),
            sep_color,
        );

        let node = node.setup(|me| VectorArt::new(me, shape, app.render_api.clone())).await;
        layer_node.link(node);

        // Desktop platforms
        //#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        //let text = format!("{}. #{channel}", i + 1);
        //// Mobile platforms
        //#[cfg(any(target_os = "android", target_os = "ios"))]
        let text = "#".to_string() + channel;

        // Create some text
        let node = create_text(&(channel.to_string() + "_channel_label"));
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
        prop.set_f32(atom, Role::App, 1, channel_y + CHANNEL_LABEL_Y).unwrap();
        prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
        prop.set_f32(atom, Role::App, 3, 200.).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        node.set_property_f32(atom, Role::App, "font_size", CHANNEL_LABEL_FONTSIZE).unwrap();
        node.set_property_str(atom, Role::App, "text", text).unwrap();
        //node.set_property_bool(atom, Role::App, "debug", true).unwrap();
        //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
        let color_prop = node.get_property("text_color").unwrap();
        let set_normal_color = move |atom: &mut PropertyAtomicGuard| {
            if COLOR_SCHEME == ColorScheme::DarkMode {
                color_prop.set_f32(atom, Role::App, 0, 1.).unwrap();
                color_prop.set_f32(atom, Role::App, 1, 1.).unwrap();
                color_prop.set_f32(atom, Role::App, 2, 1.).unwrap();
                color_prop.set_f32(atom, Role::App, 3, 1.).unwrap();
            } else if COLOR_SCHEME == ColorScheme::PaperLight {
                color_prop.set_f32(atom, Role::App, 0, 0.).unwrap();
                color_prop.set_f32(atom, Role::App, 1, 0.).unwrap();
                color_prop.set_f32(atom, Role::App, 2, 0.).unwrap();
                color_prop.set_f32(atom, Role::App, 3, 1.).unwrap();
            }
        };
        set_normal_color(atom);
        node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

        let node = node
            .setup(|me| {
                Text::new(me, window_scale.clone(), app.render_api.clone(), i18n_fish.clone())
            })
            .await;
        layer_node.link(node);

        // Create the button
        let node = create_button(&(channel.to_string() + "_channel_btn"));
        node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, channel_y).unwrap();
        prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();

        let (slot, recvr) = Slot::new(channel.to_string() + "_clicked");
        node.register("click", slot).unwrap();
        let chatview_path = "/window/content/".to_string() + channel + "_chat_layer";
        let chatview_node = app.sg_root.lookup_node(chatview_path).unwrap();
        let chatview_is_visible =
            PropertyBool::wrap(&chatview_node, Role::App, "is_visible", 0).unwrap();
        let menu_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();

        let render_api = app.render_api.clone();
        let select_channel = move || {
            let atom = &mut render_api.make_guard(gfxtag!("channel_clicked"));
            info!(target: "app::menu", "clicked: {channel}!");
            chatview_is_visible.set(atom, true);
            menu_is_visible.set(atom, false);
            set_normal_color(atom);
        };

        let select_channel2 = select_channel.clone();
        let listen_click = app.ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                select_channel2();
            }
        });
        app.tasks.lock().unwrap().push(listen_click);

        let node = node.setup(Button::new).await;
        layer_node.link(node);

        // Create shortcut
        let channel_id = i + 1;
        let node = create_shortcut(&format!("channel_shortcut_{channel_id}"));
        #[cfg(not(target_os = "macos"))]
        let key = format!("alt+{channel_id}");
        #[cfg(target_os = "macos")]
        let key = format!("logo+{channel_id}");
        node.set_property_str(atom, Role::App, "key", key).unwrap();
        node.set_property_u32(atom, Role::App, "priority", 1).unwrap();

        let (slot, recvr) = Slot::new("back_pressed");
        node.register("shortcut", slot).unwrap();
        let listen_enter = app.ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                select_channel();
            }
        });
        app.tasks.lock().unwrap().push(listen_enter);

        let node = node.setup(|me| Shortcut::new(me)).await;
        layer_node.link(node);

        channel_y += CHANNEL_LABEL_LINESPACE;
    }
    */

    let node = create_vector_art("version_block");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - VERBLOCK_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - VERBLOCK_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let shape = shape::create_version_block([1., 0., 0.25, 1.]).scaled(VERBLOCK_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.render_api.clone())).await;
    layer_node.link(node);
}
