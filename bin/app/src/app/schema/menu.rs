/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
        Button, ChatEdit, ChatView, EditBox, Image, Layer, ShapeVertex, Shortcut, Text, VectorArt,
        VectorShape, Window,
    },
    ExecutorPtr,
};

use super::{ColorScheme, CHANNELS, COLOR_SCHEME};

mod android_ui_consts {
    pub const CHANNEL_LABEL_X: f32 = 40.;
    pub const CHANNEL_LABEL_LINESPACE: f32 = 140.;
    pub const CHANNEL_LABEL_FONTSIZE: f32 = 40.;
    pub const CHANNEL_LABEL_BASELINE: f32 = 82.;
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
    pub const CHANNEL_LABEL_LINESPACE: f32 = 60.;
    pub const CHANNEL_LABEL_FONTSIZE: f32 = 20.;
    pub const CHANNEL_LABEL_BASELINE: f32 = 37.;
}

use ui_consts::*;

pub async fn make(app: &App, window: SceneNodePtr) {
    let window_scale = PropertyFloat32::wrap(&window, Role::Internal, "scale", 0).unwrap();
    let atom = &mut PropertyAtomicGuard::new();

    let mut cc = Compiler::new();

    // Main view
    let layer_node = create_layer("menu_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.link(layer_node.clone());

    let mut channel_y = 0.;

    // Channels label bg
    let node = create_vector_art("channels_label_bg");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, channel_y).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();
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

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create some text
    let node = create_text("channels_label");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, channel_y).unwrap();
    prop.clone().set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.clone().set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", CHANNEL_LABEL_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", CHANNEL_LABEL_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "CHANNELS").unwrap();
    //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.clone().set_f32(atom, Role::App, 0, 0.65).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.87).unwrap();
        prop.clone().set_f32(atom, Role::App, 2, 0.83).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| {
            Text::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.clone().link(node);

    channel_y += CHANNEL_LABEL_LINESPACE;

    for (i, channel) in CHANNELS.iter().enumerate() {
        let node = create_vector_art(&(channel.to_string() + "_channel_label_bg"));
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, channel_y).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

        let mut shape = VectorShape::new();
        let bg_color = match COLOR_SCHEME {
            ColorScheme::DarkMode => [0.05, 0.05, 0.05, 1.],
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

        let node = node
            .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
            .await;
        layer_node.clone().link(node);

        // Desktop platforms
        //#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        //let text = format!("{}. #{channel}", i + 1);
        //// Mobile platforms
        //#[cfg(any(target_os = "android", target_os = "ios"))]
        let text = "#".to_string() + channel;

        // Create some text
        let node = create_text(&(channel.to_string() + "_channel_label"));
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, channel_y).unwrap();
        prop.clone().set_f32(atom, Role::App, 2, 1000.).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, 200.).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        node.set_property_f32(atom, Role::App, "baseline", CHANNEL_LABEL_BASELINE).unwrap();
        node.set_property_f32(atom, Role::App, "font_size", CHANNEL_LABEL_FONTSIZE).unwrap();
        node.set_property_str(atom, Role::App, "text", text).unwrap();
        //node.set_property_bool(atom, Role::App, "debug", true).unwrap();
        //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
        let color_prop = node.get_property("text_color").unwrap();
        let set_normal_color = move |atom: &mut PropertyAtomicGuard| {
            if COLOR_SCHEME == ColorScheme::DarkMode {
                color_prop.clone().set_f32(atom, Role::App, 0, 1.).unwrap();
                color_prop.clone().set_f32(atom, Role::App, 1, 1.).unwrap();
                color_prop.clone().set_f32(atom, Role::App, 2, 1.).unwrap();
                color_prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
            } else if COLOR_SCHEME == ColorScheme::PaperLight {
                color_prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
                color_prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
                color_prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
                color_prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
            }
        };
        set_normal_color(atom);
        node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

        let node = node
            .setup(|me| {
                Text::new(
                    me,
                    window_scale.clone(),
                    app.render_api.clone(),
                    app.text_shaper.clone(),
                    app.ex.clone(),
                )
            })
            .await;
        layer_node.clone().link(node);

        // Create the button
        let node = create_button(&(channel.to_string() + "_channel_btn"));
        node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, channel_y).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();

        let (slot, recvr) = Slot::new(channel.to_string() + "_clicked");
        node.register("click", slot).unwrap();
        let chatview_path = "/window/".to_string() + channel + "_chat_layer";
        let chatview_node = app.sg_root.clone().lookup_node(chatview_path).unwrap();
        let chatview_is_visible =
            PropertyBool::wrap(&chatview_node, Role::App, "is_visible", 0).unwrap();
        let menu_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();

        let select_channel = move || {
            let atom = &mut PropertyAtomicGuard::new();
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

        let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
        layer_node.clone().link(node);

        // Create shortcut
        let channel_id = i + 1;
        let node = create_shortcut(&format!("channel_shortcut_{channel_id}"));
        let key = format!("alt+{channel_id}");
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
        layer_node.clone().link(node);

        channel_y += CHANNEL_LABEL_LINESPACE;
    }
}
