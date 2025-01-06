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

use darkfi::system::msleep;
use sled_overlay::sled;

use crate::{
    app::{
        node::{
            create_button, create_chatedit, create_chatview, create_editbox, create_emoji_picker,
            create_image, create_layer, create_text, create_vector_art,
        },
        populate_tree, App,
    },
    error::Error,
    expr::{self, Compiler, Op},
    gfx::{GraphicsEventPublisherPtr, Point, Rectangle, RenderApi, Vertex},
    mesh::{Color, MeshBuilder},
    prop::{
        Property, PropertyBool, PropertyFloat32, PropertyStr, PropertySubType, PropertyType, Role,
    },
    scene::{SceneNodePtr, Slot},
    shape,
    text::TextShaperPtr,
    ui::{
        emoji_picker, Button, ChatEdit, ChatView, EditBox, EmojiPicker, Image, Layer, ShapeVertex,
        Text, VectorArt, VectorShape, Window,
    },
    ExecutorPtr,
};

const LIGHTMODE: bool = false;

mod android_ui_consts {
    use crate::gfx::{Point, Rectangle};

    pub const CHANNEL_LABEL_BASELINE: f32 = 82.;
    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 50.;
    pub const BACKARROW_Y: f32 = 70.;
    pub const CHATEDIT_MIN_HEIGHT: f32 = 140.;
    pub const CHATEDIT_HEIGHT: f32 = 140.;
    pub const CHATEDIT_SINGLE_LINE_Y: f32 = 120.;
    pub const CHATEDIT_BOTTOM_PAD: f32 = 10.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = 50.;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = 20.;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 50.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 20.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 10.;
    pub const CHATEDIT_LINESPACING: f32 = 70.;
    pub const CHATEDIT_NEG_W: f32 = 280.;
    pub const CHATEDIT_LHS_PAD: f32 = 150.;
    pub const TEXTBAR_BASELINE: f32 = 60.;
    pub const TEXT_DESCENT: f32 = 20.;
    pub const EMOJI_BTN_X: f32 = 60.;
    pub const EMOJI_BG_W: f32 = 120.;
    pub const EMOJI_SCALE: f32 = 40.;
    pub const EMOJI_NEG_Y: f32 = 85.;
    pub const EMOJIBTN_BOX: [f32; 4] = [20., 118., 80., 75.];
    pub const EMOJI_CLOSE_SCALE: f32 = 20.;
    pub const SENDARROW_NEG_X: f32 = 50.;
    pub const SENDARROW_NEG_Y: f32 = 80.;
    pub const SENDBTN_BOX: [f32; 4] = [86., 120., 80., 70.];
    pub const FONTSIZE: f32 = 40.;
    pub const TIMESTAMP_FONTSIZE: f32 = 30.;
    pub const TIMESTAMP_WIDTH: f32 = 135.;
    pub const MESSAGE_SPACING: f32 = 15.;
    pub const LINE_HEIGHT: f32 = 58.;
    pub const CHATVIEW_BASELINE: f32 = 36.;

    pub const ACTION_POPUP_Y_OFF: f32 = 200.;
    pub const ACTION_COPY_RECT: Rectangle = Rectangle::new(0., 0., 200., 160.);
    pub const ACTION_PASTE_RECT: Rectangle = Rectangle::new(220., 0., 240., 160.);
    pub const ACTION_SELECT_ALL_RECT: Rectangle = Rectangle::new(480., 0., 400., 160.);
    pub const ACTION_LABEL_POS: Point = Point::new(40., 92.);
}

#[cfg(target_os = "android")]
mod ui_consts {
    pub const CHATDB_PATH: &str = "/data/data/darkfi.darkwallet/chatdb/";
    pub const BG_PATH: &str = "bg.png";
    pub use super::android_ui_consts::*;
}

#[cfg(feature = "emulate-android")]
mod ui_consts {
    pub const CHATDB_PATH: &str = "chatdb";
    pub const BG_PATH: &str = "assets/bg.png";
    pub use super::android_ui_consts::*;
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    use crate::gfx::{Point, Rectangle};

    pub const CHATDB_PATH: &str = "chatdb";
    pub const BG_PATH: &str = "assets/bg.png";

    // Main menu

    // Chat UI
    pub const CHANNEL_LABEL_BASELINE: f32 = 37.;
    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 26.;
    pub const CHATEDIT_MIN_HEIGHT: f32 = 60.;
    pub const CHATEDIT_HEIGHT: f32 = 60.;
    pub const CHATEDIT_SINGLE_LINE_Y: f32 = 58.;
    pub const CHATEDIT_BOTTOM_PAD: f32 = 5.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = 25.;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = 8.;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 30.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 10.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 35.;
    pub const CHATEDIT_LINESPACING: f32 = 35.;
    pub const CHATEDIT_NEG_W: f32 = 190.;
    pub const CHATEDIT_LHS_PAD: f32 = 100.;
    pub const TEXTBAR_BASELINE: f32 = 34.;
    pub const TEXT_DESCENT: f32 = 10.;
    pub const EMOJI_BTN_X: f32 = 38.;
    pub const EMOJI_BG_W: f32 = 80.;
    pub const EMOJI_SCALE: f32 = 20.;
    pub const EMOJI_NEG_Y: f32 = 34.;
    pub const EMOJIBTN_BOX: [f32; 4] = [16., 50., 44., 36.];
    pub const EMOJI_CLOSE_SCALE: f32 = 10.;
    pub const SENDARROW_NEG_X: f32 = 50.;
    pub const SENDARROW_NEG_Y: f32 = 32.;
    pub const SENDBTN_BOX: [f32; 4] = [72., 50., 45., 34.];
    pub const FONTSIZE: f32 = 20.;
    pub const TIMESTAMP_FONTSIZE: f32 = 12.;
    pub const TIMESTAMP_WIDTH: f32 = 60.;
    pub const MESSAGE_SPACING: f32 = 5.;
    pub const LINE_HEIGHT: f32 = 30.;
    pub const CHATVIEW_BASELINE: f32 = 20.;

    pub const ACTION_POPUP_Y_OFF: f32 = 100.;
    pub const ACTION_COPY_RECT: Rectangle = Rectangle::new(0., 0., 100., 80.);
    pub const ACTION_PASTE_RECT: Rectangle = Rectangle::new(110., 0., 120., 80.);
    pub const ACTION_SELECT_ALL_RECT: Rectangle = Rectangle::new(240., 0., 200., 80.);
    pub const ACTION_LABEL_POS: Point = Point::new(20., 46.);
}

use super::EMOJI_PICKER_ICON_SIZE;
use ui_consts::*;

pub async fn make(
    app: &App,
    window: SceneNodePtr,
    channel: &str,
    db: &sled::Db,
    emoji_meshes: emoji_picker::EmojiMeshesPtr,
) {
    let window_scale = PropertyFloat32::wrap(&window, Role::Internal, "scale", 0).unwrap();

    let mut cc = Compiler::new();

    cc.add_const_f32("CHATEDIT_HEIGHT", CHATEDIT_HEIGHT);
    cc.add_const_f32("CHATEDIT_SINGLE_LINE_Y", CHATEDIT_SINGLE_LINE_Y);
    cc.add_const_f32("CHATEDIT_BOTTOM_PAD", CHATEDIT_BOTTOM_PAD);
    cc.add_const_f32("CHATEDIT_NEG_W", CHATEDIT_NEG_W);
    cc.add_const_f32("CHATEDIT_MIN_HEIGHT", CHATEDIT_MIN_HEIGHT);
    cc.add_const_f32("SENDARROW_NEG_X", SENDARROW_NEG_X);
    cc.add_const_f32("SENDARROW_NEG_Y", SENDARROW_NEG_Y);
    cc.add_const_f32("EMOJI_NEG_Y", EMOJI_NEG_Y);
    cc.add_const_f32("EMOJIBTN_BOX_1", EMOJIBTN_BOX[1]);
    cc.add_const_f32("SENDBTN_BOX_0", SENDBTN_BOX[0]);
    cc.add_const_f32("SENDBTN_BOX_1", SENDBTN_BOX[1]);
    cc.add_const_f32("ACTION_POPUP_Y_OFF", ACTION_POPUP_Y_OFF);

    // Main view
    let layer_node = create_layer(&(channel.to_string() + "_chat_layer"));
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(Role::App, "is_visible", false).unwrap();
    layer_node.set_property_u32(Role::App, "z_index", 1).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.link(layer_node.clone());

    // Create the toolbar bg
    let node = create_vector_art("toolbar_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(EMOJI_BG_W),
        expr::load_var("h"),
        [0., 0.11, 0.11, 1.],
    );
    shape.add_filled_box(
        expr::const_f32(EMOJI_BG_W),
        expr::const_f32(0.),
        expr::const_f32(EMOJI_BG_W + 1.),
        expr::load_var("h"),
        [0.41, 0.6, 0.65, 1.],
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::load_var("h"),
        expr::load_var("w"),
        cc.compile("h + 1").unwrap(),
        [0.41, 0.6, 0.65, 1.],
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the send button
    let node = create_vector_art("back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(Role::App, 2, 500.).unwrap();
    prop.set_f32(Role::App, 3, 500.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();

    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the back button
    let node = create_button("back_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_f32(Role::App, 2, EMOJI_BG_W).unwrap();
    prop.set_f32(Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    let (slot, recvr) = Slot::new("back_clicked");
    node.register("click", slot).unwrap();
    // Menu doesn't exist yet ;)
    let sg_root = app.sg_root.clone();
    let chatview_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked back");

            let menu_node = sg_root.clone().lookup_node("/window/menu_layer").unwrap();
            menu_node.set_property_bool(Role::App, "is_visible", true).unwrap();

            chatview_is_visible.set(false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create some text
    let node = create_text("channel_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, CHATEDIT_LHS_PAD).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_f32(Role::App, "baseline", CHANNEL_LABEL_BASELINE).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(Role::App, "text", &("#".to_string() + channel)).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();
    //node.set_property_str(Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(Role::App, 0, 1.).unwrap();
    prop.set_f32(Role::App, 1, 1.).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();

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

    // Create the emoji picker
    let mut node = create_emoji_picker("emoji_picker");
    let prop = Property::new("dynamic_h", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();
    let emoji_dynamic_h_prop = node.get_property("dynamic_h").unwrap();
    //emoji_dynamic_h_prop.set_f32(Role::App, 0, 400.).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code = cc.compile("h - dynamic_h").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(Role::App, 3, expr::load_var("dynamic_h")).unwrap();
    prop.add_depend(&emoji_dynamic_h_prop, 0, "dynamic_h");
    let emoji_h_prop = PropertyFloat32::wrap(&node, Role::App, "dynamic_h", 0).unwrap();
    //node.set_property_f32(Role::App, "baseline", CHANNEL_LABEL_BASELINE).unwrap();
    //node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_f32(Role::App, "emoji_size", EMOJI_PICKER_ICON_SIZE).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();
    let node = node
        .setup(|me| {
            EmojiPicker::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                emoji_meshes,
                app.ex.clone(),
            )
        })
        .await;
    let emoji_picker_node = node.clone();
    layer_node.clone().link(node);

    // Main content view
    let chat_layer_node = layer_node;
    let layer_node = create_layer("content");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h - emoji_h").unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();
    prop.add_depend(&emoji_dynamic_h_prop, 0, "emoji_h");
    layer_node.set_property_bool(Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(Role::App, "z_index", 1).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    chat_layer_node.clone().link(layer_node.clone());

    // ChatView
    let node = create_chatview("chatty");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 10.).unwrap();
    prop.set_f32(Role::App, 1, CHATEDIT_HEIGHT).unwrap();
    let code = cc.compile("w - 30").unwrap();
    prop.set_expr(Role::App, 2, code).unwrap();
    let code = cc
        .compile(
            "
        height = if editz_h < CHATEDIT_MIN_HEIGHT {
            CHATEDIT_MIN_HEIGHT
        } else {
            editz_h
        };

        h - CHATEDIT_HEIGHT - height - 2 * CHATEDIT_BOTTOM_PAD
",
        )
        .unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();
    let chatview_rect_prop = prop.clone();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_f32(Role::App, "timestamp_font_size", TIMESTAMP_FONTSIZE).unwrap();
    node.set_property_f32(Role::App, "timestamp_width", TIMESTAMP_WIDTH).unwrap();
    node.set_property_f32(Role::App, "line_height", LINE_HEIGHT).unwrap();
    node.set_property_f32(Role::App, "message_spacing", MESSAGE_SPACING).unwrap();
    node.set_property_f32(Role::App, "baseline", CHATVIEW_BASELINE).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    #[cfg(target_os = "android")]
    node.set_property_f32(Role::App, "scroll_start_accel", 40.).unwrap();
    #[cfg(target_os = "linux")]
    node.set_property_f32(Role::App, "scroll_start_accel", 15.).unwrap();

    node.set_property_f32(Role::App, "scroll_resist", 0.9).unwrap();

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

    let prop = node.get_property("hi_bg_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }

    let tree_name = channel.to_string() + "__chat_tree";
    let chat_tree = db.open_tree(tree_name.as_bytes()).unwrap();
    if chat_tree.is_empty() {
        populate_tree(&chat_tree);
    }
    debug!(target: "app", "db has {} lines", chat_tree.len());
    let node = node
        .setup(|me| {
            ChatView::new(
                me,
                chat_tree,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.clone().link(node);

    // Create the editbox bg
    let node = create_vector_art("editbox_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    let code = cc
        .compile(
            "
        height = if editz_h < CHATEDIT_MIN_HEIGHT {
            CHATEDIT_MIN_HEIGHT
        } else {
            editz_h
        };

        h - height - 2 * CHATEDIT_BOTTOM_PAD
",
        )
        .unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_expr(Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc
        .compile(
            "
        height = if editz_h < CHATEDIT_MIN_HEIGHT {
            CHATEDIT_MIN_HEIGHT
        } else {
            editz_h
        };

        height + 2 * CHATEDIT_BOTTOM_PAD
",
        )
        .unwrap();
    prop.set_expr(Role::App, 3, code).unwrap();
    node.set_property_u32(Role::App, "z_index", 2).unwrap();

    let editbox_bg_rect_prop = prop.clone();

    let mut shape = VectorShape::new();
    // Main green background
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 0.13, 0.08, 1.],
    );
    // Left hand darker box
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(1.),
        expr::const_f32(EMOJI_BG_W),
        expr::load_var("h"),
        [0., 0.11, 0.11, 1.],
    );
    // Top line
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(1.),
        [0.41, 0.6, 0.65, 1.],
    );
    // Side line
    shape.add_filled_box(
        expr::const_f32(EMOJI_BG_W),
        expr::const_f32(0.),
        expr::const_f32(EMOJI_BG_W + 1.),
        expr::load_var("h"),
        [0.41, 0.6, 0.65, 1.],
    );
    // Bottom line
    shape.add_filled_box(
        expr::const_f32(0.),
        cc.compile("h - 1").unwrap(),
        expr::load_var("w"),
        expr::load_var("h"),
        [0.41, 0.6, 0.65, 1.],
    );
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the send button
    let node = create_vector_art("send_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - SENDARROW_NEG_X").unwrap();
    prop.set_expr(Role::App, 0, code).unwrap();
    let code = cc.compile("h - SENDARROW_NEG_Y").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, 500.).unwrap();
    prop.set_f32(Role::App, 3, 500.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();
    let shape = shape::create_send_arrow().scaled(EMOJI_SCALE);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the emoji button
    let node = create_vector_art("emoji_btn_bg");
    let emoji_btn_is_visible = PropertyBool::wrap(&node, Role::App, "is_visible", 0).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, EMOJI_BTN_X).unwrap();
    let code = cc.compile("h - EMOJI_NEG_Y").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, 500.).unwrap();
    prop.set_f32(Role::App, 3, 500.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();
    let shape = shape::create_emoji_selector().scaled(EMOJI_SCALE);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the emoji button
    let node = create_vector_art("emoji_close_btn_bg");
    node.set_property_bool(Role::App, "is_visible", false).unwrap();
    let prop = node.get_property("rect").unwrap();
    let emoji_close_is_visible = PropertyBool::wrap(&node, Role::App, "is_visible", 0).unwrap();
    prop.set_f32(Role::App, 0, EMOJI_BTN_X).unwrap();
    let code = cc.compile("h - EMOJI_NEG_Y").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, 500.).unwrap();
    prop.set_f32(Role::App, 3, 500.).unwrap();
    node.set_property_u32(Role::App, "z_index", 3).unwrap();
    let shape = shape::create_close_icon().scaled(EMOJI_CLOSE_SCALE);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Text edit
    let node = create_chatedit("editz");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    node.set_property_bool(Role::App, "is_focused", true).unwrap();

    node.set_property_f32(Role::App, "max_height", 300.).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, CHATEDIT_LHS_PAD).unwrap();
    let code = cc
        .compile(
            "
        height = if rect_h < CHATEDIT_MIN_HEIGHT {
            CHATEDIT_SINGLE_LINE_Y
        } else {
            rect_h
        };

        parent_h - (height + CHATEDIT_BOTTOM_PAD)
",
        )
        .unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    let code = cc.compile("parent_w - CHATEDIT_NEG_W").unwrap();
    prop.set_expr(Role::App, 2, code).unwrap();
    prop.set_f32(Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    chatview_rect_prop.add_depend(&prop, 3, "editz_h");
    editbox_bg_rect_prop.add_depend(&prop, 3, "editz_h");

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, TEXTBAR_BASELINE / 2.).unwrap();

    node.set_property_f32(Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(Role::App, "linespacing", CHATEDIT_LINESPACING).unwrap();
    node.set_property_f32(Role::App, "descent", TEXT_DESCENT).unwrap();
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
    let prop = node.get_property("text_hi_color").unwrap();
    prop.set_f32(Role::App, 0, 0.44).unwrap();
    prop.set_f32(Role::App, 1, 0.96).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(Role::App, 0, 0.816).unwrap();
    prop.set_f32(Role::App, 1, 0.627).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_f32(Role::App, "cursor_ascent", CHATEDIT_CURSOR_ASCENT).unwrap();
    node.set_property_f32(Role::App, "cursor_descent", CHATEDIT_CURSOR_DESCENT).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    node.set_property_f32(Role::App, "select_ascent", CHATEDIT_SELECT_ASCENT).unwrap();
    node.set_property_f32(Role::App, "select_descent", CHATEDIT_SELECT_DESCENT).unwrap();
    node.set_property_f32(Role::App, "handle_descent", CHATEDIT_HANDLE_DESCENT).unwrap();
    if LIGHTMODE {
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.27).unwrap();
        prop.set_f32(Role::App, 2, 0.22).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(Role::App, "z_index", 3).unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();

    let editz_select_text_prop = node.get_property("select_text").unwrap();

    //let editbox_text = PropertyStr::wrap(node, Role::App, "text", 0).unwrap();
    //let editbox_focus = PropertyBool::wrap(node, Role::App, "is_focused", 0).unwrap();
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

    let node = node
        .setup(|me| {
            ChatEdit::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    let chatedit_node = node.clone();
    layer_node.clone().link(node);

    let (slot, recvr) = Slot::new("emoji_selected");
    emoji_picker_node.register("emoji_select", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            // No need to decode the data. Just pass it straight along
            chatedit_node.call_method("insert_text", data).await.unwrap();
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Create the send button
    let node = create_button("send_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - SENDBTN_BOX_0").unwrap();
    prop.set_expr(Role::App, 0, code).unwrap();
    let code = cc.compile("h - SENDBTN_BOX_1").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, SENDBTN_BOX[2]).unwrap();
    prop.set_f32(Role::App, 3, SENDBTN_BOX[3]).unwrap();

    let (slot, recvr) = Slot::new("send_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked send");
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create the emoji button
    let node = create_button("emoji_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, EMOJIBTN_BOX[0]).unwrap();
    let code = cc.compile("h - EMOJIBTN_BOX_1").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, EMOJIBTN_BOX[2]).unwrap();
    prop.set_f32(Role::App, 3, EMOJIBTN_BOX[3]).unwrap();

    let (slot, recvr) = Slot::new("emoji_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked emoji");
            if emoji_btn_is_visible.get() {
                assert!(!emoji_close_is_visible.get());
                assert!(emoji_h_prop.get() < 0.001);
                emoji_btn_is_visible.set(false);
                emoji_close_is_visible.set(true);
                for i in 1..=20 {
                    emoji_h_prop.set((20 * i) as f32);
                    msleep(10).await;
                }
            } else {
                assert!(emoji_close_is_visible.get());
                assert!(emoji_h_prop.get() > 0.);
                emoji_btn_is_visible.set(true);
                emoji_close_is_visible.set(false);
                for i in 1..=20 {
                    emoji_h_prop.set((400 - 20 * i) as f32);
                    msleep(10).await;
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create debug box
    /*
    let mut node = create_vector_art("debugtool");
    let mut prop = Property::new("hoff", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();
    node.set_property_f32(Role::App, "hoff", 40.).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, EMOJIBTN_BOX[0]).unwrap();
    let code = cc.compile("h - EMOJIBTN_BOX_1").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, EMOJIBTN_BOX[2]).unwrap();
    prop.set_f32(Role::App, 3, EMOJIBTN_BOX[3]).unwrap();
    let hoff_prop = node.get_property("hoff").unwrap();
    prop.add_depend(&hoff_prop, 0, "hoff");
    node.set_property_u32(Role::App, "z_index", 6).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        2.,
        [1., 0., 0., 1.],
    );
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);
    */

    // Overlay popup
    let layer_node = create_layer("overlay");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 40.).unwrap();
    let code = cc.compile("editz_bg_top_y - ACTION_POPUP_Y_OFF").unwrap();
    //let code = cc.compile("h - 60 - 80").unwrap();
    //let code = cc.compile("h - 60 - 300").unwrap();
    prop.set_expr(Role::App, 1, code).unwrap();
    prop.set_f32(Role::App, 2, ACTION_SELECT_ALL_RECT.rhs()).unwrap();
    prop.set_f32(Role::App, 3, ACTION_SELECT_ALL_RECT.h).unwrap();
    prop.add_depend(&editbox_bg_rect_prop, 1, "editz_bg_top_y");
    layer_node.set_property_bool(Role::App, "is_visible", false).unwrap();
    layer_node.set_property_u32(Role::App, "z_index", 2).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    chat_layer_node.link(layer_node.clone());

    let actions_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();

    // Create the actionbar bg
    let node = create_vector_art("actionbar_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.).unwrap();
    prop.set_f32(Role::App, 2, ACTION_SELECT_ALL_RECT.rhs()).unwrap();
    prop.set_f32(Role::App, 3, ACTION_SELECT_ALL_RECT.h).unwrap();
    node.set_property_u32(Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let color1 = [0., 0., 0., 0.4];
    let color2 = [0., 0., 0., 0.9];
    let gradient = [color1.clone(), color1, color2.clone(), color2];
    let hicolor = [0., 0.94, 1., 1.];

    // Copy box
    shape.add_gradient_box(
        expr::const_f32(ACTION_COPY_RECT.x),
        expr::const_f32(ACTION_COPY_RECT.y),
        expr::const_f32(ACTION_COPY_RECT.rhs()),
        expr::const_f32(ACTION_COPY_RECT.bhs()),
        gradient.clone(),
    );

    // Copy outline
    shape.add_outline(
        expr::const_f32(ACTION_COPY_RECT.x),
        expr::const_f32(ACTION_COPY_RECT.y),
        expr::const_f32(ACTION_COPY_RECT.rhs()),
        expr::const_f32(ACTION_COPY_RECT.bhs()),
        1.,
        hicolor.clone(),
    );

    // Paste box
    shape.add_gradient_box(
        expr::const_f32(ACTION_PASTE_RECT.x),
        expr::const_f32(ACTION_PASTE_RECT.y),
        expr::const_f32(ACTION_PASTE_RECT.rhs()),
        expr::const_f32(ACTION_PASTE_RECT.bhs()),
        gradient.clone(),
    );

    // Paste outline
    shape.add_outline(
        expr::const_f32(ACTION_PASTE_RECT.x),
        expr::const_f32(ACTION_PASTE_RECT.y),
        expr::const_f32(ACTION_PASTE_RECT.rhs()),
        expr::const_f32(ACTION_PASTE_RECT.bhs()),
        1.,
        hicolor.clone(),
    );

    // Select all box
    shape.add_gradient_box(
        expr::const_f32(ACTION_SELECT_ALL_RECT.x),
        expr::const_f32(ACTION_SELECT_ALL_RECT.y),
        expr::const_f32(ACTION_SELECT_ALL_RECT.rhs()),
        expr::const_f32(ACTION_SELECT_ALL_RECT.bhs()),
        gradient,
    );

    // Select all outline
    shape.add_outline(
        expr::const_f32(ACTION_SELECT_ALL_RECT.x),
        expr::const_f32(ACTION_SELECT_ALL_RECT.y),
        expr::const_f32(ACTION_SELECT_ALL_RECT.rhs()),
        expr::const_f32(ACTION_SELECT_ALL_RECT.bhs()),
        1.,
        hicolor,
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);

    // Create some text
    let node = create_text("actions_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, ACTION_LABEL_POS.x).unwrap();
    prop.set_f32(Role::App, 1, ACTION_LABEL_POS.y).unwrap();
    prop.set_f32(Role::App, 2, ACTION_SELECT_ALL_RECT.rhs()).unwrap();
    prop.set_f32(Role::App, 3, ACTION_SELECT_ALL_RECT.h).unwrap();
    node.set_property_f32(Role::App, "baseline", 0.).unwrap();
    node.set_property_f32(Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(Role::App, "text", "copy   paste   select all").unwrap();
    //node.set_property_bool(Role::App, "debug", true).unwrap();
    //node.set_property_str(Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(Role::App, 0, 0.).unwrap();
    prop.set_f32(Role::App, 1, 0.94).unwrap();
    prop.set_f32(Role::App, 2, 1.).unwrap();
    prop.set_f32(Role::App, 3, 1.).unwrap();
    node.set_property_u32(Role::App, "z_index", 1).unwrap();

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

    // Copy button
    let node = create_button("copy_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, ACTION_COPY_RECT.x).unwrap();
    prop.set_f32(Role::App, 1, ACTION_COPY_RECT.y).unwrap();
    prop.set_f32(Role::App, 2, ACTION_COPY_RECT.w).unwrap();
    prop.set_f32(Role::App, 3, ACTION_COPY_RECT.h).unwrap();

    let (slot, recvr) = Slot::new("copy_clicked");
    node.register("click", slot).unwrap();
    let actions_is_visible2 = actions_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked copy");
            actions_is_visible2.set(false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);

    // Paste button
    let node = create_button("paste_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, ACTION_PASTE_RECT.x).unwrap();
    prop.set_f32(Role::App, 1, ACTION_PASTE_RECT.y).unwrap();
    prop.set_f32(Role::App, 2, ACTION_PASTE_RECT.w).unwrap();
    prop.set_f32(Role::App, 3, ACTION_PASTE_RECT.h).unwrap();

    let (slot, recvr) = Slot::new("paste_clicked");
    node.register("click", slot).unwrap();
    let actions_is_visible2 = actions_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked paste");
            actions_is_visible2.set(false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);

    // Select all button
    let node = create_button("select_all_btn");
    node.set_property_bool(Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(Role::App, 0, ACTION_SELECT_ALL_RECT.x).unwrap();
    prop.set_f32(Role::App, 1, ACTION_SELECT_ALL_RECT.y).unwrap();
    prop.set_f32(Role::App, 2, ACTION_SELECT_ALL_RECT.w).unwrap();
    prop.set_f32(Role::App, 3, ACTION_SELECT_ALL_RECT.h).unwrap();

    let (slot, recvr) = Slot::new("select_all_clicked");
    node.register("click", slot).unwrap();
    let actions_is_visible2 = actions_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked select_all");
            actions_is_visible2.set(false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.clone().link(node);

    let editz_select_sub = editz_select_text_prop.subscribe_modify();
    let editz_select_task = app.ex.spawn(async move {
        while let Ok(_) = editz_select_sub.receive().await {
            if editz_select_text_prop.is_null(0).unwrap() {
                info!(target: "app::chat", "selection changed: null");
                actions_is_visible.set(false);
            } else {
                let select_text = editz_select_text_prop.get_str(0).unwrap();
                info!(target: "app::chat", "selection changed: {select_text}");
                actions_is_visible.set(true);
            }
        }
    });
    app.tasks.lock().unwrap().push(editz_select_task);
}
