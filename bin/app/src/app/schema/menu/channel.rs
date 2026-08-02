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

use bs58;
use darkfi_serial::{async_trait, deserialize, Encodable, SerialDecodable, SerialEncodable};
use sled_overlay::sled;
use ui_consts::*;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "app::channel", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "app::channel", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "app::channel", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "app::channel", $($arg)*); } }

#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct Channel {
    pub name: String,
    pub secret: Option<[u8; 32]>,
}

use super::{
    super::chat, edit_buttons, edit_switch::edit_switch, ColorScheme, BTN_TEXT_Y,
    CHANNEL_ITEM_HEIGHT, COLOR_SCHEME, MENU_BTN_W_L,
};
use crate::{
    app::{
        node::{
            create_button, create_layer, create_menu, create_shortcut, create_singleline_edit,
            create_text, create_vector_art,
        },
        App,
    },
    expr,
    gfx::gfxtag,
    mesh::{COLOR_CYAN, COLOR_INACTIVE, COLOR_MINT, COLOR_MINT_OP, MINT_BTN_GRADIENT},
    prop::{PropertyBool, PropertyFloat32, Role},
    scene::{Pimpl, SceneNodePtr, Slot},
    shape,
    ui::{
        emoji_picker::EmojiMeshesPtr, BaseEdit, BaseEditType, Button, Layer, Menu, ShapeVertex,
        Shortcut, Text, UIObject, VectorArt, VectorShape, Window,
    },
    util::i18n::I18nBabelFish,
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const LABEL_X: f32 = 40.;
    pub const LABEL_LINESPACE: f32 = 125.;
    pub const LABEL_FONTSIZE: f32 = 44.;
    pub const MENU_SEP_SIZE: f32 = 3.;
    pub const MENU_HANDLE_PAD: f32 = 110.;
    pub const MENU_FADE: f32 = 130.;
    pub const VERBLOCK_SCALE: f32 = 150.;
    pub const VERBLOCK_X: f32 = 180.;
    pub const VERBLOCK_Y: f32 = 80.;
    pub const CHATEDIT_PAD: f32 = 38.;
    pub const CHATEDIT_HEIGHT: f32 = 115.;
    pub const TEXTBAR_BASELINE: f32 = FONTSIZE * 0.7;
    pub const FONTSIZE: f32 = 38.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = FONTSIZE * 0.7;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = FONTSIZE * 0.35;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 50.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 20.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 10.;
    pub const ACTION_PADDING: f32 = 32.;
    pub const ACTION_SPACING: f32 = 8.;
    pub const HEADER_HEIGHT: f32 = 140.;
    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 70.;
    pub const BACKARROW_Y: f32 = 70.;
    pub const BACKARROW_BG_W: f32 = 140.;
    pub const CONTENT_MARGIN: f32 = 30.;
    pub const COPY_WIDTH: f32 = 200.;
    pub const COPY_SCALE: f32 = 35.;
    pub const COPY_BTN_SIZE: f32 = CHATEDIT_HEIGHT;
    pub const CONTENT_OUTLINE_SIZE: f32 = 0.5;
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
    pub const LABEL_X: f32 = 20.;
    pub const LABEL_LINESPACE: f32 = 55.;
    pub const LABEL_FONTSIZE: f32 = 22.;
    pub const MENU_SEP_SIZE: f32 = 1.;
    pub const MENU_HANDLE_PAD: f32 = 80.;
    pub const MENU_FADE: f32 = 130.;
    pub const CHATEDIT_PAD: f32 = 20.;
    pub const CHATEDIT_HEIGHT: f32 = 50.;
    pub const TEXTBAR_BASELINE: f32 = FONTSIZE * 0.7;
    pub const FONTSIZE: f32 = 18.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = FONTSIZE * 0.7;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = FONTSIZE * 0.35;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 30.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 10.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 35.;
    pub const ACTION_PADDING: f32 = 8.;
    pub const ACTION_SPACING: f32 = 4.;
    pub const HEADER_HEIGHT: f32 = 60.;
    pub const CONTENT_MARGIN: f32 = 15.;
    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 26.;
    pub const BACKARROW_BG_W: f32 = 80.;
    pub const COPY_WIDTH: f32 = 100.;
    pub const COPY_SCALE: f32 = 15.;
    pub const COPY_BTN_SIZE: f32 = CHATEDIT_HEIGHT;
    pub const CONTENT_OUTLINE_SIZE: f32 = 0.3;
}

pub async fn make(
    app: &App,
    content: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
    contact_is_visible: PropertyBool,
    channel_is_visible: PropertyBool,
    channels_tree: sled::Tree,
    db: &sled::Db,
    emoji_meshes: EmojiMeshesPtr,
    is_first_time: bool,
) -> SceneNodePtr {
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("CHATEDIT_PAD", CHATEDIT_PAD);
    cc.add_const_f32("CHATEDIT_HEIGHT", CHATEDIT_HEIGHT);
    cc.add_const_f32("LABEL_LINESPACE", LABEL_LINESPACE);
    cc.add_const_f32("HEADER_HEIGHT", HEADER_HEIGHT);
    cc.add_const_f32("CONTENT_MARGIN", CONTENT_MARGIN);
    cc.add_const_f32("CONTENT_OUTLINE_SIZE", CONTENT_OUTLINE_SIZE);
    cc.add_const_f32("MENU_BTN_W_L", MENU_BTN_W_L);
    cc.add_const_f32("COPY_WIDTH", COPY_WIDTH);
    cc.add_const_f32("COPY_BTN_SIZE", COPY_BTN_SIZE);
    cc.add_const_f32("CHANNEL_ITEM_HEIGHT", CHANNEL_ITEM_HEIGHT);

    let renderer = app.renderer.clone();
    let atom = &mut renderer.make_guard(gfxtag!("write_click"));

    // Header
    let node = create_vector_art("header_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();
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
        expr::const_f32(BACKARROW_BG_W + 1.),
        expr::load_var("h"),
        sep_color,
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::load_var("h"),
        expr::load_var("w"),
        cc.compile("h + 1").unwrap(),
        CONTENT_OUTLINE_SIZE,
        sep_color,
    );
    let color1 = [0., 0.17, 0.18, 0.3];
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

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content.link(node);

    // Create back arrow
    let node = create_vector_art("back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, BACKARROW_SCALE).unwrap();
    prop.set_f32(atom, Role::App, 3, BACKARROW_SCALE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content.link(node);

    // Create back button
    let node = create_button("back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 10).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 10).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, BACKARROW_BG_W).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let sg_root = app.sg_root.clone();
    let contact_vis = contact_is_visible.clone();
    let channel_vis = channel_is_visible.clone();
    let renderer = app.renderer.clone();
    let menu_node = sg_root.lookup_node("/window/content/menu_layer").unwrap();
    let netstatus_layer = sg_root.lookup_node("/window/content/netstatus_layer").unwrap();
    let goback = async move || {
        info!(target: "app::chat", "clicked back");
        let atom = &mut renderer.make_guard(gfxtag!("go back action"));

        menu_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        netstatus_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

        contact_vis.set(atom, false);
        channel_vis.set(atom, false);
    };

    let (slot, recvr) = Slot::new("back_clicked");
    node.register("click", slot).unwrap();
    let goback2 = goback.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback2().await;
        }
    });
    content.push_task(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    content.link(node);

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
    let listen_enter = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback().await;
        }
    });
    content.push_task(listen_enter);

    let node = node.setup(|me| Shortcut::new(me)).await;
    content.link(node);

    let content_area_node = create_layer("content_area");
    let prop = content_area_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CONTENT_MARGIN).unwrap();
    prop.set_f32(atom, Role::App, 1, HEADER_HEIGHT + CONTENT_MARGIN).unwrap();
    prop.set_expr(atom, Role::App, 2, cc.compile("w - 2. * CONTENT_MARGIN").unwrap()).unwrap();
    prop.set_expr(
        atom,
        Role::App,
        3,
        cc.compile("h - HEADER_HEIGHT - CONTENT_MARGIN - 25.").unwrap(),
    )
    .unwrap();
    content_area_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    content_area_node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let content_area = content_area_node.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(content_area.clone());

    // Red bottom glow below outline
    let node = create_vector_art("bottom_glow");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CONTENT_MARGIN).unwrap();
    prop.set_expr(atom, Role::App, 1, cc.compile("h - 15").unwrap()).unwrap();
    prop.set_expr(atom, Role::App, 2, cc.compile("w - 2. * CONTENT_MARGIN").unwrap()).unwrap();
    prop.set_f32(atom, Role::App, 3, 15.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = VectorShape::new();

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content.link(node);

    // Channels label bg
    let node = create_vector_art("channels_label_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let x1 = expr::const_f32(0.);
    let y1 = expr::const_f32(0.);
    let x2 = expr::load_var("w");
    let y2 = cc.compile("LABEL_LINESPACE").unwrap();
    let (color1, color2) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0.11, 0.11, 0.], [0., 0., 0., 0.]),
        ColorScheme::PaperLight => ([1., 1., 1., 0.], [1., 1., 1., 0.]),
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

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("active_tab_overlay");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_outline(
        cc.compile("w / 2").unwrap(),
        expr::const_f32(0.),
        cc.compile("w / 2 + CONTENT_OUTLINE_SIZE").unwrap(),
        cc.compile("LABEL_LINESPACE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    shape.add_outline(
        cc.compile("w / 2").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        cc.compile("CONTENT_OUTLINE_SIZE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    shape.add_outline(
        cc.compile("w - CONTENT_OUTLINE_SIZE").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        cc.compile("LABEL_LINESPACE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("active_tab_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        cc.compile("w / 2").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        cc.compile("LABEL_LINESPACE").unwrap(),
        [0., 0., 0., 0.5],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("inactive_tab_overlay");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        cc.compile("w / 2").unwrap(),
        cc.compile("LABEL_LINESPACE").unwrap(),
        [0.3, 0.3, 0.3, 0.2],
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        cc.compile("CONTENT_OUTLINE_SIZE").unwrap(),
        cc.compile("LABEL_LINESPACE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_INACTIVE,
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        cc.compile("w / 2").unwrap(),
        cc.compile("CONTENT_OUTLINE_SIZE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_INACTIVE,
    );
    shape.add_outline(
        cc.compile("w / 2 - CONTENT_OUTLINE_SIZE").unwrap(),
        expr::const_f32(0.),
        cc.compile("w / 2").unwrap(),
        cc.compile("LABEL_LINESPACE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_INACTIVE,
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("input_area_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("8. * CHATEDIT_PAD + 4. * CHATEDIT_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        cc.compile("1").unwrap(),
        expr::const_f32(0.),
        cc.compile("w - 1").unwrap(),
        expr::load_var("h"),
        [0., 0., 0., 0.5],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("fullscreen_label_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_outline(
        cc.compile("w - CONTENT_OUTLINE_SIZE").unwrap(),
        cc.compile("LABEL_LINESPACE").unwrap(),
        expr::load_var("w"),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    shape.add_outline(
        expr::const_f32(0.),
        cc.compile("h - CONTENT_OUTLINE_SIZE").unwrap(),
        expr::load_var("w"),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    shape.add_outline(
        expr::const_f32(0.),
        cc.compile("LABEL_LINESPACE").unwrap(),
        cc.compile("CONTENT_OUTLINE_SIZE").unwrap(),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    shape.add_outline(
        cc.compile("w - CONTENT_OUTLINE_SIZE").unwrap(),
        cc.compile("LABEL_LINESPACE").unwrap(),
        expr::load_var("w"),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    shape.add_outline(
        expr::const_f32(0.),
        cc.compile("LABEL_LINESPACE").unwrap(),
        cc.compile("w / 2").unwrap(),
        cc.compile("LABEL_LINESPACE + CONTENT_OUTLINE_SIZE").unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );
    // second horizontal line + gradient
    shape.add_outline(
        expr::const_f32(0.),
        cc.compile("LABEL_LINESPACE + 5. * CHATEDIT_PAD + 3. * CHATEDIT_HEIGHT").unwrap(),
        expr::load_var("w"),
        cc.compile(
            "LABEL_LINESPACE + 5. * CHATEDIT_PAD + 3. * CHATEDIT_HEIGHT + CONTENT_OUTLINE_SIZE",
        )
        .unwrap(),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_button("channels_tab_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 2).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    let code = cc.compile("w").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, LABEL_LINESPACE).unwrap();

    let (slot, recvr) = Slot::new("channelsbtn_clicked");
    node.register("click", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let contact_vis = contact_is_visible.clone();
    let channel_vis = channel_is_visible.clone();
    let renderer = app.renderer.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("channels_click"));
            contact_vis.set(atom, false);
            channel_vis.set(atom, true);

            let netstatus_layer = sg_root.lookup_node("/window/content/netstatus_layer").unwrap();
            netstatus_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            debug!("channels btn click - switching to channels");
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_text("channels_tab_text");
    let prop = node.get_property("rect").unwrap();
    #[cfg(any(target_os = "android", feature = "emulate-android"))]
    {
        let code = cc.compile("w / 2 + CONTENT_MARGIN * 3.0").unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        prop.set_f32(atom, Role::App, 1, CONTENT_MARGIN * 1.4).unwrap();
        let code = cc.compile("w").unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        prop.set_f32(atom, Role::App, 3, 40.).unwrap();
    }
    #[cfg(not(any(target_os = "android", feature = "emulate-android")))]
    {
        let code = cc.compile("w / 2 + CONTENT_MARGIN * 3.0").unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        prop.set_f32(atom, Role::App, 1, CONTENT_MARGIN * 1.15).unwrap();
        let code = cc.compile("w").unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        prop.set_f32(atom, Role::App, 3, 40.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", LABEL_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "CHANNELS").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_MINT[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_MINT[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_MINT[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_MINT[3]).unwrap();

    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    content_area.link(node);

    let node = create_button("contacts_tab_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 2).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    let code = cc.compile("w / 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, LABEL_LINESPACE).unwrap();

    let (slot, recvr) = Slot::new("contactsbtn_clicked");
    node.register("click", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let contact_vis = contact_is_visible.clone();
    let channel_vis = channel_is_visible.clone();
    let renderer = app.renderer.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("contacts_click"));
            channel_vis.set(atom, false);
            contact_vis.set(atom, true);

            let netstatus_layer = sg_root.lookup_node("/window/content/netstatus_layer").unwrap();
            netstatus_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            debug!("contacts btn click - switching to contacts");
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_text("contacts_tab_text");
    let prop = node.get_property("rect").unwrap();
    #[cfg(any(target_os = "android", feature = "emulate-android"))]
    {
        prop.set_f32(atom, Role::App, 0, CONTENT_MARGIN * 3.0).unwrap();
        prop.set_f32(atom, Role::App, 1, CONTENT_MARGIN * 1.4).unwrap();
        prop.set_f32(atom, Role::App, 2, 200.).unwrap();
        prop.set_f32(atom, Role::App, 3, 40.).unwrap();
    }
    #[cfg(not(any(target_os = "android", feature = "emulate-android")))]
    {
        prop.set_f32(atom, Role::App, 0, CONTENT_MARGIN * 3.0).unwrap();
        prop.set_f32(atom, Role::App, 1, CONTENT_MARGIN * 1.15).unwrap();
        prop.set_f32(atom, Role::App, 2, 200.).unwrap();
        prop.set_f32(atom, Role::App, 3, 40.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", LABEL_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "CONTACTS").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_INACTIVE[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_INACTIVE[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_INACTIVE[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_INACTIVE[3]).unwrap();
    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    content_area.link(node);

    let node = create_singleline_edit("channel_search");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 7. * CHATEDIT_PAD + 3. * CHATEDIT_HEIGHT)
        .unwrap();
    let code = cc.compile("parent_w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, TEXTBAR_BASELINE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, 15.).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE * 0.88).unwrap();

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
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.8).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.027).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.039).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.039).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.6).unwrap();
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

    node.set_property_str(atom, Role::App, "placeholder_text", "search").unwrap();
    let prop = node.get_property("placeholder_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_MINT_OP[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_MINT_OP[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_MINT_OP[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_MINT_OP[3]).unwrap();

    let node = node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                BaseEditType::SingleLine,
                app.ex.clone(),
            )
        })
        .await;
    let search_node = node.clone();
    content_area.link(node);

    let node = create_vector_art("search_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 7. * CHATEDIT_PAD + 3. * CHATEDIT_HEIGHT)
        .unwrap();
    let code = cc.compile("w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 4).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 0., 0., 0.5],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("search_outline");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 7. * CHATEDIT_PAD + 3. * CHATEDIT_HEIGHT)
        .unwrap();
    let code = cc.compile("w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 7).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        0.5,
        [0.3, 0.3, 0.3, 1.],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_singleline_edit("nick_edit");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + CHATEDIT_PAD).unwrap();
    let code = cc.compile("parent_w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, TEXTBAR_BASELINE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, 15.).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE * 0.88).unwrap();

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
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.8).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.027).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.039).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.039).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.6).unwrap();
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
    node.set_property_u32(atom, Role::App, "z_index", 5).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 2).unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();

    node.set_property_str(atom, Role::App, "placeholder_text", "#NAME").unwrap();
    let prop = node.get_property("placeholder_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_MINT_OP[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_MINT_OP[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_MINT_OP[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_MINT_OP[3]).unwrap();

    let node = node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                BaseEditType::SingleLine,
                app.ex.clone(),
            )
        })
        .await;
    let nickedit_node = node.clone();
    content_area.link(node);

    let node = create_vector_art("nick_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + CHATEDIT_PAD).unwrap();
    let code = cc.compile("w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 4).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 0., 0., 0.5],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("nick_outline");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + CHATEDIT_PAD).unwrap();
    let code = cc.compile("w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 7).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        [0.3, 0.3, 0.3, 1.],
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_singleline_edit("secret_edit");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 2. * CHATEDIT_PAD + CHATEDIT_HEIGHT)
        .unwrap();
    let code = cc.compile("parent_w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, TEXTBAR_BASELINE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, 15.).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", TEXTBAR_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE * 0.88).unwrap();

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
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.8).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.027).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.039).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.039).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.6).unwrap();
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

    node.set_property_str(atom, Role::App, "placeholder_text", "KEY (OPTIONAL)").unwrap();
    let prop = node.get_property("placeholder_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_MINT_OP[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_MINT_OP[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_MINT_OP[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_MINT_OP[3]).unwrap();

    let node = node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                BaseEditType::SingleLine,
                app.ex.clone(),
            )
        })
        .await;
    let secedit_node = node.clone();
    content_area.link(node);

    let node = create_vector_art("secret_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 2. * CHATEDIT_PAD + CHATEDIT_HEIGHT)
        .unwrap();
    let code = cc.compile("w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 4).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 0., 0., 0.5],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("secret_outline");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 2. * CHATEDIT_PAD + CHATEDIT_HEIGHT)
        .unwrap();
    let code = cc.compile("w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 7).unwrap();

    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        [0.3, 0.3, 0.3, 1.],
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_vector_art("receive_copy_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - CHATEDIT_PAD - COPY_BTN_SIZE / 2.").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(
        atom,
        Role::App,
        1,
        LABEL_LINESPACE + 2. * CHATEDIT_PAD + CHATEDIT_HEIGHT + COPY_BTN_SIZE / 2.,
    )
    .unwrap();
    prop.set_f32(atom, Role::App, 2, COPY_BTN_SIZE).unwrap();
    prop.set_f32(atom, Role::App, 3, COPY_BTN_SIZE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 8).unwrap();

    let shape = shape::create_copy(COLOR_CYAN).scaled(COPY_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    content_area.link(node);

    // paste clipboard into the KEY field
    let node = create_button("receive_copy_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - CHATEDIT_PAD - COPY_BTN_SIZE").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 2. * CHATEDIT_PAD + CHATEDIT_HEIGHT)
        .unwrap();
    prop.set_f32(atom, Role::App, 2, COPY_BTN_SIZE).unwrap();
    prop.set_f32(atom, Role::App, 3, COPY_BTN_SIZE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 9).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 5).unwrap();

    let (slot, recvr) = Slot::new("receive_copy_clicked");
    node.register("click", slot).unwrap();
    let secedit_node2 = secedit_node.clone();
    let renderer2 = app.renderer.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            debug!(target: "app::menu", "secret paste button clicked");
            match miniquad::window::clipboard_get() {
                Some(clipboard_text) => {
                    let text_prop = secedit_node2.get_property("text").unwrap();
                    let atom = &mut renderer2.make_guard(gfxtag!("secret paste"));
                    text_prop.set_str(atom, Role::App, 0, &clipboard_text).unwrap();
                    if let crate::scene::Pimpl::Edit(edit) = secedit_node2.pimpl() {
                        edit.on_text_prop_changed();
                    }
                }
                None => warn!(target: "app::menu", "clipboard_get() returned None (empty or unsupported on this platform)"),
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    content_area.link(node);

    let node = create_layer("addchannel_btn_layer");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - CHATEDIT_PAD - MENU_BTN_W_L - 45").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("LABEL_LINESPACE + 3. * CHATEDIT_PAD + 2. * CHATEDIT_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("MENU_BTN_W_L + 45").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT * 0.95).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let editlayer_node = node.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content_area.link(editlayer_node.clone());

    let node = create_vector_art("btns_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(MENU_BTN_W_L + 45.),
        expr::load_var("h"),
        CONTENT_OUTLINE_SIZE,
        COLOR_CYAN,
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    editlayer_node.link(node);

    let mut node = create_button("addchannel_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    let code = cc.compile("MENU_BTN_W_L + 45").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

    let (slot, addchannel_recvr) = Slot::new("add_channel_clicked_handler");
    node.register("click", slot).unwrap();

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    editlayer_node.link(node.clone());

    let addchannel_btn = node;

    let node = create_text("add_channel");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, BTN_TEXT_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_L + 45.).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE * 0.95).unwrap();
    node.set_property_str(atom, Role::App, "text", "add channel").unwrap();
    let prop = node.get_property("text_align").unwrap();
    prop.set_enum(atom, Role::App, 0, "center").unwrap();
    node.set_property_bool(atom, Role::App, "use_i18n", false).unwrap();

    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();

    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    editlayer_node.link(node);

    // Create cancel/done edit buttons at bottom of screen
    let btns =
        edit_buttons::create_edit_buttons(app, content.clone(), &window_scale, i18n_fish).await;

    let node = create_menu("nick_menu");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, LABEL_LINESPACE + 8. * CHATEDIT_PAD + 4. * CHATEDIT_HEIGHT)
        .unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code =
        cc.compile("h - (LABEL_LINESPACE + 8. * CHATEDIT_PAD + 4. * CHATEDIT_HEIGHT)").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 0).unwrap();

    let prop = node.get_property("bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.5).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", FONTSIZE).unwrap();
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
    prop.set_f32(atom, Role::App, 0, LABEL_X).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_ITEM_HEIGHT / 2.).unwrap();

    node.set_property_f32(atom, Role::App, "handle_padding", MENU_HANDLE_PAD).unwrap();
    node.set_property_f32(atom, Role::App, "fade_zone", MENU_FADE).unwrap();

    let prop = node.get_property("items").unwrap();
    let mut channel_names: Vec<String> = vec![];
    for item in channels_tree.iter() {
        let (_key, val) = item.unwrap();
        let channel = deserialize::<Channel>(&val).unwrap();
        channel_names.push(format!("#{}", channel.name));
    }
    channel_names.sort();
    for channel_name in channel_names {
        prop.push_str(atom, Role::App, &channel_name).unwrap();
    }

    let menu_node =
        node.setup(|me| Menu::new(me, window_scale.clone(), app.renderer.clone())).await;
    content_area.link(menu_node.clone());

    // Setup add_channel button handler now that menu_node exists
    let channels_tree2 = channels_tree.clone();
    let nickedit2 = nickedit_node.clone();
    let secedit2 = secedit_node.clone();
    let menu_prop2 = menu_node.get_property("items").unwrap();
    let renderer2 = app.renderer.clone();
    let sg_root2 = app.sg_root.clone();

    let save_channel = app.ex.spawn(async move {
        while let Ok(_) = addchannel_recvr.recv().await {
            let name_prop = nickedit2.get_property("text").unwrap();
            let name = name_prop.get_str(0).unwrap();

            let secret_prop = secedit2.get_property("text").unwrap();
            let secret = secret_prop.get_str(0).unwrap();

            if name.is_empty() {
                w!("Attempted to add channel with empty name");
                continue;
            }
            // TODO: Do more thorough checking of channel names

            let name = if name.starts_with('#') { name.trim_start_matches('#') } else { &name };

            let channel_name = format!("#{}", name);

            let mut channel = Channel { name: name.to_string(), secret: None };

            // Try to decode secret field if it is set
            if !secret.is_empty() {
                let Ok(bytes) = bs58::decode(&secret).into_vec() else {
                    w!("Failed to decode secret base58");
                    continue
                };
                if bytes.len() != 32 {
                    w!("Invalid secret length: {} bytes (expected 32)", bytes.len());
                    continue
                }

                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                channel.secret = Some(arr);
            }

            let mut val = vec![];
            channel.encode(&mut val).unwrap();

            let key = name;

            channels_tree2.insert(key, val).unwrap();
            let _ = channels_tree2.flush_async().await;

            let atom = &mut renderer2.make_guard(gfxtag!("add_channel"));
            menu_prop2.push_str(atom, Role::App, &channel_name).unwrap();

            i!("Successfully saved channel: {}", channel_name);

            let atom = &mut renderer2.make_guard(gfxtag!("clear_channel_fields"));
            name_prop.set_str(atom, Role::App, 0, "").unwrap();
            secret_prop.set_str(atom, Role::App, 0, "").unwrap();
        }
    });

    app.tasks.lock().unwrap().push(save_channel);

    // Connect cancel/done buttons and edit_active signal
    btns.connect_edit_handlers(app, &menu_node, None);

    // Only one input field may be focused (caret visible)
    edit_switch(
        &mut app.tasks.lock().unwrap(),
        &[search_node, nickedit_node, secedit_node],
        app.ex.clone(),
    );

    // Register select signal on nick_menu
    let (slot, recvr) = Slot::new("channel_selected");
    menu_node.register("select", slot).unwrap();

    let sg_root = app.sg_root.clone();
    let renderer = app.renderer.clone();
    let ex = app.ex.clone();
    let channels_tree2 = channels_tree.clone();
    let content2 = content.clone();
    let channel_vis = channel_is_visible.clone();
    let window_scale2 = window_scale.clone();
    let db2 = db.clone();
    let i18n_fish2 = i18n_fish.clone();
    let emoji_meshes2 = emoji_meshes.clone();

    let listen_select = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let channel: String = deserialize(&data).unwrap();
            i!("Selected channel: {channel}");
            let path = format!("/window/content/{}_chat_layer", &channel);

            let atom = &mut renderer.make_guard(gfxtag!("channel_selected"));

            // Check if chat layer already exists
            if let Some(node) = sg_root.lookup_node(&path) {
                node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                channel_vis.set(atom, false);
                continue;
            }

            let content = sg_root.lookup_node("/window/content").unwrap();
            // Create the chat layer and get the node
            let node = chat::make(
                &sg_root,
                &renderer,
                &ex,
                content,
                &channel,
                &db2,
                &i18n_fish2,
                emoji_meshes2.clone(),
                is_first_time,
            )
            .await;
            match node.pimpl() {
                Pimpl::Layer(layer) => layer.clone().start(ex.clone()).await,
                _ => panic!("wrong pimpl"),
            }
            d!("Added channel layer: {}", node.get_full_path().unwrap());

            // Show the chat layer
            node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            // Add to main_menu items (check if not already there)
            let main_menu = sg_root.lookup_node("/window/content/menu_layer/main_menu").unwrap();
            let items_prop = main_menu.get_property("items").unwrap();

            if !items_prop.contains_str(&channel) {
                items_prop.push_str(atom, Role::App, &channel).unwrap();
            }

            // Hide channel screen
            channel_vis.set(atom, false);

            // Force redraw so newly added node parent_rect gets set.
            // There are other ways to do this but this is easiest for now.
            // We can think later about doing this better.
            let win = sg_root.lookup_node("/window").unwrap();
            match win.pimpl() {
                Pimpl::Window(win) => win.draw(atom).await,
                _ => panic!("wrong pimpl"),
            }

            // Trigger rescan for this channel
            if let Some(darkirc) = sg_root.lookup_node("/plugin/darkirc") {
                let mut data = vec![];
                channel.encode(&mut data).unwrap();
                darkirc.call_method("rescan", data).await.unwrap();
            }
        }
    });

    app.tasks.lock().unwrap().push(listen_select);

    content
}
