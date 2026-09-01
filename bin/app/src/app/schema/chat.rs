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

use darkfi_serial::{Decodable, Encodable};
#[cfg(feature = "enable-plugin-darkirc")]
use irc2::Privmsg;
use kvdb_overlay::{Database as KvDb, Tree};
use std::{sync::Arc, time::UNIX_EPOCH};

#[cfg(feature = "enable-plugin-darkirc")]
use crate::plugin::darkirc;
use crate::{
    app::{
        node::{
            create_button, create_chatview, create_emoji_picker, create_layer,
            create_multiline_edit, create_shortcut, create_text, create_vector_art,
        },
        App,
    },
    expr::{self, Compiler},
    gfx::{gfxtag, Point, Renderer},
    mesh::{rgba, COLOR_RED, COLOR_WHITE},
    prop::{
        Property, PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyStr, PropertySubType,
        PropertyType, Role,
    },
    scene::{Pimpl, SceneNodePtr, Slot},
    shape,
    ui::{
        chatview::MessageId, emoji_picker, BaseEdit, BaseEditType, Button, ChatView, EmojiPicker,
        Layer, RedrawTrigger, Shortcut, Text, VectorArt, VectorShape,
    },
    util::{i18n::I18nBabelFish, unixtime},
    ExecutorPtr,
};

use std::io::Cursor;

use url::Url;

use super::{ColorScheme, COLOR_SCHEME};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const CHANNEL_LABEL_X: f32 = 180.;
    pub const CHANNEL_LABEL_Y: f32 = 30.;
    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 70.;
    pub const BACKARROW_Y: f32 = 70.;
    pub const BACKARROW_BG_W: f32 = 140.;
    pub const CHATEDIT_MIN_HEIGHT: f32 = 160.;
    pub const CHATEDIT_MAX_HEIGHT: f32 = 500.;
    pub const CHATEDIT_HEIGHT: f32 = 140.;
    pub const CHATEDIT_SINGLE_LINE_Y: f32 = 120.;
    pub const CHATEDIT_BOTTOM_PAD: f32 = 10.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = 50.;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = 20.;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 50.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 20.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 10.;
    pub const CHATEDIT_NEG_W: f32 = 300.;
    pub const CHATEDIT_LHS_PAD: f32 = 150.;
    pub const TEXTBAR_BASELINE: f32 = 40.;
    pub const EMOJI_BTN_X: f32 = 60.;
    pub const EMOJI_BG_W: f32 = 120.;
    pub const EMOJI_SCALE: f32 = 40.;
    pub const EMOJI_NEG_Y: f32 = 85.;
    pub const EMOJIBTN_BOX: [f32; 4] = [20., 118., 80., 75.];
    pub const EMOJI_CLOSE_SCALE: f32 = 20.;
    pub const SENDARROW_NEG_X: f32 = 80.;
    pub const SENDARROW_NEG_Y: f32 = 80.;
    pub const SENDBTN_BOX: [f32; 4] = [116., 120., 80., 70.];
    pub const FONTSIZE: f32 = 50.;
    pub const TIMESTAMP_FONTSIZE: f32 = 30.;
    pub const TIMESTAMP_WIDTH: f32 = 135.;
    pub const MESSAGE_SPACING: f32 = 15.;
    pub const LINE_HEIGHT: f32 = 58.;
    pub const CHATVIEW_BASELINE: f32 = 36.;
    pub const CHATVIEW_DATE_FONTSIZE: f32 = 32.;

    pub const CMD_HELP_HEIGHT: f32 = 110.;
    pub const CMD_HELP_GAP: f32 = 10.;
    pub const CMD_HELP_CMD_FONTSIZE: f32 = 48.;
    pub const CMD_HELP_CMD_LABEL_X_INSET: f32 = 40.;
    pub const CMD_HELP_NICK_CMD_WIDTH: f32 = 280.;
    pub const CMD_HELP_NICK_DESC_WIDTH: f32 = 1000.;
    pub const CMD_HELP_NICK_DESC_X: f32 = 320.;
    pub const CMD_HELP_LABEL_Y: f32 = 20.;

    // Action menu
    pub const ACTION_PADDING: f32 = 32.;
    pub const ACTION_SPACING: f32 = 8.;
    pub const BACK_SEP_W: f32 = 1.;

    pub const COPY_BTN_SCALE: f32 = 40.;
    pub const COPY_BTN_X_OFF: f32 = 60.;
    pub const COPY_BTN_Y: f32 = 70.;

    pub const SELECT_CLOSE_X: f32 = 70.;
    pub const SELECT_CLOSE_Y: f32 = 70.;
    pub const SELECT_CLOSE_SCALE: f32 = 20.;

    // Down arrow (scroll to bottom) overlay
    pub const DOWNARROW_NEG_X: f32 = 240.;
    pub const DOWNARROW_NEG_Y: f32 = 450.;
    pub const DOWNARROW_W: f32 = 200.;
    pub const DOWNARROW_H: f32 = 200.;
    pub const DOWNBG_X: f32 = 100.;
    pub const DOWNBG_Y: f32 = 100.;
    pub const DOWNARROW_SCALE: f32 = 100.;
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
    // Chat UI
    pub const CHANNEL_LABEL_X: f32 = 100.;
    pub const CHANNEL_LABEL_Y: f32 = 12.;
    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 30.;
    pub const BACKARROW_BG_W: f32 = 80.;
    pub const CHATEDIT_MIN_HEIGHT: f32 = 60.;
    pub const CHATEDIT_MAX_HEIGHT: f32 = 600.;
    pub const CHATEDIT_HEIGHT: f32 = 60.;
    pub const CHATEDIT_SINGLE_LINE_Y: f32 = 58.;
    pub const CHATEDIT_BOTTOM_PAD: f32 = 5.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = 25.;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = 8.;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 30.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 10.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 35.;
    pub const CHATEDIT_NEG_W: f32 = 190.;
    pub const CHATEDIT_LHS_PAD: f32 = 100.;
    pub const TEXTBAR_BASELINE: f32 = 25.;
    pub const EMOJI_BTN_X: f32 = 38.;
    pub const EMOJI_BG_W: f32 = 80.;
    pub const EMOJI_SCALE: f32 = 20.;
    pub const EMOJI_NEG_Y: f32 = 34.;
    pub const EMOJIBTN_BOX: [f32; 4] = [16., 50., 44., 36.];
    pub const EMOJI_CLOSE_SCALE: f32 = 10.;
    pub const SENDARROW_NEG_X: f32 = 50.;
    pub const SENDARROW_NEG_Y: f32 = 32.;
    pub const SENDBTN_BOX: [f32; 4] = [72., 50., 45., 34.];
    pub const FONTSIZE: f32 = 25.;
    pub const TIMESTAMP_FONTSIZE: f32 = 12.;
    pub const TIMESTAMP_WIDTH: f32 = 60.;
    pub const MESSAGE_SPACING: f32 = 5.;
    pub const LINE_HEIGHT: f32 = 30.;
    pub const CHATVIEW_BASELINE: f32 = 20.;
    pub const CHATVIEW_DATE_FONTSIZE: f32 = 16.;

    pub const CMD_HELP_HEIGHT: f32 = 55.;
    pub const CMD_HELP_GAP: f32 = 5.;
    pub const CMD_HELP_CMD_FONTSIZE: f32 = 24.;
    pub const CMD_HELP_CMD_LABEL_X_INSET: f32 = 20.;
    pub const CMD_HELP_NICK_CMD_WIDTH: f32 = 140.;
    pub const CMD_HELP_NICK_DESC_WIDTH: f32 = 500.;
    pub const CMD_HELP_NICK_DESC_X: f32 = 160.;
    pub const CMD_HELP_LABEL_Y: f32 = 10.;

    // Action menu
    pub const ACTION_PADDING: f32 = 8.;
    pub const ACTION_SPACING: f32 = 4.;
    pub const BACK_SEP_W: f32 = 0.5;

    pub const COPY_BTN_SCALE: f32 = 20.;
    pub const COPY_BTN_X_OFF: f32 = 30.;
    pub const COPY_BTN_Y: f32 = 27.;

    pub const SELECT_CLOSE_X: f32 = 40.;
    pub const SELECT_CLOSE_Y: f32 = 30.;
    pub const SELECT_CLOSE_SCALE: f32 = 10.;

    // Down arrow (scroll to bottom) overlay
    pub const DOWNARROW_NEG_X: f32 = 120.;
    pub const DOWNARROW_NEG_Y: f32 = 250.;
    pub const DOWNARROW_W: f32 = 100.;
    pub const DOWNARROW_H: f32 = 100.;
    pub const DOWNBG_X: f32 = 50.;
    pub const DOWNBG_Y: f32 = 50.;
    pub const DOWNARROW_SCALE: f32 = 50.;
}

use super::{EMOJI_PICKER_ICON_MARGIN_X, EMOJI_PICKER_ICON_MARGIN_Y, EMOJI_PICKER_ICON_SIZE};
use ui_consts::*;

/// Height the keyboard occupies, in virtual units. Android reports it in
/// physical pixels, so divide by the window scale like every other inset.
fn android_keyboard_height(window_scale: f32) -> f32 {
    #[cfg(target_os = "android")]
    return crate::android::get_keyboard_height() as f32 / window_scale;

    #[cfg(not(target_os = "android"))]
    {
        let _ = window_scale;
        unreachable!()
    }
}

fn is_ime_visible() -> bool {
    #[cfg(target_os = "android")]
    return crate::android::is_ime_visible();

    #[cfg(not(target_os = "android"))]
    false
}

pub async fn make(
    sg_root: &SceneNodePtr,
    renderer: &Renderer,
    ex: &ExecutorPtr,
    content: SceneNodePtr,
    kv_db: &KvDb,
    i18n_fish: &I18nBabelFish,
    emoji_meshes: emoji_picker::EmojiMeshesPtr,
    redraw: RedrawTrigger,
) -> SceneNodePtr {
    let window_scale =
        PropertyFloat32::wrap(&sg_root.lookup_node("/window").unwrap(), Role::Internal, "scale", 0)
            .unwrap();
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = Compiler::new();

    cc.add_const_f32("CHATEDIT_HEIGHT", CHATEDIT_HEIGHT);
    cc.add_const_f32("CHATEDIT_SINGLE_LINE_Y", CHATEDIT_SINGLE_LINE_Y);
    cc.add_const_f32("CHATEDIT_BOTTOM_PAD", CHATEDIT_BOTTOM_PAD);
    cc.add_const_f32("CHATEDIT_NEG_W", CHATEDIT_NEG_W);
    cc.add_const_f32("SENDARROW_NEG_X", SENDARROW_NEG_X);
    cc.add_const_f32("SENDARROW_NEG_Y", SENDARROW_NEG_Y);
    cc.add_const_f32("EMOJI_NEG_Y", EMOJI_NEG_Y);
    cc.add_const_f32("COPY_BTN_X_OFF", COPY_BTN_X_OFF);
    cc.add_const_f32("EMOJIBTN_BOX_1", EMOJIBTN_BOX[1]);
    cc.add_const_f32("SENDBTN_BOX_0", SENDBTN_BOX[0]);
    cc.add_const_f32("SENDBTN_BOX_1", SENDBTN_BOX[1]);
    cc.add_const_f32("CMD_HELP_HEIGHT", CMD_HELP_HEIGHT);
    cc.add_const_f32("CMD_HELP_GAP", CMD_HELP_GAP);
    cc.add_const_f32("DOWNARROW_NEG_X", DOWNARROW_NEG_X);
    cc.add_const_f32("DOWNARROW_NEG_Y", DOWNARROW_NEG_Y);
    cc.add_const_f32("NETSTATUS_ICON_SIZE", super::NETSTATUS_ICON_SIZE);

    // Main view
    let layer_node = create_layer("main_chat_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let layer_node = layer_node.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    content.link(layer_node.clone());

    // Create a bg mesh on top to fade the bg image
    let node = create_vector_art("bg");
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
        expr::load_var("w"),
        expr::load_var("h"),
        [[0., 0., 0., 0.5], [0., 0., 0., 0.5], [0., 0., 0., 0.5], [0., 0., 0., 0.8]],
    );
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create the toolbar bg
    let node = create_vector_art("toolbar_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let (bg_color, sep_color) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0., 0., 1.], [0.41, 0.6, 0.65, 1.]),
        ColorScheme::PaperLight => ([1., 1., 1., 1.], [0., 0.6, 0.65, 1.]),
    };
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        bg_color,
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(BACKARROW_BG_W),
        expr::load_var("h"),
        [0.0, 0.106, 0.114, 1.0],
    );
    shape.add_filled_box(
        expr::const_f32(BACKARROW_BG_W),
        expr::const_f32(0.),
        expr::const_f32(BACKARROW_BG_W + BACK_SEP_W),
        expr::load_var("h"),
        sep_color,
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
        expr::const_f32(BACKARROW_BG_W + 1.),
        expr::const_f32(0.),
        expr::load_var("w"),
        cc.compile("h / 2").unwrap(),
        color1,
        color2,
        8,
        0.2,
    );

    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();

    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create the send button
    let node = create_vector_art("back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, BACKARROW_SCALE).unwrap();
    prop.set_f32(atom, Role::App, 3, BACKARROW_SCALE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let back_btn_bg_node =
        node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(back_btn_bg_node.clone());

    // Create the back button
    let node = create_button("back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, BACKARROW_BG_W).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    // Menu doesn't exist yet ;)
    // So look it up in the callback.
    let sg_root2 = sg_root.clone();
    let layer_node2 = layer_node.clone();
    let chatview_is_visible = PropertyBool::wrap(&layer_node, Role::App, "is_visible", 0).unwrap();
    let redraw2 = redraw.clone();
    let goback = async move || {
        info!(target: "app::chat", "clicked back");
        let atom = &mut redraw2.make_guard(gfxtag!("goback action"));

        // Only unfocus editor (hide keyboard) on Android to hide IME.
        // On desktop we will keep editor focused so when we switch back
        // the user can keep typing without having to click the edit.
        #[cfg(target_os = "android")]
        {
            let editz_node = layer_node2.lookup_node("/content/editz").unwrap();
            editz_node.call_method("unfocus", vec![]).await.unwrap();
        }

        let menu_node = sg_root2.lookup_node("/window/content/chat/menu_layer").unwrap();
        menu_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

        chatview_is_visible.set(atom, false);
    };

    let (slot, recvr) = Slot::new("back_clicked");
    node.register("click", slot).unwrap();
    let goback2 = goback.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback2().await;
        }
    });
    layer_node.push_task(listen_click);

    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create shortcut to go back as well
    let node = create_shortcut("back_shortcut");
    #[cfg(target_os = "android")]
    node.set_property_str(atom, Role::App, "key", "back").unwrap();
    #[cfg(target_os = "macos")]
    node.set_property_str(atom, Role::App, "key", "logo+left").unwrap();
    #[cfg(all(not(target_os = "android"), not(target_os = "macos")))]
    node.set_property_str(atom, Role::App, "key", "alt+left").unwrap();
    // Not sure what was eating my keys. This is a workaround.
    node.set_property_u32(atom, Role::App, "priority", 10).unwrap();

    let (slot, recvr) = Slot::new("back_pressed");
    node.register("shortcut", slot).unwrap();
    let listen_enter = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback().await;
        }
    });
    layer_node.push_task(listen_enter);

    let node = node.setup(|me| Shortcut::new(me)).await;
    layer_node.link(node);

    // Create some text
    let node = create_text("channel_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHANNEL_LABEL_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_Y).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let label_node = node
        .setup(|me| {
            Text::new(me, window_scale.clone(), renderer.clone(), i18n_fish.clone(), redraw.clone())
        })
        .await;
    layer_node.link(label_node.clone());

    // Create the emoji picker
    let mut node = create_emoji_picker("emoji_picker");
    let prop = Property::new("dynamic_h", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();
    let emoji_dynamic_h_prop = node.get_property("dynamic_h").unwrap();
    //emoji_dynamic_h_prop.set_f32(atom, Role::App, 0, 400.).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("h - dynamic_h").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("dynamic_h")).unwrap();
    prop.add_depend(&emoji_dynamic_h_prop, 0, "dynamic_h");
    let emoji_h_prop = PropertyFloat32::wrap(&node, Role::App, "dynamic_h", 0).unwrap();
    //node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "emoji_size", EMOJI_PICKER_ICON_SIZE).unwrap();
    let prop = node.get_property("emoji_margin").unwrap();
    prop.set_f32(atom, Role::App, 0, EMOJI_PICKER_ICON_MARGIN_X).unwrap();
    prop.set_f32(atom, Role::App, 1, EMOJI_PICKER_ICON_MARGIN_Y).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let node =
        node.setup(|me| EmojiPicker::new(me, renderer.clone(), emoji_meshes, redraw.clone())).await;
    let emoji_picker_node = node.clone();
    layer_node.link(node);

    // Create the editbox bg
    let node = create_vector_art("emoji_picker_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("h - dynamic_h").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("dynamic_h")).unwrap();
    prop.add_depend(&emoji_dynamic_h_prop, 0, "dynamic_h");
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let mut shape = VectorShape::new();
    // Main bg
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
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
    // Bottom line
    //shape.add_filled_box(
    //    expr::const_f32(0.),
    //    cc.compile("h - 1").unwrap(),
    //    expr::load_var("w"),
    //    expr::load_var("h"),
    //    [0.41, 0.6, 0.65, 1.],
    //);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Main content view
    let chat_layer_node = layer_node;
    let layer_node = create_layer("content");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h - emoji_h").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    prop.add_depend(&emoji_dynamic_h_prop, 0, "emoji_h");
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    layer_node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let layer_node = layer_node.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    chat_layer_node.link(layer_node.clone());

    // ChatView
    let node = create_chatview("chatty");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 10.).unwrap();
    prop.set_f32(atom, Role::App, 1, CHATEDIT_HEIGHT).unwrap();
    let code = cc.compile("w - 30").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    let code = cc.compile("h - CHATEDIT_HEIGHT - editz_h - 2 * CHATEDIT_BOTTOM_PAD").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    let chatview_rect_prop = prop.clone();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "timestamp_font_size", TIMESTAMP_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "timestamp_width", TIMESTAMP_WIDTH).unwrap();
    node.set_property_f32(atom, Role::App, "line_height", LINE_HEIGHT).unwrap();
    node.set_property_f32(atom, Role::App, "message_spacing", MESSAGE_SPACING).unwrap();
    node.set_property_f32(atom, Role::App, "wheel_page_frac", 0.2).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", CHATVIEW_BASELINE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let prop = node.get_property("timestamp_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.407).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.604).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.647).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }

    let prop = node.get_property("hi_bg_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.2).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.2).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }

    let chatview_node = node
        .setup(|me| {
            ChatView::new(
                me,
                kv_db.clone(),
                window_scale.clone(),
                i18n_fish.clone(),
                renderer.clone(),
                redraw.clone(),
                ex.clone(),
            )
        })
        .await;
    layer_node.link(chatview_node.clone());

    let tree_name = "#dev__chat_tree_v2";
    let tree = kv_db.open_tree_default(&tree_name).unwrap();
    if tree.is_empty().expect("cannot read dev chat tree") {
        populate_tree(&tree);
    }

    // The label follows the bound channel.
    {
        let channel_prop = PropertyStr::wrap(&chatview_node, Role::App, "channel", 0).unwrap();
        let channel_sub = channel_prop.prop().subscribe_modify();
        let label_text = PropertyStr::wrap(&label_node, Role::App, "text", 0).unwrap();
        let redraw2 = redraw.clone();
        let label_task = ex.spawn(async move {
            while let Ok(_) = channel_sub.receive().await {
                let atom = &mut redraw2.make_guard(gfxtag!("channel label"));
                label_text.set(atom, channel_prop.get());
            }
        });
        chatview_node.push_task(label_task);
    }

    // Type-specific styling lives on the privmsg type sub-node.
    let privmsg_node = chatview_node.lookup_node("/privmsg").expect("privmsg type node");
    privmsg_node.set_property_f32(atom, Role::App, "cap_max_height", 1000.).unwrap();
    let prop = privmsg_node.get_property("action_text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.25).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.75).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = privmsg_node.get_property("url_text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.94).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = privmsg_node.get_property("url_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.13).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.08).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_bg_border_size", 1.).unwrap();
    let prop = privmsg_node.get_property("url_bg_border_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.11).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.6).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.63).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    // "Copied link" overlay styling (mirrors the edit action menu)
    let prop = privmsg_node.get_property("url_copy_fg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.94).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = privmsg_node.get_property("url_copy_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.1).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.1).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.1).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.9).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_copy_font_size", FONTSIZE).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_copy_padding", ACTION_PADDING).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_copy_offset", ACTION_PADDING).unwrap();
    let prop = privmsg_node.get_property("nick_colors").unwrap();
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
        prop.push_f32(atom, Role::App, c).unwrap();
    }

    // Date separator styling.
    let datemsg_node = chatview_node.lookup_node("/datemsg").expect("datemsg type node");
    datemsg_node.set_property_f32(atom, Role::App, "font_size", CHATVIEW_DATE_FONTSIZE).unwrap();
    let prop = datemsg_node.get_property("color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    // File signals live on the filemsg type node. The download request
    // payload carries (id, url); the fud plugin takes the url.
    let filemsg_node = chatview_node.lookup_node("/filemsg").expect("filemsg type node");

    let (slot, recvr) = Slot::new("fileurl_detect");
    filemsg_node.register("fileurl_detected", slot).unwrap();
    let sg_root2 = sg_root.clone();
    let listen_fileurl = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            if let Some(fud_node) = sg_root2.lookup_node("/plugin/fud") {
                let _ = fud_node.call_method("track_file", data).await;
            }
        }
    });
    layer_node.push_task(listen_fileurl);

    let (slot, recvr) = Slot::new("file_download");
    filemsg_node.register("download_request", slot).unwrap();
    let sg_root2 = sg_root.clone();
    let listen_file_download = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let mut cur = Cursor::new(&data);
            let Ok(_id) = MessageId::decode(&mut cur) else { continue };
            let Ok(url) = Url::decode(&mut cur) else { continue };
            if let Some(fud_node) = sg_root2.lookup_node("/plugin/fud") {
                let mut fud_data = vec![];
                url.encode(&mut fud_data).unwrap();
                let _ = fud_node.call_method("get", fud_data).await;
            }
        }
    });
    layer_node.push_task(listen_file_download);

    let down_layer = create_layer("chat_down_arrow");
    let prop = down_layer.get_property("rect").unwrap();
    let code = cc.compile("w - DOWNARROW_NEG_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - DOWNARROW_NEG_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, DOWNARROW_W).unwrap();
    prop.set_f32(atom, Role::App, 3, DOWNARROW_H).unwrap();
    down_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    down_layer.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    // The arrow floats over the chatview (priority 0) — it must be
    // hit-tested first.
    down_layer.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let down_layer = down_layer.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(down_layer.clone());

    let down_layer_is_visible =
        PropertyBool::wrap(&down_layer, Role::App, "is_visible", 0).unwrap();
    let chatview_at_bottom =
        PropertyBool::wrap(&chatview_node, Role::App, "is_at_bottom", 0).unwrap();
    let at_bottom_sub = chatview_at_bottom.prop().subscribe_modify();
    let redraw2 = redraw.clone();
    let chatview_at_bottom2 = chatview_at_bottom.clone();
    let monitor_scroll_task = ex.spawn(async move {
        while let Ok(_) = at_bottom_sub.receive().await {
            let at_bottom = chatview_at_bottom2.get();
            let atom = &mut redraw2.make_guard(gfxtag!("down arrow visibility change"));
            down_layer_is_visible.set(atom, !at_bottom);
        }
    });
    down_layer.push_task(monitor_scroll_task);

    // Placeholder single-color background filling the whole overlay
    let node = create_vector_art("downbg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, DOWNBG_X).unwrap();
    prop.set_f32(atom, Role::App, 1, DOWNBG_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, DOWNARROW_W).unwrap();
    prop.set_f32(atom, Role::App, 3, DOWNARROW_H).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape =
        shape::create_down_bgtab(rgba!(0x003232ff), rgba!(0x294f60ff), 0.1).scaled(DOWNARROW_SCALE);
    shape.join(shape::create_down_arrow(rgba!(0x00f0ffff), 1.));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    down_layer.link(node);

    // Create the p2p toggle button
    let node = create_button("scroll_bottom_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, DOWNARROW_W).unwrap();
    prop.set_f32(atom, Role::App, 3, DOWNARROW_H).unwrap();
    let (slot, recvr) = Slot::new("scroll_bottom");
    node.register("click", slot).unwrap();
    let chatview_node2 = chatview_node.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let _ = chatview_node2.call_method("scroll_to_bottom", vec![]).await;
        }
    });
    down_layer.push_task(listen_click);
    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    down_layer.link(node);

    // Selection overlay: shown only while the chatview has selected lines. It's
    // a child of `content` (not the chat layer) with z_index and priority above
    // the netstatus layer, so its single background box draws over the netstatus
    // icons and its buttons win click hit-testing. It carries `unselect_btn`
    // (over `back_btn`) and `copy_btn` (over the reconnect button).
    let select_layer = create_layer("select_layer");
    let prop = select_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    select_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    select_layer.set_property_u32(atom, Role::App, "z_index", 100).unwrap();
    select_layer.set_property_u32(atom, Role::App, "priority", 100).unwrap();
    let select_layer =
        select_layer.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    content.link(select_layer.clone());

    // Background box covering the back button
    let node = create_vector_art("select_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(BACKARROW_BG_W),
        expr::load_var("h"),
        rgba!(0x1e0000ff),
    );
    shape.join(shape::create_x(
        Point::new(SELECT_CLOSE_X, SELECT_CLOSE_Y),
        SELECT_CLOSE_SCALE,
        1.,
        COLOR_RED,
    ));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    select_layer.link(node);

    let node = create_vector_art("copy_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - COPY_BTN_X_OFF").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, COPY_BTN_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 400.).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let shape = shape::create_copy(COLOR_WHITE).scaled(COPY_BTN_SCALE);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    select_layer.link(node);

    // unselect_btn sits over the back button and calls the chatview's unselect.
    let node = create_button("unselect_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, EMOJI_BG_W).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    {
        let chatview_node2 = chatview_node.clone();
        let (slot, recvr) = Slot::new("unselect_clicked");
        node.register("click", slot).unwrap();
        let listen_click = ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                let _ = chatview_node2.call_method("unselect", vec![]).await;
            }
        });
        select_layer.push_task(listen_click);
    }
    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    select_layer.link(node);

    // copy_btn sits over the reconnect button and calls the chatview's
    // copy_select (which also deselects, hiding this overlay again).
    let node = create_button("copy_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, super::NETSTATUS_ICON_SIZE).unwrap();
    prop.set_f32(atom, Role::App, 3, super::NETSTATUS_ICON_SIZE).unwrap();
    {
        let chatview_node2 = chatview_node.clone();
        let (slot, recvr) = Slot::new("copy_clicked");
        node.register("click", slot).unwrap();
        let listen_click = ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                chatview_node2.call_method("copy_select", vec![]).await.unwrap();
            }
        });
        select_layer.push_task(listen_click);
    }
    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    select_layer.link(node);

    // Show/hide the overlay from the chatview's select_changed signal.
    let select_is_visible = PropertyBool::wrap(&select_layer, Role::App, "is_visible", 0).unwrap();
    let back_btn_bg_node2 = back_btn_bg_node.clone();
    let sg_root2 = sg_root.clone();
    let redraw2 = redraw.clone();
    let (slot, recvr) = Slot::new("select_changed_slot");
    chatview_node.register("select_changed", slot).unwrap();
    let listen_select = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let Ok(selected) = bool::decode(&mut std::io::Cursor::new(&data)) else { continue };
            let atom = &mut redraw2.make_guard(gfxtag!("select_changed"));
            select_is_visible.set(atom, selected);
            back_btn_bg_node2.set_property_bool(atom, Role::App, "is_visible", !selected).unwrap();
            if let Some(netstatus_layer) =
                sg_root2.lookup_node("/window/content/chat/netstatus_layer")
            {
                netstatus_layer
                    .set_property_bool(atom, Role::App, "is_visible", !selected)
                    .unwrap();
            }
        }
    });
    select_layer.push_task(listen_select);

    // Create the editbox bg
    let node = create_vector_art("editbox_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("h - editz_h - 2 * CHATEDIT_BOTTOM_PAD").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("editz_h + 2 * CHATEDIT_BOTTOM_PAD").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 4).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 2).unwrap();

    let editbox_bg_rect_prop = prop.clone();

    let (bg_color, lhs_bg_color, line_color) = match COLOR_SCHEME {
        ColorScheme::DarkMode => {
            ([0., 0.13, 0.08, 1.], [0., 0.11, 0.11, 1.], [0.41, 0.6, 0.65, 1.])
        }
        ColorScheme::PaperLight => ([1., 1., 1., 1.], [1., 1., 1., 1.], [0., 0., 0., 1.]),
    };

    let mut shape = VectorShape::new();
    // Main green background
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        bg_color,
    );
    // Top line
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(1.),
        line_color,
    );
    shape.add_radial_glow(
        // Center
        cc.compile("w / 2").unwrap(),
        expr::load_var("h"),
        // Size
        expr::load_var("w"),
        cc.compile("h / 4").unwrap(),
        // Segments
        8,
        // Angles
        std::f32::consts::PI,
        2. * std::f32::consts::PI,
        // Color
        [0., 0.28, 0.2, 1.],
    );
    // Bottom line
    //shape.add_filled_box(
    //    expr::const_f32(0.),
    //    cc.compile("h - 1").unwrap(),
    //    expr::load_var("w"),
    //    expr::load_var("h"),
    //    [0.41, 0.6, 0.65, 1.],
    //);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create the send button
    let node = create_vector_art("send_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - SENDARROW_NEG_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - SENDARROW_NEG_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 5).unwrap();
    let shape = shape::create_send_arrow().scaled(EMOJI_SCALE);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create the emoji button
    let node = create_vector_art("emoji_btn_bg");
    let emoji_btn_is_visible = PropertyBool::wrap(&node, Role::Ignored, "is_visible", 0).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, EMOJI_BTN_X).unwrap();
    let code = cc.compile("h - EMOJI_NEG_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 5).unwrap();
    let color = match COLOR_SCHEME {
        ColorScheme::DarkMode => [0., 1., 1., 1.],
        ColorScheme::PaperLight => [0., 0., 0., 1.],
    };
    let shape = shape::create_emoji_selector(color).scaled(EMOJI_SCALE);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create the emoji button
    let node = create_vector_art("emoji_close_btn_bg");
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    let prop = node.get_property("rect").unwrap();
    let emoji_close_is_visible = PropertyBool::wrap(&node, Role::Ignored, "is_visible", 0).unwrap();
    prop.set_f32(atom, Role::App, 0, EMOJI_BTN_X).unwrap();
    let code = cc.compile("h - EMOJI_NEG_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 5).unwrap();
    let shape = shape::create_close_icon().scaled(EMOJI_CLOSE_SCALE);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Text edit
    let node = create_multiline_edit("editz");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();

    let prop = node.get_property("height_range").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_MIN_HEIGHT).unwrap();
    prop.set_f32(atom, Role::App, 1, CHATEDIT_MAX_HEIGHT).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_LHS_PAD).unwrap();
    let code = cc.compile("parent_h - (rect_h + CHATEDIT_BOTTOM_PAD)").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("parent_w - CHATEDIT_NEG_W").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    chatview_rect_prop.add_depend(&prop, 3, "editz_h");
    editbox_bg_rect_prop.add_depend(&prop, 3, "editz_h");

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, TEXTBAR_BASELINE * 0.4).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, TEXTBAR_BASELINE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();

    node.set_property_f32(atom, Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
    //node.set_property_str(atom, Role::App, "text", "hello king!😁🍆jelly 🍆1234").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    let prop = node.get_property("text_hi_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.44).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.96).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("text_cmd_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.64).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.83).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_f32(atom, Role::App, "cursor_ascent", CHATEDIT_CURSOR_ASCENT).unwrap();
    node.set_property_f32(atom, Role::App, "cursor_descent", CHATEDIT_CURSOR_DESCENT).unwrap();
    node.set_property_f32(atom, Role::App, "select_ascent", CHATEDIT_SELECT_ASCENT).unwrap();
    node.set_property_f32(atom, Role::App, "select_descent", CHATEDIT_SELECT_DESCENT).unwrap();
    node.set_property_f32(atom, Role::App, "handle_descent", CHATEDIT_HANDLE_DESCENT).unwrap();
    node.set_property_f32(atom, Role::App, "action_padding", ACTION_PADDING).unwrap();
    node.set_property_f32(atom, Role::App, "action_spacing", ACTION_SPACING).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.27).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.22).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    let prop = node.get_property("cmd_bg_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.30).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.25).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 6).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 3).unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();

    let editz_text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();

    let editz_select_text = node.get_property("select_text").unwrap();

    //let editbox_focus = PropertyBool::wrap(node, Role::App, "is_focused", 0).unwrap();
    //let darkirc_backend = app.darkirc_backend.clone();
    //let task = ex.spawn(async move {
    //    while let Ok(_) = btn_click_recvr.recv().await {
    //        let text = editbox_text.get();
    //        editbox_text.prop().unset(Role::App, 0).unwrap();
    //        // Clicking outside the editbox makes it lose focus
    //        // So lets focus it again
    //        editbox_focus.set(atom, true);

    //        debug!(target: "app", "sending text {text}");

    //        let privmsg =
    //            Privmsg { channel: "#random".to_string(), nick: "king".to_string(), msg: text };
    //        darkirc_backend.send(privmsg).await;
    //    }
    //});
    //tasks.push(task);

    let node = node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                renderer.clone(),
                redraw.clone(),
                BaseEditType::MultiLine,
                ex.clone(),
            )
        })
        .await;
    let chatedit_node = node.clone();
    layer_node.link(node);

    // Nick clicks on messages insert the nick at the editor cursor,
    // like the emoji picker.
    {
        let (slot, recvr) = Slot::new("nick_clicked");
        privmsg_node.register("nick_clicked", slot).unwrap();
        let chatedit_node3 = chatedit_node.clone();
        let listen_nick = ex.spawn(async move {
            while let Ok(data) = recvr.recv().await {
                let mut cur = Cursor::new(&data);
                let Ok(_id) = MessageId::decode(&mut cur) else { continue };
                let Ok(nick) = String::decode(&mut cur) else { continue };
                let mut edit_data = vec![];
                nick.encode(&mut edit_data).unwrap();
                chatedit_node3.call_method("insert_text", edit_data).await.unwrap();
            }
        });
        layer_node.push_task(listen_nick);
    }

    let (slot, recvr) = Slot::new("emoji_selected");
    emoji_picker_node.register("emoji_select", slot).unwrap();
    let chatedit_node2 = chatedit_node.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            // No need to decode the data. Just pass it straight along
            chatedit_node2.call_method("insert_text", data).await.unwrap();
        }
    });
    layer_node.push_task(listen_click);

    // No way to get the top of the editbox until eval has been called.
    // But this is on top so it happens before the eval. So the value is 0.
    // Not sure the best way to fix this.
    /*
    // Create the editbox fg shadow
    let node = create_vector_art("editbox_fg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, EMOJI_BG_W).unwrap();
    let code = cc.compile("editz_bg_top_y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, 300.).unwrap();
    //prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    //prop.set_expr(atom, Role::App, 3, expr::load_var("editz_bg_h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 4).unwrap();
    prop.add_depend(&editbox_bg_rect_prop, 1, "editz_bg_top_y");
    prop.add_depend(&editbox_bg_rect_prop, 3, "editz_bg_h");

    let mut shape = VectorShape::new();
    // Left hand darker box
    shape.add_gradient_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(40.),
        [
            [0., 0., 0., 1.],
            [0., 0., 0., 1.],
            [0., 0., 0., 0.],
            [0., 0., 0., 0.],
        ]
    );
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);
    */

    // Create the send button
    let node = create_button("send_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - SENDBTN_BOX_0").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - SENDBTN_BOX_1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, SENDBTN_BOX[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, SENDBTN_BOX[3]).unwrap();

    let editz_text2 = editz_text.clone();
    let sg_root2 = sg_root.clone();
    let redraw2 = redraw.clone();
    let sendmsg = move || {
        let editz_text = editz_text2.clone();
        let sg_root = sg_root2.clone();
        let chatview_node = chatview_node.clone();
        let redraw = redraw2.clone();
        async move {
            let mut text = editz_text.get();
            let channel = chatview_node.get_property_str("channel").unwrap_or_default();
            let privmsg_node = chatview_node.lookup_node("/privmsg").expect("privmsg type node");
            trace!(target: "app::chat", "send to channel: {channel}");
            {
                let atom = &mut redraw.make_guard(gfxtag!("sendmsg clear edit"));
                editz_text.set(atom, "");
            }

            let Some(darkirc) = sg_root.lookup_node("/plugin/darkirc") else {
                error!(target: "app::chat", "DarkIrc plugin has not been loaded");
                return
            };

            if text.starts_with("/nick") {
                let nick = text.split_whitespace().nth(1).unwrap_or("anon");
                info!(target: "app::chat", "Setting nick to: {nick}");
                {
                    let atom = &mut redraw.make_guard(gfxtag!("sendmsg action"));
                    darkirc.set_property_str(atom, Role::App, "nick", nick).unwrap();
                }

                let msg = format!("You are now known as <{nick}>");
                let id: [u8; 32] = rand::random();

                let mut data = vec![];
                unixtime().encode(&mut data).unwrap();
                id.encode(&mut data).unwrap();
                "NOTICE".encode(&mut data).unwrap();
                msg.encode(&mut data).unwrap();
                privmsg_node.call_method("insert_line", data).await.unwrap();

                return
            }

            if text == "/me" {
                // Bare /me without action text sends nothing
                return
            }

            if let Some(rest) = text.strip_prefix("/me ") {
                let mut action = rest.trim().to_string();
                if action.is_empty() {
                    return
                }

                // Limit line length. The CTCP framing bytes are neither
                // counted towards the limit nor cut by truncation.
                if action.len() > 300 {
                    action.truncate(300);
                    action.push('…');
                }

                let text = format!("\u{1}ACTION {action}\u{1}");
                let timest = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
                let nick = darkirc.get_property_str("nick").unwrap();
                #[cfg(feature = "enable-plugin-darkirc")]
                {
                    let msg = Privmsg { version: 0, msg_type: 0, channel, nick, msg: text };
                    let mut data = vec![];
                    timest.encode(&mut data).unwrap();
                    msg.channel.encode(&mut data).unwrap();
                    msg.msg.encode(&mut data).unwrap();
                    darkirc.call_method("send", data).await.unwrap();
                }

                return
            }

            // Limit line length
            if text.len() > 300 {
                text.truncate(300);
                text.push('…');
            }

            let timest = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
            let nick = darkirc.get_property_str("nick").unwrap();
            #[cfg(feature = "enable-plugin-darkirc")]
            {
                let msg = Privmsg { version: 0, msg_type: 0, channel, nick, msg: text };
                let mut data = vec![];
                timest.encode(&mut data).unwrap();
                msg.channel.encode(&mut data).unwrap();
                msg.msg.encode(&mut data).unwrap();
                darkirc.call_method("send", data).await.unwrap();
            }
        }
    };

    let (slot, recvr) = Slot::new("send_clicked");
    node.register("click", slot).unwrap();
    let sendmsg2 = sendmsg.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            sendmsg2().await;
        }
    });
    layer_node.push_task(listen_click);

    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Create shortcut to send as well
    let node = create_shortcut("send_shortcut");
    node.set_property_str(atom, Role::App, "key", "enter").unwrap();

    let (slot, recvr) = Slot::new("enter_pressed");
    node.register("shortcut", slot).unwrap();
    let listen_enter = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            sendmsg().await;
        }
    });
    layer_node.push_task(listen_enter);

    let node = node.setup(|me| Shortcut::new(me)).await;
    layer_node.link(node);

    // Create the emoji button
    let node = create_button("emoji_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, EMOJIBTN_BOX[0]).unwrap();
    let code = cc.compile("h - EMOJIBTN_BOX_1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, EMOJIBTN_BOX[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, EMOJIBTN_BOX[3]).unwrap();

    // Chatedit is clicked and requests keyboard. Only show if emoji
    // picker isnt visible: while the panel is open, tapping the edit
    // just moves the cursor.
    let (slot, recvr) = Slot::new("reqkeyb");
    chatedit_node.register("focus_request", slot).unwrap();
    let chatedit_node2 = chatedit_node.clone();
    let emoji_btn_is_visible2 = emoji_btn_is_visible.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            if emoji_btn_is_visible2.get() {
                debug!(target: "app::chat", "Emoji picker not visible so showing keyboard");
                chatedit_node2.call_method("focus", vec![]).await.unwrap();
            }
        }
    });
    layer_node.push_task(listen_click);

    // Emoji button is clicked
    let (slot, recvr) = Slot::new("emoji_clicked");
    let chatedit_node2 = chatedit_node.clone();
    node.register("click", slot).unwrap();
    let redraw2 = redraw.clone();
    let window_scale2 = window_scale.clone();
    let listen_click = ex.spawn(async move {
        let mut panel_height = if cfg!(target_os = "android") {
            let keyb_height = android_keyboard_height(window_scale2.get());
            if keyb_height > 0. {
                keyb_height
            } else {
                600.
            }
        } else {
            400.
        };

        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked emoji");
            let atom = &mut redraw2.make_guard(gfxtag!("emoji click action"));

            if cfg!(target_os = "android") {
                let keyb_height = android_keyboard_height(window_scale2.get());
                if keyb_height > panel_height {
                    panel_height = keyb_height
                }
            }

            if emoji_btn_is_visible.get() {
                // Open emoji panel and hide the keyboard. The edit
                // stays focused so its cursor remains visible; only
                // the IME is detached.
                chatedit_node2.call_method("hide_ime", vec![]).await.unwrap();

                assert!(!emoji_close_is_visible.get());
                assert!(emoji_h_prop.get() < 0.001);
                emoji_btn_is_visible.set(atom, false);
                emoji_close_is_visible.set(atom, true);
                emoji_h_prop.set(atom, panel_height as f32);
                //for i in 1..=20 {
                //    emoji_h_prop.set(&mut atom, (20 * i) as f32);
                //    msleep(10).await;
                //}
            } else {
                assert!(!is_ime_visible());
                // Hide emoji panel and show the keyboard
                chatedit_node2.call_method("focus", vec![]).await.unwrap();

                assert!(emoji_close_is_visible.get());
                assert!(emoji_h_prop.get() > 0.);
                emoji_btn_is_visible.set(atom, true);
                emoji_close_is_visible.set(atom, false);
                emoji_h_prop.set(atom, 0. as f32);
                //for i in 1..=20 {
                //    emoji_h_prop.set(&mut atom, (400 - 20 * i) as f32);
                //    msleep(10).await;
                //}
            }
        }
    });
    layer_node.push_task(listen_click);

    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(node);

    // Commands help hint
    let mut cmd_layer_node = create_layer("cmd_hint_layer");

    // Number of visible command rows (0, 1, or 2). Drives the popup height
    // so it shrinks/grows as rows are filtered by the typed text.
    let mut prop = Property::new("vis_rows", PropertyType::Float32, PropertySubType::Null);
    prop.set_defaults_f32(vec![0.]).unwrap();
    cmd_layer_node.add_property(prop).unwrap();
    let cmd_vis_rows_prop = cmd_layer_node.get_property("vis_rows").unwrap();

    // Whether the /nick row is visible (1.0/0.0). Positions the /me row
    // below it when both rows are shown.
    let mut prop = Property::new("nick_row_vis", PropertyType::Float32, PropertySubType::Null);
    prop.set_defaults_f32(vec![0.]).unwrap();
    cmd_layer_node.add_property(prop).unwrap();
    let nick_row_vis_prop = cmd_layer_node.get_property("nick_row_vis").unwrap();

    let prop = cmd_layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 5.).unwrap();
    let code = cc.compile("editz_bg_top_y - CMD_HELP_HEIGHT * vis_rows - CMD_HELP_GAP").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - 2 * CMD_HELP_GAP").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    let code = cc.compile("CMD_HELP_HEIGHT * vis_rows").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    prop.add_depend(&editbox_bg_rect_prop, 1, "editz_bg_top_y");
    prop.add_depend(&cmd_vis_rows_prop, 0, "vis_rows");
    cmd_layer_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    cmd_layer_node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    // The hint popup opens over the chatview (priority 0) — it must be
    // hit-tested first.
    cmd_layer_node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let cmd_layer_node =
        cmd_layer_node.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    layer_node.link(cmd_layer_node.clone());

    let cmd_hint_is_visible =
        PropertyBool::wrap(&cmd_layer_node, Role::App, "is_visible", 0).unwrap();

    // The /nick row sits at the top of the popup
    let nick_row_node = create_layer("nick_row");
    let prop = nick_row_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    let code = cc.compile("w").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CMD_HELP_HEIGHT).unwrap();
    nick_row_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    let nick_row_node =
        nick_row_node.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    cmd_layer_node.link(nick_row_node.clone());

    let nick_row_is_visible =
        PropertyBool::wrap(&nick_row_node, Role::App, "is_visible", 0).unwrap();

    // The /me row sits below the /nick row, or at the top when /nick is filtered out
    let me_row_node = create_layer("me_row");
    let prop = me_row_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("CMD_HELP_HEIGHT * nick_row_vis").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CMD_HELP_HEIGHT).unwrap();
    prop.add_depend(&nick_row_vis_prop, 0, "nick_row_vis");
    me_row_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    let me_row_node =
        me_row_node.setup(|me| Layer::new(me, renderer.clone(), redraw.clone())).await;
    cmd_layer_node.link(me_row_node.clone());

    let me_row_is_visible = PropertyBool::wrap(&me_row_node, Role::App, "is_visible", 0).unwrap();

    // Make nick label clickable
    let node = create_button("nickcmd_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, CMD_HELP_NICK_CMD_WIDTH).unwrap();
    prop.set_f32(atom, Role::App, 3, CMD_HELP_HEIGHT).unwrap();

    let (slot, recvr) = Slot::new("nickcmd_clicked");
    node.register("click", slot).unwrap();
    let editz_text2 = editz_text.clone();
    let redraw2 = redraw.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked /nick");
            let atom = &mut redraw2.make_guard(gfxtag!("nickcmd_clicked action"));
            // This will autohide this popup due to ending in a space.
            // Setting the property will retrigger the logic whether to show popup.
            editz_text2.set(atom, "/nick ");
        }
    });
    layer_node.push_task(listen_click);

    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    nick_row_node.link(node);

    // Make /me label clickable
    let node = create_button("mecmd_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, CMD_HELP_NICK_CMD_WIDTH).unwrap();
    prop.set_f32(atom, Role::App, 3, CMD_HELP_HEIGHT).unwrap();

    let (slot, recvr) = Slot::new("mecmd_clicked");
    node.register("click", slot).unwrap();
    let editz_text2 = editz_text.clone();
    let redraw2 = redraw.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::chat", "clicked /me");
            let atom = &mut redraw2.make_guard(gfxtag!("mecmd_clicked action"));
            // This will autohide this popup due to ending in a space.
            // Setting the property will retrigger the logic whether to show popup.
            editz_text2.set(atom, "/me ");
        }
    });
    layer_node.push_task(listen_click);

    let node = node.setup(|me| Button::new(me, renderer.clone(), redraw.clone())).await;
    me_row_node.link(node);

    // Create the /nick row actionbar bg
    let node = create_vector_art("cmd_hint_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_GAP),
        cc.compile("w - CMD_HELP_GAP").unwrap(),
        cc.compile("h - CMD_HELP_GAP").unwrap(),
        [0., 0.11, 0.11, 0.4],
    );
    shape.add_filled_box(
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_NICK_CMD_WIDTH),
        cc.compile("h - CMD_HELP_GAP").unwrap(),
        [0., 0.3, 0.25, 1.],
    );
    shape.add_filled_box(
        expr::const_f32(CMD_HELP_NICK_CMD_WIDTH),
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_NICK_DESC_WIDTH),
        cc.compile("h - CMD_HELP_GAP").unwrap(),
        [0., 0.11, 0.11, 1.],
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        [0.29, 0.51, 0.45, 1.],
    );

    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();

    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    nick_row_node.link(node);

    // Create some text
    let node = create_text("cmd_nick_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CMD_HELP_CMD_LABEL_X_INSET).unwrap();
    prop.set_f32(atom, Role::App, 1, CMD_HELP_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", CMD_HELP_CMD_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "/nick").unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();
    //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.64).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.83).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| {
            Text::new(me, window_scale.clone(), renderer.clone(), i18n_fish.clone(), redraw.clone())
        })
        .await;
    nick_row_node.link(node);

    // Create some text
    let node = create_text("cmd_nick_desc_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CMD_HELP_NICK_DESC_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CMD_HELP_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Change your nickname").unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();
    //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.94).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| {
            Text::new(me, window_scale.clone(), renderer.clone(), i18n_fish.clone(), redraw.clone())
        })
        .await;
    nick_row_node.link(node);

    // Create the /me row actionbar bg
    let node = create_vector_art("me_cmd_hint_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_GAP),
        cc.compile("w - CMD_HELP_GAP").unwrap(),
        cc.compile("h - CMD_HELP_GAP").unwrap(),
        [0., 0.11, 0.11, 0.4],
    );
    shape.add_filled_box(
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_NICK_CMD_WIDTH),
        cc.compile("h - CMD_HELP_GAP").unwrap(),
        [0., 0.3, 0.25, 1.],
    );
    shape.add_filled_box(
        expr::const_f32(CMD_HELP_NICK_CMD_WIDTH),
        expr::const_f32(CMD_HELP_GAP),
        expr::const_f32(CMD_HELP_NICK_DESC_WIDTH),
        cc.compile("h - CMD_HELP_GAP").unwrap(),
        [0., 0.11, 0.11, 1.],
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        [0.29, 0.51, 0.45, 1.],
    );

    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();

    let node = node.setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone())).await;
    me_row_node.link(node);

    // Create some text
    let node = create_text("cmd_me_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CMD_HELP_CMD_LABEL_X_INSET).unwrap();
    prop.set_f32(atom, Role::App, 1, CMD_HELP_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", CMD_HELP_CMD_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "/me").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.64).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.83).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| {
            Text::new(me, window_scale.clone(), renderer.clone(), i18n_fish.clone(), redraw.clone())
        })
        .await;
    me_row_node.link(node);

    // Create some text
    let node = create_text("cmd_me_desc_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CMD_HELP_NICK_DESC_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CMD_HELP_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Send an action message").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.94).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| {
            Text::new(me, window_scale.clone(), renderer.clone(), i18n_fish.clone(), redraw.clone())
        })
        .await;
    me_row_node.link(node);

    let editz_text_sub = editz_text.prop().subscribe_modify();
    let redraw = redraw.clone();
    let editz_text_task = ex.spawn(async move {
        while let Ok(_) = editz_text_sub.receive().await {
            let atom = &mut redraw.make_guard(gfxtag!("chatedit txt changed"));

            let text = editz_text.get();
            debug!(target: "app::chat", "text changed: {text}");
            // We want to avoid setting the property multiple times to the same value
            // because then it triggers unnecessary redraw work.

            // Show the /nick row for any prefix of "/nick", including the
            // full command (it still needs an argument). Show the /me row
            // only for strict prefixes ("/", "/m") — once "/me" is fully
            // typed the popup hides.
            let nick_vis = !text.is_empty() && "/nick".starts_with(&text);
            let me_vis = !text.is_empty() && text != "/me" && "/me".starts_with(&text);
            let popup_vis = nick_vis || me_vis;

            if nick_vis != nick_row_is_visible.get() {
                nick_row_is_visible.set(atom, nick_vis);
            }
            if me_vis != me_row_is_visible.get() {
                me_row_is_visible.set(atom, me_vis);
            }
            if popup_vis != cmd_hint_is_visible.get() {
                cmd_hint_is_visible.set(atom, popup_vis);
            }

            // Geometry inputs: the row count drives the popup height and
            // the /me row sits below the /nick row when both are shown.
            let vis_rows = (nick_vis as u8 + me_vis as u8) as f32;
            if cmd_vis_rows_prop.get_f32(0).unwrap() != vis_rows {
                cmd_vis_rows_prop.set_f32(atom, Role::App, 0, vis_rows).unwrap();
            }
            let nick_vis_f = nick_vis as u8 as f32;
            if nick_row_vis_prop.get_f32(0).unwrap() != nick_vis_f {
                nick_row_vis_prop.set_f32(atom, Role::App, 0, nick_vis_f).unwrap();
            }
        }
    });
    layer_node.push_task(editz_text_task);

    chat_layer_node
}

// Just for testing
#[allow(dead_code)]
pub(super) fn populate_tree(tree: &Tree) {
    use crate::ui::chatview::{codec, MessageId, MsgType};
    use chrono::{NaiveDate, NaiveDateTime};

    let chat_txt = include_str!("../../../data/chat.txt");
    for (idx, line) in chat_txt.lines().enumerate() {
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        assert_eq!(parts.len(), 3);
        let time_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
        let (hour, min) = (time_parts[0], time_parts[1]);
        let hour = hour.parse::<u32>().unwrap();
        let min = min.parse::<u32>().unwrap();
        let dt: NaiveDateTime =
            NaiveDate::from_ymd_opt(2024, 8, 6).unwrap().and_hms_opt(hour, min, 0).unwrap();
        let timest = dt.and_utc().timestamp_millis() as u64;

        let nick = parts[1].to_string();
        let text = parts[2].to_string();

        // Unique id per line: the minute timestamp alone can repeat.
        let mut id_bytes = [0u8; 32];
        id_bytes[..8].copy_from_slice(&(idx as u64).to_be_bytes());
        let id = MessageId(id_bytes);

        let payload = codec::encode_privmsg_payload(&nick, &text, true);
        let val = codec::encode_value(MsgType::PrivMsg, &payload);
        let key = codec::encode_key(timest, &id);
        tree.insert(&key, &val).unwrap();
    }
    // O(n)
    debug!(target: "app::schema", "populated db with {} lines", tree.len().unwrap());
}
