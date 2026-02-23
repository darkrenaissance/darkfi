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

use super::{edit_switch::edit_switch, ColorScheme, CHANNELS, COLOR_SCHEME};
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
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{
        BaseEdit, BaseEditType, Button, Layer, Menu, ShapeVertex, Shortcut, Text, VectorArt,
        VectorShape,
    },
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
    pub const CHATEDIT_PAD: f32 = 80.;
    pub const CHATEDIT_HEIGHT: f32 = 140.;
    pub const TEXTBAR_BASELINE: f32 = 40.;
    pub const FONTSIZE: f32 = 50.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = 50.;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = 20.;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 50.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 20.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 10.;
    pub const ACTION_PADDING: f32 = 32.;
    pub const ACTION_SPACING: f32 = 8.;
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
    pub const CHATEDIT_PAD: f32 = 40.;
    pub const CHATEDIT_HEIGHT: f32 = 60.;
    pub const TEXTBAR_BASELINE: f32 = 25.;
    pub const FONTSIZE: f32 = 25.;
    pub const CHATEDIT_CURSOR_ASCENT: f32 = 25.;
    pub const CHATEDIT_CURSOR_DESCENT: f32 = 8.;
    pub const CHATEDIT_SELECT_ASCENT: f32 = 30.;
    pub const CHATEDIT_SELECT_DESCENT: f32 = 10.;
    pub const CHATEDIT_HANDLE_DESCENT: f32 = 35.;
    pub const ACTION_PADDING: f32 = 8.;
    pub const ACTION_SPACING: f32 = 4.;
}

use ui_consts::*;

pub async fn make(
    app: &App,
    content: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("CHATEDIT_PAD", CHATEDIT_PAD);
    cc.add_const_f32("CHANNEL_LABEL_LINESPACE", CHANNEL_LABEL_LINESPACE);

    let atom = &mut PropertyAtomicGuard::none();

    let layer_node = create_layer("contact_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
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
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        cc.compile("w / 2").unwrap(),
        expr::const_f32(CHANNEL_LABEL_LINESPACE),
        1.,
        [1., 0.2, 0.19, 1.],
    );
    shape.add_filled_box(
        cc.compile("w / 2").unwrap(),
        expr::const_f32(0.),
        cc.compile("w / 2 + 1").unwrap(),
        expr::load_var("h"),
        [0., 0., 0., 1.],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer_node.link(node);

    let node = create_button("chans_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    let code = cc.compile("w / 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHANNEL_LABEL_LINESPACE).unwrap();

    let (slot, recvr) = Slot::new("channelbtn_clicked");
    node.register("click", slot).unwrap();
    let renderer = app.renderer.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            //let atom = &mut renderer.make_guard(gfxtag!("chans_click"));
            debug!("chans btn click");
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(Button::new).await;
    layer_node.link(node);

    let node = create_singleline_edit("nick_edit");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, CHANNEL_LABEL_LINESPACE + CHATEDIT_PAD).unwrap();
    let code = cc.compile("parent_w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

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
    layer_node.link(node);

    let node = create_singleline_edit("secret_edit");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, CHATEDIT_PAD).unwrap();
    prop.set_f32(atom, Role::App, 1, 2. * (CHANNEL_LABEL_LINESPACE + CHATEDIT_PAD)).unwrap();
    let code = cc.compile("parent_w - 2 * CHATEDIT_PAD").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, CHATEDIT_HEIGHT).unwrap();

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
    layer_node.link(node);

    {
        let mut app_tasks = app.tasks.lock().unwrap();
        edit_switch(
            &mut app_tasks,
            &[nickedit_node, secedit_node],
            app.renderer.clone(),
            app.ex.clone(),
        );
    }

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

    let node = create_menu("nick_menu");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 3. * (CHANNEL_LABEL_LINESPACE + CHATEDIT_PAD)).unwrap();
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
    for nick in [
        "@john", "@stacy", "@barry", "@steve", "@obombo", "@xyz", "@lunar", "@fren", "@anon",
        "@anon1",
    ] {
        prop.push_str(atom, Role::App, nick).unwrap();
    }

    /*
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
    */

    // Subscribe to edit_active signal to hide version block
    let (edit_slot, edit_recvr) = Slot::new("edit_activated");
    node.register("edit_active", edit_slot).unwrap();
    let renderer = app.renderer.clone();
    let editlayer_is_visible2 = editlayer_is_visible.clone();
    let edit_listen = app.ex.spawn(async move {
        while let Ok(_) = edit_recvr.recv().await {
            debug!(target: "app::menu", "menu edit active");
            let atom = &mut renderer.make_guard(gfxtag!("edit_active"));
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

    layer_node
}
