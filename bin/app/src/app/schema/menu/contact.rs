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

pub async fn make(app: &App, content: SceneNodePtr, i18n_fish: &I18nBabelFish) -> SceneNodePtr {
    let mut cc = expr::Compiler::new();

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
    shape.add_filled_box(
        cc.compile("w / 2").unwrap(),
        expr::const_f32(0.),
        cc.compile("w / 2 + 1").unwrap(),
        expr::load_var("h"),
        [0., 0., 0., 1.],
    );

    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer_node.link(node);

    layer_node
}

