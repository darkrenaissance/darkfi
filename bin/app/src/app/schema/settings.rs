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

use crate::{
    app::{
        node::{create_button, create_editbox, create_layer, create_text, create_vector_art},
        App,
    },
    expr::{self, Compiler},
    prop::{PropertyAtomicGuard, PropertyFloat32, PropertyStr, PropertyValue, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{Button, EditBox, Layer, ShapeVertex, Text, VectorArt, VectorShape},
    ExecutorPtr,
};

use super::{ColorScheme, COLOR_SCHEME};

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const SETTING_LABEL_X: f32 = 40.;
    pub const SETTING_LABEL_LINESPACE: f32 = 140.;
    pub const SETTING_LABEL_BASELINE: f32 = 82.;
    pub const SETTING_LABEL_FONTSIZE: f32 = 24.;
    pub const SETTING_EDIT_FONTSIZE: f32 = 24.;
    pub const SETTING_TITLE_X: f32 = 150.;
    pub const SETTING_TITLE_FONTSIZE: f32 = 40.;
    pub const SETTING_TITLE_BASELINE: f32 = 82.;
    pub const SEARCH_PADDING_X: f32 = 120.;
    pub const BORDER_RIGHT_SCALE: f32 = 5.;
    pub const CURSOR_ASCENT: f32 = 50.;
    pub const CURSOR_DESCENT: f32 = 20.;
    pub const SELECT_ASCENT: f32 = 50.;
    pub const SELECT_DESCENT: f32 = 20.;

    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 50.;
    pub const BACKARROW_Y: f32 = 70.;
    pub const BACKARROW_BG_W: f32 = 120.;
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
    pub const SETTING_LABEL_X: f32 = 20.;
    pub const SETTING_LABEL_LINESPACE: f32 = 60.;
    pub const SETTING_LABEL_BASELINE: f32 = 37.;
    pub const SETTING_LABEL_FONTSIZE: f32 = 14.;
    pub const SETTING_EDIT_FONTSIZE: f32 = 14.;
    pub const SETTING_TITLE_X: f32 = 100.;
    pub const SETTING_TITLE_FONTSIZE: f32 = 20.;
    pub const SETTING_TITLE_BASELINE: f32 = 37.;
    pub const SEARCH_PADDING_X: f32 = 80.;
    pub const BORDER_RIGHT_SCALE: f32 = 10.;
    pub const CURSOR_ASCENT: f32 = 24.;
    pub const CURSOR_DESCENT: f32 = 8.;
    pub const SELECT_ASCENT: f32 = 30.;
    pub const SELECT_DESCENT: f32 = 10.;

    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 26.;
    pub const BACKARROW_BG_W: f32 = 80.;
}

use ui_consts::*;

#[derive(Clone)]
struct Setting {
    name: String,
    node: SceneNodePtr,
}

impl Setting {
    fn value_as_string(&self) -> String {
        match &self.node.get_property("value").unwrap().get_value(0).ok().unwrap() {
            PropertyValue::Str(s) => s.clone(),
            PropertyValue::Uint32(i) => i.to_string(),
            PropertyValue::Bool(b) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            PropertyValue::Float32(fl) => fl.to_string(),
            _ => "unknown".to_string(),
        }
    }
    fn get_value(&self) -> PropertyValue {
        self.node.get_property("value").unwrap().get_value(0).ok().unwrap()
    }
    fn get_default(&self) -> PropertyValue {
        self.node.get_property("default").unwrap().get_value(0).ok().unwrap()
    }
    fn is_default(&self) -> bool {
        self.get_value() == self.get_default()
    }
    fn reset(&self) {
        let prop = self.node.get_property("value").unwrap();
        prop.set_raw_value(Role::App, 0, self.get_default()).unwrap();
    }
}

pub async fn make(app: &App, window: SceneNodePtr, _ex: ExecutorPtr) {
    let mut cc = Compiler::new();
    cc.add_const_f32("BORDER_RIGHT_SCALE", BORDER_RIGHT_SCALE);
    cc.add_const_f32("SEARCH_PADDING_X", SEARCH_PADDING_X);
    cc.add_const_f32("X_RATIO", 1. / 2.);
    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/setting/scale").unwrap(),
        Role::Internal,
        "value",
        0,
    )
    .unwrap();
    let atom = &mut PropertyAtomicGuard::new();

    // Main view
    let layer_node = create_layer("settings_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.link(layer_node.clone());

    let mut setting_y = 0.;

    // Create the toolbar bg
    let node = create_vector_art("toolbar_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let (bg_color, sep_color) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0.41, 0.6, 0.65, 1.]),
        ColorScheme::PaperLight => ([1., 1., 1., 1.], [0., 0.6, 0.65, 1.]),
    };
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(BACKARROW_BG_W),
        expr::load_var("h"),
        bg_color,
    );
    shape.add_filled_box(
        expr::const_f32(BACKARROW_BG_W),
        expr::const_f32(0.),
        expr::const_f32(BACKARROW_BG_W + 1.),
        expr::load_var("h"),
        sep_color,
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::load_var("h"),
        expr::load_var("w"),
        cc.compile("h + 1").unwrap(),
        sep_color,
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Create the back button
    let node = create_vector_art("back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    let node = create_button("back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, BACKARROW_BG_W).unwrap();
    prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();

    let sg_root = app.sg_root.clone();
    let goback = move || {
        let atom = &mut PropertyAtomicGuard::new();

        // Disable visilibity of all relevant window nodes
        // This is needed since for example all chats have a different node name.
        let windows = sg_root.lookup_node("/window/content").unwrap().get_children();
        let target_substrings = vec!["_chat_layer", "menu_layer", "settings_layer"];

        for node in windows.iter() {
            // Check if the node's name contains any of the target substrings
            if target_substrings.iter().any(|&s| node.name.contains(s)) {
                if let Err(e) = node.set_property_bool(atom, Role::App, "is_visible", false) {
                    debug!("Failed to set property 'is_visible' on node: {:?}", e);
                }
            }
        }

        // Go back to the dev channel
        let menu_node = sg_root.lookup_node("/window/content/dev_chat_layer").unwrap();
        menu_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    };

    let (slot, recvr) = Slot::new("back_clicked");
    node.register("click", slot).unwrap();
    let goback2 = goback.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback2();
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.link(node.clone());

    // Label: "SETTINGS" title
    let node = create_text("settings_label_fontsize");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, SETTING_TITLE_X).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", SETTING_TITLE_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", SETTING_TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "SETTINGS").unwrap();
    node.set_property_f32_vec(atom, Role::App, "text_color", vec![1., 1., 1., 1.]).unwrap();
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
    layer_node.link(node);

    // Search Bar Background
    let node = create_vector_art("emoji_picker_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("100").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("50")).unwrap();
    //prop.add_depend(&emoji_dynamic_h_prop, 0, "dynamic_h");
    node.set_property_u32(atom, Role::App, "z_index", 4).unwrap();

    let mut shape = VectorShape::new();

    // Top line
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(80.),
        expr::load_var("w"),
        expr::const_f32(1.),
        [0.41, 0.6, 0.65, 1.],
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Search Bar Input
    let editbox_node = create_editbox("search_input");
    editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
    let prop = editbox_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, SEARCH_PADDING_X).unwrap();
    prop.set_expr(atom, Role::App, 1, cc.compile("60 + 20").unwrap()).unwrap();
    prop.clone()
        .set_expr(atom, Role::App, 2, cc.compile("w - SEARCH_PADDING_X*2").unwrap())
        .unwrap();
    prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
    editbox_node.set_property_f32_vec(atom, Role::App, "text_color", vec![1., 1., 1., 1.]).unwrap();
    let prop = editbox_node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "cursor_ascent", CURSOR_ASCENT).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "cursor_descent", CURSOR_DESCENT).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "select_ascent", SELECT_ASCENT).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "select_descent", SELECT_DESCENT).unwrap();
    editbox_node
        .set_property_f32_vec(atom, Role::App, "hi_bg_color", vec![0.5, 0.5, 0.5, 1.])
        .unwrap();
    let prop = editbox_node.get_property("selected").unwrap();
    prop.set_null(atom, Role::App, 0).unwrap();
    prop.set_null(atom, Role::App, 1).unwrap();
    editbox_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "font_size", 16.).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "baseline", 16.).unwrap();

    // Search icon
    let node = create_vector_art("search_icon");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.clone()
        .set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE + SETTING_LABEL_LINESPACE / 2.)
        .unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_logo([1., 1., 1., 1.]).scaled(500.);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Search placeholder
    let node = create_text("search_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, SEARCH_PADDING_X).unwrap();
    prop.set_expr(atom, Role::App, 1, cc.compile("60 + 20").unwrap()).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", 16.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", 16.).unwrap();
    node.set_property_str(atom, Role::App, "text", "SEARCH...").unwrap();
    node.set_property_f32_vec(atom, Role::App, "text_color", vec![1., 1., 1., 0.45]).unwrap();
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
    layer_node.link(node);

    // Search settings counter
    let node = create_text("search_count");
    let prop = node.get_property("rect").unwrap();
    prop.set_expr(atom, Role::App, 0, cc.compile("w - 50").unwrap()).unwrap();
    prop.set_expr(atom, Role::App, 1, cc.compile("60 + 20").unwrap()).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", 16.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", 16.).unwrap();
    node.set_property_str(atom, Role::App, "text", "").unwrap();
    node.set_property_f32_vec(atom, Role::App, "text_color", vec![0., 0.94, 1., 1.]).unwrap();
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
    layer_node.link(node);

    let sg_root3 = app.sg_root.clone();
    let search = move || {
        let atom = &mut PropertyAtomicGuard::new();

        let path = "/window/content/settings_layer/search_input";
        let node = sg_root3.lookup_node(path.to_string()).unwrap();
        let search_string = node.get_property_str("text").unwrap();

        let path = "/window/content/settings_layer/search_label";
        let search_label_node = sg_root3.lookup_node(path.to_string()).unwrap();

        if search_string.len() > 0 {
            let _ = search_label_node.set_property_f32(atom, Role::App, "font_size", 0.);
        } else {
            let _ = search_label_node.set_property_f32(atom, Role::App, "font_size", 16.);
        }

        let path = "/window/content/settings_layer/settings";
        let node = sg_root3.lookup_node(path.to_string()).unwrap();
        let setting_nodes = node.get_children();
        let mut found_nodes = Vec::new();

        // Iterate through the nodes
        for node in setting_nodes.iter() {
            // Hide all nodes initially, no matter what
            let _ = node.set_property_bool(atom, Role::App, "is_visible", false);

            // Check if the node matches the search string
            if node.name.contains(&search_string.to_string()) {
                found_nodes.push(node); // Store matching nodes
                if let Err(e) = node.set_property_bool(atom, Role::App, "is_visible", true) {
                    debug!("Failed to set property 'is_visible' on node: {:?}", e);
                }
            }
        }

        // Set the `rect` property for each found node
        for (i, node) in found_nodes.iter().enumerate() {
            let prop = node.get_property("rect").unwrap();
            let y = i as f32 * 60. + 60. + 60.;
            prop.set_f32(atom, Role::App, 1, y).unwrap();
        }

        // Update the counter
        let counter_text = found_nodes.len().to_string();
        let path = "/window/content/settings_layer/search_count";
        let node = sg_root3.lookup_node(path.to_string()).unwrap();
        let _ = node.set_property_str(atom, Role::App, "text", &counter_text).unwrap();
    };

    // Handle searching
    let search_text = editbox_node.get_property("text").unwrap();
    let search_text_sub = search_text.subscribe_modify();
    let search2 = search.clone();
    let listen_search_text = app.ex.spawn(async move {
        while let Ok(_) = search_text_sub.receive().await {
            search2();
        }
    });
    app.tasks.lock().unwrap().push(listen_search_text);

    let node = editbox_node
        .setup(|me| {
            EditBox::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.link(node);

    // Search background
    let node = create_vector_art("search_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 60.).unwrap();
    prop.set_expr(atom, Role::App, 2, cc.compile("w  * 100").unwrap()).unwrap();
    prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let x1 = expr::const_f32(0.);
    let y1 = expr::const_f32(0.);
    let x2 = expr::load_var("w");
    let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);
    let (color1, color2) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0.11, 0.11, 0.4], [0., 0.11, 0.11, 0.5]),
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
        expr::const_f32(SETTING_LABEL_LINESPACE - 1.),
        expr::load_var("w"),
        expr::const_f32(SETTING_LABEL_LINESPACE),
        [0.15, 0.2, 0.19, 1.],
        //[0., 0.11, 0.11, 0.4],
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    let node = create_vector_art("search_bg2");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 60.).unwrap();
    prop.set_expr(atom, Role::App, 2, cc.compile("w  * 100").unwrap()).unwrap();
    prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE / 3.5).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let x1 = expr::const_f32(0.);
    let y1 = expr::const_f32(0.);
    let x2 = expr::load_var("w");
    let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);

    let (color1, color2) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0.94, 1., 0.4], [0., 0.3, 0.25, 0.0]),
        ColorScheme::PaperLight => ([0., 0.94, 1., 0.4], [0., 0.3, 0.25, 0.0]),
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
        expr::const_f32(SETTING_LABEL_LINESPACE - 1.),
        expr::load_var("w"),
        expr::const_f32(SETTING_LABEL_LINESPACE),
        [0.15, 0.2, 0.19, 1.],
    );

    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Create a BTreeMap to store settings
    let mut settings_map: BTreeMap<String, Arc<Setting>> = BTreeMap::new();

    // Get the app settings
    let app_setting_root = app.sg_root.lookup_node("/setting").unwrap();
    for setting in app_setting_root.get_children().iter() {
        let name = ["app", &setting.name.clone()].join(".");
        settings_map.insert(name.clone(), Arc::new(Setting { name, node: setting.clone() }));
    }

    // Get the settings from all plugins
    let sg_root_children = app.sg_root.clone().get_children();
    let plugin_node = sg_root_children.iter().find(|node| node.name == "plugin");
    if let Some(pnode) = plugin_node {
        for plugin in pnode.get_children().iter() {
            let plugin_children = plugin.get_children();
            let setting_root = plugin_children.iter().find(|node| node.name == "setting");
            if let Some(sroot) = setting_root {
                for setting in sroot.get_children().iter() {
                    let name = [plugin.name.clone(), setting.name.clone()].join(".");
                    settings_map
                        .insert(name.clone(), Arc::new(Setting { name, node: setting.clone() }));
                }
            }
        }
    }

    // Setting currently being edited
    let active_setting: Arc<Mutex<Option<Arc<Setting>>>> = Arc::new(Mutex::new(None));

    // Setting Layer
    // Contain a setting
    let settings_layer_node = create_layer("settings");
    let prop = settings_layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();
    settings_layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    settings_layer_node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let settings_layer_node = settings_layer_node
        .setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone()))
        .await;
    layer_node.link(settings_layer_node.clone());

    // Iterate over the map and process each setting
    for setting in settings_map.values() {
        let setting_clone = setting.clone();
        let setting_name = setting_clone.name.clone();
        let is_bool = matches!(setting_clone.get_value(), PropertyValue::Bool(_));

        setting_y += SETTING_LABEL_LINESPACE;

        // Setting Layer
        // Contain a setting
        let setting_layer_node = create_layer(&setting_name.to_string());
        let prop = setting_layer_node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, setting_y + 60.).unwrap();
        prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        setting_layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        setting_layer_node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        let setting_layer_node = setting_layer_node
            .setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone()))
            .await;
        settings_layer_node.link(setting_layer_node.clone());

        // Background Label
        let node = create_vector_art("key_bg");
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone()
            .set_expr(atom, Role::App, 2, cc.compile("w  * X_RATIO - BORDER_RIGHT_SCALE").unwrap())
            .unwrap();
        prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

        let mut shape = VectorShape::new();

        let x1 = expr::const_f32(0.);
        let y1 = expr::const_f32(0.);
        let x2 = expr::load_var("w");
        let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);
        let (color1, color2) = match COLOR_SCHEME {
            //ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0.11, 0.11, 1.]),
            ColorScheme::DarkMode => ([0., 0.11, 0.11, 0.4], [0., 0.11, 0.11, 0.5]),
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
            expr::const_f32(SETTING_LABEL_LINESPACE - 1.),
            expr::load_var("w"),
            expr::const_f32(SETTING_LABEL_LINESPACE),
            [0.15, 0.2, 0.19, 1.],
        );

        let node = node
            .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
            .await;
        setting_layer_node.link(node);

        if is_bool {
            // Background Value: Bool FALSE
            let node = create_vector_art("value_bg_bool_false");
            let prop = node.get_property("rect").unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    0,
                    cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
            node.set_property_bool(
                atom,
                Role::App,
                "is_visible",
                matches!(setting_clone.get_value(), PropertyValue::Bool(false)),
            )
            .unwrap();

            let mut shape = VectorShape::new();

            let x1 = expr::const_f32(0.);
            let y1 = expr::const_f32(0.);
            let x2 = expr::load_var("w");
            let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);

            let (color1, color2) = match COLOR_SCHEME {
                //ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0.11, 0.11, 1.]),
                ColorScheme::DarkMode => ([0.0, 0.04, 0.04, 0.0], [0.7, 0.0, 0.0, 0.15]),
                ColorScheme::PaperLight => ([0.0, 0.04, 0.04, 0.0], [0.7, 0.0, 0.0, 0.15]),
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
                expr::const_f32(SETTING_LABEL_LINESPACE - 1.),
                expr::load_var("w"),
                expr::const_f32(SETTING_LABEL_LINESPACE),
                [0.15, 0.2, 0.19, 1.],
            );

            let node = node
                .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                .await;
            setting_layer_node.link(node);

            // Background Value: Bool TRUE
            let node = create_vector_art("value_bg_bool_true");
            let prop = node.get_property("rect").unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    0,
                    cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
            node.set_property_bool(
                atom,
                Role::App,
                "is_visible",
                matches!(setting_clone.get_value(), PropertyValue::Bool(true)),
            )
            .unwrap();

            let mut shape = VectorShape::new();

            let x1 = expr::const_f32(0.);
            let y1 = expr::const_f32(0.);
            let x2 = expr::load_var("w");
            let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);

            let (color1, color2) = match COLOR_SCHEME {
                //ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0.11, 0.11, 1.]),
                ColorScheme::DarkMode => ([0., 0.3, 0.25, 0.0], [0., 0.3, 0.25, 0.5]),
                ColorScheme::PaperLight => ([0., 0.3, 0.25, 0.0], [0., 0.3, 0.25, 0.5]),
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
                expr::const_f32(SETTING_LABEL_LINESPACE - 1.),
                expr::load_var("w"),
                expr::const_f32(SETTING_LABEL_LINESPACE),
                [0.15, 0.2, 0.19, 1.],
            );

            let node = node
                .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                .await;
            setting_layer_node.link(node);
        } else {
            // Background Value
            let node = create_vector_art("value_bg");
            let prop = node.get_property("rect").unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    0,
                    cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
            node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            let mut shape = VectorShape::new();

            let x1 = expr::const_f32(0.);
            let y1 = expr::const_f32(0.);
            let x2 = expr::load_var("w");
            let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);

            let (color1, color2) = match COLOR_SCHEME {
                //ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0.11, 0.11, 1.]),
                ColorScheme::DarkMode => ([0., 0.02, 0.02, 0.5], [0., 0.04, 0.04, 0.7]),
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
                expr::const_f32(SETTING_LABEL_LINESPACE - 1.),
                expr::load_var("w"),
                expr::const_f32(SETTING_LABEL_LINESPACE),
                [0.15, 0.2, 0.19, 1.],
            );

            let node = node
                .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                .await;
            setting_layer_node.link(node);
        }

        // Label Key
        let label_value_node = create_text("key_label");
        let prop = label_value_node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, SETTING_LABEL_X).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap()).unwrap();
        prop.set_f32(atom, Role::App, 3, 100.).unwrap();
        label_value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        label_value_node
            .set_property_f32(atom, Role::App, "baseline", SETTING_LABEL_BASELINE)
            .unwrap();
        label_value_node
            .set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE)
            .unwrap();
        label_value_node.set_property_str(atom, Role::App, "text", setting_name.clone()).unwrap();
        if setting.is_default() {
            label_value_node
                .set_property_f32_vec(atom, Role::App, "text_color", vec![0.65, 0.87, 0.83, 1.])
                .unwrap();
        } else {
            label_value_node
                .set_property_f32_vec(atom, Role::App, "text_color", vec![1., 1., 1., 1.])
                .unwrap();
        }
        label_value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

        let label_value_node = label_value_node
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
        setting_layer_node.link(label_value_node);

        // Text edit
        let editbox_node = create_editbox("value_editbox");
        editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
        let prop = editbox_node.get_property("rect").unwrap();
        prop.clone()
            .set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE").unwrap())
            .unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.clone()
            .set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE").unwrap())
            .unwrap();
        prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "baseline", SETTING_LABEL_BASELINE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "font_size", SETTING_EDIT_FONTSIZE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "cursor_ascent", CURSOR_ASCENT).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "cursor_descent", CURSOR_DESCENT).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "select_ascent", SELECT_ASCENT).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "select_descent", SELECT_DESCENT).unwrap();
        editbox_node
            .set_property_f32_vec(atom, Role::App, "text_color", vec![0.7, 0.7, 0.7, 1.])
            .unwrap();
        editbox_node
            .set_property_f32_vec(atom, Role::App, "cursor_color", vec![0.5, 0.5, 0.5, 1.])
            .unwrap();
        editbox_node
            .set_property_f32_vec(atom, Role::App, "hi_bg_color", vec![0.5, 0.5, 0.5, 1.])
            .unwrap();
        let prop = editbox_node.get_property("selected").unwrap();
        prop.set_null(atom, Role::App, 0).unwrap();
        prop.set_null(atom, Role::App, 1).unwrap();
        editbox_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        editbox_node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
        editbox_node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "font_size", 0.).unwrap();

        let editz_text = PropertyStr::wrap(&editbox_node, Role::App, "text", 0).unwrap();

        // Handle enter pressed in the editbox
        {
            let (slot, recvr) = Slot::new("setting_enter_pressed");
            editbox_node.register("enter_pressed", slot).unwrap();
            let setting2 = setting.clone();
            let sg_root2 = setting_layer_node.clone();
            let active_setting2 = active_setting.clone();
            let editz_text2 = editz_text.clone();
            let listen_enter = app.ex.spawn(async move {
                while let Ok(_) = recvr.recv().await {
                    update_setting(
                        setting2.clone(),
                        sg_root2.clone(),
                        active_setting2.clone(),
                        editz_text2.clone(),
                    )
                    .await;
                    refresh_setting(setting2.clone(), sg_root2.clone());
                }
            });
            app.tasks.lock().unwrap().push(listen_enter);
        }

        let node = editbox_node
            .setup(|me| {
                EditBox::new(
                    me,
                    window_scale.clone(),
                    app.render_api.clone(),
                    app.text_shaper.clone(),
                    app.ex.clone(),
                )
            })
            .await;
        setting_layer_node.link(node);

        // Is this setting the one that is currently active
        let cloned_active_setting = active_setting.clone();
        let is_active_setting = match active_setting.lock().unwrap().as_ref() {
            Some(active) => Arc::ptr_eq(active, &setting),
            None => false,
        };

        if !is_active_setting {
            if is_bool {
                // Bool circle: FALSE
                let node = create_vector_art("bool_icon_bg_false");
                let prop = node.get_property("rect").unwrap();
                prop.clone()
                    .set_expr(
                        atom,
                        Role::App,
                        0,
                        cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE + 6").unwrap(),
                    )
                    .unwrap();
                prop.set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE / 2.).unwrap();
                prop.set_f32(atom, Role::App, 2, 0.).unwrap();
                prop.set_f32(atom, Role::App, 3, 0.).unwrap();
                node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                node.set_property_bool(
                    atom,
                    Role::App,
                    "is_visible",
                    matches!(setting_clone.get_value(), PropertyValue::Bool(false)),
                )
                .unwrap();

                let shape = shape::create_circle([0.9, 0.4, 0.4, 0.7]).scaled(5.);
                let node = node
                    .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                    .await;
                setting_layer_node.link(node);

                // Bool circle: TRUE
                let node = create_vector_art("bool_icon_bg_true");
                let prop = node.get_property("rect").unwrap();
                prop.clone()
                    .set_expr(
                        atom,
                        Role::App,
                        0,
                        cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE + 6").unwrap(),
                    )
                    .unwrap();
                prop.set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE / 2.).unwrap();
                prop.set_f32(atom, Role::App, 2, 0.).unwrap();
                prop.set_f32(atom, Role::App, 3, 0.).unwrap();
                node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                node.set_property_bool(
                    atom,
                    Role::App,
                    "is_visible",
                    matches!(setting_clone.get_value(), PropertyValue::Bool(true)),
                )
                .unwrap();

                let shape = shape::create_circle([0., 0.94, 1., 1.]).scaled(5.);
                let node = node
                    .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                    .await;
                setting_layer_node.link(node);

                // Label showing the setting's current value
                let value_node = create_text("value_label");
                let prop = value_node.get_property("rect").unwrap();
                prop.clone()
                    .set_expr(
                        atom,
                        Role::App,
                        0,
                        cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE + 20 + 6").unwrap(),
                    )
                    .unwrap();
                prop.set_f32(atom, Role::App, 1, 0.).unwrap();
                prop.clone()
                    .set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap())
                    .unwrap();
                prop.set_f32(atom, Role::App, 3, 100.).unwrap();
                value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                value_node
                    .set_property_f32(atom, Role::App, "baseline", SETTING_LABEL_BASELINE)
                    .unwrap();
                value_node
                    .set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE)
                    .unwrap();
                value_node
                    .set_property_str(atom, Role::App, "text", setting_clone.value_as_string())
                    .unwrap();
                if matches!(setting_clone.get_value(), PropertyValue::Bool(false)) {
                    value_node
                        .set_property_f32_vec(
                            atom,
                            Role::App,
                            "text_color",
                            vec![0.9, 0.4, 0.4, 1.],
                        )
                        .unwrap();
                } else {
                    value_node
                        .set_property_f32_vec(
                            atom,
                            Role::App,
                            "text_color",
                            vec![0.0, 0.94, 1., 1.],
                        )
                        .unwrap();
                }
                value_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

                let node = value_node
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
                setting_layer_node.link(node);
            } else {
                // Label showing the setting's current value
                let value_node = create_text("value_label");
                let prop = value_node.get_property("rect").unwrap();
                prop.clone()
                    .set_expr(
                        atom,
                        Role::App,
                        0,
                        cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE").unwrap(),
                    )
                    .unwrap();
                prop.set_f32(atom, Role::App, 1, 0.).unwrap();
                prop.clone()
                    .set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap())
                    .unwrap();
                prop.set_f32(atom, Role::App, 3, 100.).unwrap();
                value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                value_node
                    .set_property_f32(atom, Role::App, "baseline", SETTING_LABEL_BASELINE)
                    .unwrap();
                value_node
                    .set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE)
                    .unwrap();
                value_node
                    .set_property_str(atom, Role::App, "text", setting_clone.value_as_string())
                    .unwrap();
                value_node
                    .set_property_f32_vec(atom, Role::App, "text_color", vec![1., 1., 1., 1.])
                    .unwrap();
                value_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

                let node = value_node
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
                setting_layer_node.link(node);
            }

            // A wide button useful to select the current setting
            let node = create_button("selector_btn");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO").unwrap()).unwrap();
            prop.set_f32(atom, Role::App, 1, 0.).unwrap();
            prop.clone()
                .set_expr(atom, Role::App, 2, cc.compile("w * (1-X_RATIO)").unwrap())
                .unwrap();
            prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();

            let sg_root2 = app.sg_root.clone();
            let setting_clone2 = setting_clone.clone();
            let setting_root2 = setting_layer_node.clone();
            let select = move || {
                let atom = &mut PropertyAtomicGuard::new();
                let sg_root = sg_root2.clone();
                let mut lock = cloned_active_setting.lock().unwrap();

                let path = "/window/content/settings_layer/search_input";
                let node = sg_root.lookup_node(path).unwrap();
                //node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
                node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

                let was_active = if let Some(s) = lock.as_ref() {
                    let path = format!("/window/content/settings_layer/settings/{}", &s.name);
                    let old_node = sg_root.lookup_node(&path).unwrap();

                    let _was_active = s.name == setting_clone2.clone().name;

                    // Show the selected setting value label
                    // of the selected setting, if there's one
                    let node = old_node.lookup_node("/value_label").unwrap();
                    let text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
                    text.set(atom, &s.value_as_string());

                    // Hide the selected setting editbox
                    // of the selected setting, if there's one
                    if !_was_active {
                        let node = old_node.lookup_node("/value_editbox").unwrap();
                        node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
                        node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
                        node.set_property_f32(atom, Role::App, "font_size", 0.).unwrap();
                        node.set_property_str(atom, Role::App, "text", "").unwrap();
                    }

                    // Hide conftrm button
                    // (Bool settings don't have a confirm button so we have to check for it to
                    // not panic)
                    let is_bool =
                        matches!(lock.clone().unwrap().get_value(), PropertyValue::Bool(_));
                    if !is_bool {
                        old_node
                            .lookup_node("/confirm_btn_bg")
                            .unwrap()
                            .set_property_bool(atom, Role::App, "is_visible", false)
                            .unwrap();
                    }

                    _was_active
                } else {
                    false
                };

                // Update what setting active_setting points to
                *lock = Some(setting_clone2.clone());
                debug!("active setting set to: {}", lock.clone().unwrap().name);

                if is_bool {
                    // Hide the setting value label (set its text empty)
                    let editbox = setting_root2.lookup_node("/value_label").unwrap();
                    let label_text = PropertyStr::wrap(&editbox, Role::App, "text", 0).unwrap();

                    let value = setting_clone2.get_value();

                    setting_root2
                        .lookup_node("/value_bg_bool_true")
                        .unwrap()
                        .set_property_bool(atom, Role::App, "is_visible", false)
                        .unwrap();
                    setting_root2
                        .lookup_node("/value_bg_bool_false")
                        .unwrap()
                        .set_property_bool(atom, Role::App, "is_visible", false)
                        .unwrap();
                    setting_root2
                        .lookup_node("/bool_icon_bg_true")
                        .unwrap()
                        .set_property_bool(atom, Role::App, "is_visible", false)
                        .unwrap();
                    setting_root2
                        .lookup_node("/bool_icon_bg_false")
                        .unwrap()
                        .set_property_bool(atom, Role::App, "is_visible", false)
                        .unwrap();

                    if matches!(value, PropertyValue::Bool(false)) {
                        setting_clone2
                            .node
                            .set_property_bool(atom, Role::User, "value", true)
                            .unwrap();
                        label_text.set(atom, "TRUE");

                        setting_root2
                            .lookup_node("/value_bg_bool_true")
                            .unwrap()
                            .set_property_bool(atom, Role::App, "is_visible", true)
                            .unwrap();
                        setting_root2
                            .lookup_node("/bool_icon_bg_true")
                            .unwrap()
                            .set_property_bool(atom, Role::App, "is_visible", true)
                            .unwrap();

                        let node = setting_root2.lookup_node("/value_label").unwrap();
                        node.set_property_f32_vec(
                            atom,
                            Role::App,
                            "text_color",
                            vec![0., 0.94, 1., 1.],
                        )
                        .unwrap();
                    } else {
                        setting_clone2
                            .node
                            .set_property_bool(atom, Role::User, "value", false)
                            .unwrap();
                        label_text.set(atom, "FALSE");

                        setting_root2
                            .lookup_node("/value_bg_bool_false")
                            .unwrap()
                            .set_property_bool(atom, Role::App, "is_visible", true)
                            .unwrap();
                        setting_root2
                            .lookup_node("/bool_icon_bg_false")
                            .unwrap()
                            .set_property_bool(atom, Role::App, "is_visible", true)
                            .unwrap();

                        let node = setting_root2.lookup_node("/value_label").unwrap();
                        node.set_property_f32_vec(
                            atom,
                            Role::App,
                            "text_color",
                            vec![0.9, 0.4, 0.4, 1.],
                        )
                        .unwrap();
                    }
                } else {
                    // Hide the setting value label (set its text empty)
                    // TODO?: Visilibity property on labels
                    let editbox = setting_root2.lookup_node("/value_label").unwrap();
                    let label_text = PropertyStr::wrap(&editbox, Role::App, "text", 0).unwrap();
                    label_text.set(atom, "");

                    // Show the editbox
                    let node = setting_root2.lookup_node("/value_editbox").unwrap();
                    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
                    node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
                    node.set_property_f32(atom, Role::App, "font_size", 16.).unwrap();
                    if !was_active {
                        node.set_property_str(
                            atom,
                            Role::App,
                            "text",
                            setting_clone2.value_as_string(),
                        )
                        .unwrap();
                    }

                    // Show confirm button
                    setting_root2
                        .lookup_node("/confirm_btn_bg")
                        .unwrap()
                        .set_property_bool(atom, Role::App, "is_visible", true)
                        .unwrap();
                }

                refresh_setting(setting_clone2.clone(), setting_root2.clone());
            };

            {
                let (slot, recvr) = Slot::new("select_clicked");
                node.register("click", slot).unwrap();
                let select2 = select.clone();
                let listen_click = app.ex.spawn(async move {
                    while let Ok(_) = recvr.recv().await {
                        select2();
                    }
                });
                app.tasks.lock().unwrap().push(listen_click);

                let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
                setting_layer_node.link(node.clone());
            }
        }

        if is_bool {
            // Switch icon
            let node = create_vector_art("switch_btn_bg");
            let prop = node.get_property("rect").unwrap();
            prop.set_expr(atom, Role::App, 0, cc.compile("w - 50").unwrap()).unwrap();
            prop.set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE / 2.).unwrap();
            prop.set_f32(atom, Role::App, 2, 0.).unwrap();
            prop.set_f32(atom, Role::App, 3, 0.).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

            let shape = shape::create_switch([0., 0.94, 1., 1.]).scaled(10.);
            let node = node
                .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                .await;
            setting_layer_node.link(node);
        } else {
            // Confirm button
            let node = create_vector_art("confirm_btn_bg");
            let prop = node.get_property("rect").unwrap();
            prop.set_expr(atom, Role::App, 0, cc.compile("w - 50").unwrap()).unwrap();
            prop.set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE / 2.).unwrap();
            prop.set_f32(atom, Role::App, 2, 0.).unwrap();
            prop.set_f32(atom, Role::App, 3, 0.).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
            node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();

            let shape = shape::create_confirm([0., 0.94, 1., 1.]).scaled(10.);
            let node = node
                .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
                .await;
            setting_layer_node.link(node.clone());

            let node = create_button("confirm_btn");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.set_expr(atom, Role::App, 0, cc.compile("w - 100").unwrap()).unwrap();
            prop.set_f32(atom, Role::App, 1, 0.).unwrap();
            prop.set_f32(atom, Role::App, 2, 100.).unwrap();
            prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

            let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
            setting_layer_node.link(node.clone());

            // Handle confirm button click
            {
                let (slot, recvr) = Slot::new("confirm_clicked");
                node.register("click", slot).unwrap();
                let setting2 = setting.clone();
                let sg_root2 = setting_layer_node.clone();
                let active_setting2 = active_setting.clone();
                let editz_text2 = editz_text.clone();
                let listen_click = app.ex.spawn(async move {
                    while let Ok(_) = recvr.recv().await {
                        info!("confirm clicked");
                        update_setting(
                            setting2.clone(),
                            sg_root2.clone(),
                            active_setting2.clone(),
                            editz_text2.clone(),
                        )
                        .await;
                        refresh_setting(setting2.clone(), sg_root2.clone());
                    }
                });
                app.tasks.lock().unwrap().push(listen_click);
            }
        }

        // Reset icon
        let node = create_vector_art("reset_btn_bg");
        let prop = node.get_property("rect").unwrap();
        prop.set_expr(atom, Role::App, 0, cc.compile("w - 100").unwrap()).unwrap();
        prop.set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE / 2.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.).unwrap();
        node.set_property_bool(atom, Role::App, "is_visible", !setting.is_default()).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

        let shape = shape::create_reset([0., 0.94, 1., 1.]).scaled(15.);
        let node = node
            .setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone()))
            .await;
        setting_layer_node.link(node);

        let node = create_button("reset_btn");
        node.set_property_bool(atom, Role::App, "is_active", !setting.is_default()).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_expr(atom, Role::App, 0, cc.compile("w - 115").unwrap()).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 50.).unwrap();
        prop.set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

        let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
        setting_layer_node.link(node.clone());

        // Handle reset button click
        {
            let (slot, recvr) = Slot::new("reset_clicked");
            node.register("click", slot).unwrap();
            let setting2 = setting.clone();
            let sg_root2 = setting_layer_node.clone();
            let active_setting2 = active_setting.clone();
            let editz_text2 = editz_text.clone();
            let listen_click = app.ex.spawn(async move {
                while let Ok(_) = recvr.recv().await {
                    info!("reset clicked");
                    setting2.reset();

                    let atom = &mut PropertyAtomicGuard::new();

                    // Show the selected setting value label (set its text empty)
                    // of the selected setting, if there's one
                    let node = sg_root2.lookup_node("/value_label").unwrap();
                    let text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
                    text.set(atom, setting2.value_as_string());

                    let node = sg_root2.lookup_node("/value_editbox").unwrap();
                    node.set_property_str(atom, Role::App, "text", setting2.value_as_string())
                        .unwrap();

                    update_setting(
                        setting2.clone(),
                        sg_root2.clone(),
                        active_setting2.clone(),
                        editz_text2.clone(),
                    )
                    .await;
                    refresh_setting(setting2.clone(), sg_root2.clone());
                }
            });
            app.tasks.lock().unwrap().push(listen_click);
        }
    }

    let settings_node = app.sg_root.lookup_node("/window/content/settings_layer").unwrap();
    settings_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();

    // Searchbar results count
    let node = app.sg_root.lookup_node("/window/content/settings_layer/settings").unwrap();
    let counter_text = node.get_children().len().to_string();
    let node = app.sg_root.lookup_node("/window/content/settings_layer/search_count").unwrap();
    node.set_property_str(atom, Role::App, "text", &counter_text).unwrap();
}

fn refresh_setting(setting: Arc<Setting>, sn: SceneNodePtr) {
    let atom = &mut PropertyAtomicGuard::new();
    let is_bool = matches!(setting.get_value(), PropertyValue::Bool(_));

    let node = sn.lookup_node("/key_label").unwrap();
    if setting.clone().is_default() {
        node.set_property_f32_vec(atom, Role::App, "text_color", vec![0.65, 0.87, 0.83, 1.])
            .unwrap();
    } else {
        node.set_property_f32_vec(atom, Role::App, "text_color", vec![1., 1., 1., 1.]).unwrap();
    }

    let node = sn.lookup_node("/reset_btn_bg").unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", !setting.clone().is_default()).unwrap();
    let node = sn.lookup_node("/reset_btn").unwrap();
    node.set_property_bool(atom, Role::App, "is_active", !is_bool && !setting.clone().is_default())
        .unwrap();
}

async fn update_setting(
    setting: Arc<Setting>,
    sn: SceneNodePtr,
    active_setting: Arc<Mutex<Option<Arc<Setting>>>>,
    editz_text: PropertyStr,
) {
    let atom = &mut PropertyAtomicGuard::new();

    if let Some(node) = sn.lookup_node("/value_editbox") {
        node.set_property_f32(atom, Role::App, "font_size", 0.).unwrap();
        node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
        node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
    }

    match &setting.get_value() {
        PropertyValue::Uint32(_) => {
            let value_str = editz_text.get();
            let parsed = value_str.parse::<u32>();
            if let Ok(value) = parsed {
                if let Some(node) = sn.lookup_node("/value_label") {
                    node.set_property_str(atom, Role::App, "text", value_str).unwrap();
                }
                if let Some(node) = sn.lookup_node("/confirm_btn_bg") {
                    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                setting.node.set_property_u32(atom, Role::User, "value", value).unwrap();
                let mut active_setting_value = active_setting.lock().unwrap();
                *active_setting_value = None;
            }
        }
        PropertyValue::Float32(_) => {
            let value_str = editz_text.get();
            let parsed = value_str.parse::<f32>();
            if let Ok(value) = parsed {
                if let Some(node) = sn.lookup_node("/value_label") {
                    node.set_property_str(atom, Role::App, "text", value_str).unwrap();
                }
                if let Some(node) = sn.lookup_node("/confirm_btn_bg") {
                    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                setting.node.set_property_f32(atom, Role::User, "value", value).unwrap();
                let mut active_setting_value = active_setting.lock().unwrap();
                *active_setting_value = None;
            }
        }
        PropertyValue::Str(_) => {
            let value_str = editz_text.get();
            if let Some(node) = sn.lookup_node("/value_label") {
                node.set_property_str(atom, Role::App, "text", &value_str).unwrap();
            }
            if let Some(node) = sn.lookup_node("/confirm_btn_bg") {
                node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
            }
            setting.node.set_property_str(atom, Role::User, "value", value_str).unwrap();
            let mut active_setting_value = active_setting.lock().unwrap();
            *active_setting_value = None;
        }
        _ => {}
    };
}
