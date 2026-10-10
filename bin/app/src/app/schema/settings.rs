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
        node::{
            create_button, create_dropdown, create_layer, create_singleline_edit, create_slider,
            create_text, create_vector_art,
        },
        schema::menu::edit_switch::edit_switch,
        App,
    },
    expr::{self, Compiler},
    gfx::gfxtag,
    prop::{
        PropertyAtomicGuard, PropertyFloat32, PropertyPtr, PropertyStr, PropertyType,
        PropertyValue, Role,
    },
    scene::{SceneNode, SceneNodePtr, Slot},
    shape,
    theme::wire_color,
    ui::{
        BaseEdit, BaseEditType, Button, Dropdown, Layer, ShapeVertex, Slider, Text, VectorArt,
        VectorShape,
    },
    util::i18n::I18nBabelFish,
};

use darkfi_serial::{deserialize, Decodable};

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const SETTING_LABEL_X: f32 = 40.;
    pub const SETTING_LABEL_LINESPACE: f32 = 140.;
    pub const SEARCH_BAR_Y: f32 = SETTING_LABEL_LINESPACE;
    pub const SEARCH_INPUT_FONTSIZE: f32 = 32.;
    pub const SEARCH_INPUT_BASELINE: f32 = 32.;
    pub const SETTING_LABEL_Y: f32 = (SETTING_LABEL_LINESPACE - 1.2 * SETTING_LABEL_FONTSIZE) / 2.;
    pub const SLIDER_PAD: f32 = 20.;
    pub const RESET_BTN_W: f32 = 70.;
    pub const RESET_GLYPH_X: f32 = 50.;
    pub const SLIDER_H: f32 = 52.;
    pub const SWITCH_X_OFFSET: f32 = 50.;
    pub const CONFIRM_X_OFFSET: f32 = 100.;
    pub const CONFIRM_BTN_W: f32 = 200.;
    pub const SETTING_LABEL_FONTSIZE: f32 = 24.;
    pub const SETTING_EDIT_FONTSIZE: f32 = 24.;
    pub const SETTING_TITLE_X: f32 = 150.;
    pub const SETTING_TITLE_FONTSIZE: f32 = 40.;
    pub const SETTING_TITLE_Y: f32 = (SETTING_LABEL_LINESPACE - 1.2 * SETTING_TITLE_FONTSIZE) / 2.;
    pub const SETTING_EDIT_BASELINE: f32 = 82.;
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
    pub const SEARCH_BAR_Y: f32 = SETTING_LABEL_LINESPACE;
    pub const SEARCH_INPUT_FONTSIZE: f32 = 16.;
    pub const SEARCH_INPUT_BASELINE: f32 = 16.;
    pub const SETTING_LABEL_Y: f32 = (SETTING_LABEL_LINESPACE - 1.2 * SETTING_LABEL_FONTSIZE) / 2.;
    pub const SLIDER_PAD: f32 = 10.;
    pub const RESET_BTN_W: f32 = 35.;
    pub const RESET_GLYPH_X: f32 = 25.;
    pub const SLIDER_H: f32 = 26.;
    pub const SWITCH_X_OFFSET: f32 = 25.;
    pub const CONFIRM_X_OFFSET: f32 = 50.;
    pub const CONFIRM_BTN_W: f32 = 100.;
    pub const SETTING_LABEL_FONTSIZE: f32 = 14.;
    pub const SETTING_EDIT_FONTSIZE: f32 = 14.;
    pub const SETTING_TITLE_X: f32 = 100.;
    pub const SETTING_TITLE_FONTSIZE: f32 = 20.;
    pub const SETTING_TITLE_Y: f32 = (SETTING_LABEL_LINESPACE - 1.2 * SETTING_TITLE_FONTSIZE) / 2.;
    pub const SETTING_EDIT_BASELINE: f32 = 37.;
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
    prop: PropertyPtr,
}

impl Setting {
    fn value_as_string(&self) -> String {
        match &self.prop.get_value(0).ok().unwrap() {
            PropertyValue::Str(s) => s.clone(),
            PropertyValue::Enum(s) => s.clone(),
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
        self.prop.get_value(0).ok().unwrap()
    }
    fn get_default(&self) -> PropertyValue {
        let def = self.prop.defaults.lock().unwrap()[0].clone();
        match def {
            PropertyValue::Str(s) if self.prop.typ == PropertyType::Enum => PropertyValue::Enum(s),
            v => v,
        }
    }
    fn is_default(&self) -> bool {
        self.get_value() == self.get_default()
    }
    fn is_bool(&self) -> bool {
        matches!(self.get_value(), PropertyValue::Bool(_))
    }
    fn reset(&self) {
        let atom = &mut PropertyAtomicGuard::none();
        self.prop.set_value(atom, Role::App, 0, self.get_default()).unwrap();
    }
}

pub async fn make(app: &App, window: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let mut cc = Compiler::new();
    cc.add_const_f32("BORDER_RIGHT_SCALE", BORDER_RIGHT_SCALE);
    cc.add_const_f32("SETTING_LABEL_X", SETTING_LABEL_X);
    cc.add_const_f32("SLIDER_PAD", SLIDER_PAD);
    cc.add_const_f32("RESET_BTN_W", RESET_BTN_W);
    cc.add_const_f32("RESET_GLYPH_X", RESET_GLYPH_X);
    cc.add_const_f32("SEARCH_PADDING_X", SEARCH_PADDING_X);
    cc.add_const_f32("SEARCH_BAR_Y", SEARCH_BAR_Y);
    cc.add_const_f32("SWITCH_X_OFFSET", SWITCH_X_OFFSET);
    cc.add_const_f32("CONFIRM_X_OFFSET", CONFIRM_X_OFFSET);
    cc.add_const_f32("CONFIRM_BTN_W", CONFIRM_BTN_W);
    cc.add_const_f32("X_RATIO", 1. / 2.);
    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/setting").unwrap(),
        Role::Internal,
        "win.scale",
        0,
    )
    .unwrap();
    let atom = &mut PropertyAtomicGuard::none();

    // Main view
    let layer_node = create_layer("settings_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let layer_node = layer_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    window.link(layer_node.clone());

    let mut setting_y = 0.;

    // Create the back button
    let node = create_vector_art("back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, BACKARROW_X).unwrap();
    prop.set_default_f32(1, BACKARROW_Y).unwrap();
    prop.set_default_f32(2, 0.).unwrap();
    prop.set_default_f32(3, 0.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_back_arrow([0., 1., 1., 1.]).scaled(BACKARROW_SCALE);
    let prop = node.get_property("shape").unwrap();
    prop.set_default_shape(0, shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node);

    let node = create_button("back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_f32(2, BACKARROW_BG_W).unwrap();
    prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();

    let sg_root = app.sg_root.clone();
    let goback = move || {
        let atom = &mut PropertyAtomicGuard::none();

        let settings_node = sg_root.lookup_node("/window/content/settings_layer").unwrap();
        settings_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
        let chat_node = sg_root.lookup_node("/window/content/chat").unwrap();
        chat_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    };

    let (slot, recvr) = Slot::new("back_clicked");
    node.register("click", slot).unwrap();
    let goback2 = goback.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback2();
        }
    });
    app.tasks.lock().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node.clone());

    // Label: "SETTINGS" title
    let node = create_text("settings_label_fontsize");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, SETTING_TITLE_X).unwrap();
    prop.set_default_f32(2, 1000.).unwrap();
    prop.set_default_f32(3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    prop.set_default_f32(1, SETTING_TITLE_Y).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, SETTING_TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "SETTINGS").unwrap();
    // Class wiring: the settings title follows the shared text token.
    let theme = app.sg_root.lookup_node("/theme").unwrap();
    wire_color(&node, "text_color", &theme, "text_color").unwrap();
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

    // Search Bar Background
    let node = create_vector_art("emoji_picker_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    let code = cc.compile("100").unwrap();
    prop.set_default_expr(1, code).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("50")).unwrap();
    //prop.add_depend(Role::App, &emoji_dynamic_h_prop, 0, "dynamic_h");
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

    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();

    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node);

    // Search Bar Input
    let mut edit_nodes: Vec<SceneNodePtr> = vec![];
    let editbox_node = create_singleline_edit("search_input");
    editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
    let prop = editbox_node.get_property("rect").unwrap();
    prop.set_default_f32(0, SEARCH_PADDING_X).unwrap();
    prop.set_default_expr(1, cc.compile("SEARCH_BAR_Y").unwrap()).unwrap();
    prop.clone()
        .set_expr(atom, Role::App, 2, cc.compile("parent_w - SEARCH_PADDING_X*2").unwrap())
        .unwrap();
    prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
    editbox_node
        .get_property("text_color")
        .unwrap()
        .set_default_f32_multi(&[0.92, 0.92, 0.92, 1.])
        .unwrap();
    let prop = editbox_node.get_property("cursor_color").unwrap();
    prop.set_default_f32_multi(&[0.5, 0.5, 0.5, 1.]).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "cursor_ascent", CURSOR_ASCENT).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "cursor_descent", CURSOR_DESCENT).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "select_ascent", SELECT_ASCENT).unwrap();
    editbox_node.set_property_f32(atom, Role::App, "select_descent", SELECT_DESCENT).unwrap();
    editbox_node
        .get_property("hi_bg_color")
        .unwrap()
        .set_default_f32_multi(&[0.5, 0.5, 0.5, 1.])
        .unwrap();
    editbox_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
    editbox_node
        .get_property("font_size")
        .unwrap()
        .set_default_f32(0, SEARCH_INPUT_FONTSIZE)
        .unwrap();
    editbox_node
        .get_property("baseline")
        .unwrap()
        .set_default_f32(0, SEARCH_INPUT_BASELINE)
        .unwrap();

    // Search icon
    let node = create_vector_art("search_icon");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, BACKARROW_X).unwrap();
    prop.clone()
        .set_f32(atom, Role::App, 1, SETTING_LABEL_LINESPACE + SETTING_LABEL_LINESPACE / 2.)
        .unwrap();
    prop.set_default_f32(2, 0.).unwrap();
    prop.set_default_f32(3, 0.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_logo([1., 1., 1., 1.]).scaled(500.);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node);

    // Search placeholder
    let node = create_text("search_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, SEARCH_PADDING_X).unwrap();
    prop.set_default_expr(1, cc.compile("SEARCH_BAR_Y + 20").unwrap()).unwrap();
    prop.set_default_f32(2, 1000.).unwrap();
    prop.set_default_f32(3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, 16.).unwrap();
    node.set_property_str(atom, Role::App, "text", "SEARCH...").unwrap();
    node.get_property("text_color").unwrap().set_default_f32_multi(&[1., 1., 1., 0.45]).unwrap();
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

    // Search settings counter
    let node = create_text("search_count");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_expr(0, cc.compile("w - 50").unwrap()).unwrap();
    prop.set_default_expr(1, cc.compile("SEARCH_BAR_Y + 20").unwrap()).unwrap();
    prop.set_default_f32(2, 1000.).unwrap();
    prop.set_default_f32(3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, 16.).unwrap();
    node.set_property_str(atom, Role::App, "text", "").unwrap();
    node.get_property("text_color")
        .unwrap()
        .set_default_f32_multi(&[0.75, 0.75, 0.75, 1.])
        .unwrap();
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

    let sg_root3 = app.sg_root.clone();
    let search = move || {
        let atom = &mut PropertyAtomicGuard::none();

        let path = "/window/content/settings_layer/search_input";
        let node = sg_root3.lookup_node(path.to_string()).unwrap();
        let search_string = node.get_property_str("text").unwrap();

        let path = "/window/content/settings_layer/search_label";
        let search_label_node = sg_root3.lookup_node(path.to_string()).unwrap();

        if !search_string.is_empty() {
            let _ = search_label_node.set_property_str(atom, Role::App, "text", "");
        } else {
            let _ = search_label_node.set_property_str(atom, Role::App, "text", "SEARCH...");
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
            let y = i as f32 * SETTING_LABEL_LINESPACE + 2. * SEARCH_BAR_Y;
            prop.set_default_f32(1, y).unwrap();
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
    app.tasks.lock().push(listen_search_text);

    let node = editbox_node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                app.redraw_trigger.clone(),
                BaseEditType::SingleLine,
                app.ex.clone(),
            )
        })
        .await;
    layer_node.link(node.clone());
    edit_nodes.push(node);

    // Search background
    let node = create_vector_art("search_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, SEARCH_BAR_Y).unwrap();
    prop.set_default_expr(2, cc.compile("w  * 100").unwrap()).unwrap();
    prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let x1 = expr::const_f32(0.);
    let y1 = expr::const_f32(0.);
    let x2 = expr::load_var("w");
    let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);
    let (color1, color2) = ([0., 0.11, 0.11, 0.4], [0., 0.11, 0.11, 0.5]);
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
        [0.12, 0.12, 0.12, 1.],
        //[0.07, 0.07, 0.07, 0.4],
    );

    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();

    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node);

    let node = create_vector_art("search_bg2");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, SEARCH_BAR_Y).unwrap();
    prop.set_default_expr(2, cc.compile("w  * 100").unwrap()).unwrap();
    prop.set_default_f32(3, SETTING_LABEL_LINESPACE / 3.5).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();

    let x1 = expr::const_f32(0.);
    let y1 = expr::const_f32(0.);
    let x2 = expr::load_var("w");
    let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);

    let (color1, color2) = ([0., 0.94, 1., 0.4], [0., 0.3, 0.25, 0.0]);

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

    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();

    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node);

    // Create a BTreeMap to store settings
    let mut settings_map: BTreeMap<String, Arc<Setting>> = BTreeMap::new();

    // Get the app settings
    let app_setting_root = app.sg_root.lookup_node("/setting").unwrap();
    for prop in app_setting_root.props.iter() {
        let name = prop.name.clone();
        settings_map.insert(name.clone(), Arc::new(Setting { name, prop: prop.clone() }));
    }

    // Get the settings from all plugins
    let sg_root_children = app.sg_root.clone().get_children();
    let plugin_node = sg_root_children.iter().find(|node| node.name == "plugin");
    if let Some(pnode) = plugin_node {
        for plugin in pnode.get_children().iter() {
            let plugin_children = plugin.get_children();
            let setting_root = plugin_children.iter().find(|node| node.name == "setting");
            if let Some(sroot) = setting_root {
                for prop in sroot.props.iter() {
                    let name = [plugin.name.clone(), prop.name.clone()].join(".");
                    settings_map
                        .insert(name.clone(), Arc::new(Setting { name, prop: prop.clone() }));
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
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();
    settings_layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    settings_layer_node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let settings_layer_node = settings_layer_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    layer_node.link(settings_layer_node.clone());

    // Iterate over the map and process each setting
    for (row_idx, setting) in settings_map.values().enumerate() {
        let setting_clone = setting.clone();
        let setting_name = setting_clone.name.clone();
        let is_bool = matches!(setting_clone.get_value(), PropertyValue::Bool(_));
        let is_enum = setting_clone.prop.typ == PropertyType::Enum;

        setting_y += SETTING_LABEL_LINESPACE;

        // Setting Layer
        // Contain a setting
        let setting_layer_node = create_layer(&setting_name.to_string());
        let prop = setting_layer_node.get_property("rect").unwrap();
        prop.set_default_f32(0, 0.).unwrap();
        prop.set_default_f32(1, setting_y + SEARCH_BAR_Y).unwrap();
        prop.set_default_expr(2, expr::load_var("w")).unwrap();
        prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
        setting_layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        setting_layer_node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        setting_layer_node
            .set_property_u32(atom, Role::App, "priority", (settings_map.len() - row_idx) as u32)
            .unwrap();
        let setting_layer_node = setting_layer_node
            .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
        settings_layer_node.link(setting_layer_node.clone());

        // Background Label
        let node = create_vector_art("key_bg");
        let prop = node.get_property("rect").unwrap();
        prop.set_default_f32(0, 0.).unwrap();
        prop.set_default_f32(1, 0.).unwrap();
        prop.clone()
            .set_expr(atom, Role::App, 2, cc.compile("w  * X_RATIO - BORDER_RIGHT_SCALE").unwrap())
            .unwrap();
        prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

        let mut shape = VectorShape::new();

        let x1 = expr::const_f32(0.);
        let y1 = expr::const_f32(0.);
        let x2 = expr::load_var("w");
        let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);
        let (color1, color2) = ([0., 0.11, 0.11, 0.4], [0., 0.11, 0.11, 0.5]);
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

        node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
        let node = node
            .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
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
            prop.set_default_f32(1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
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

            let (color1, color2) = ([0.0, 0.04, 0.04, 0.0], [0.7, 0.0, 0.0, 0.15]);

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

            node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
            let node = node
                .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
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
            prop.set_default_f32(1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
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

            let (color1, color2) = ([0., 0.3, 0.25, 0.0], [0., 0.3, 0.25, 0.5]);

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

            node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
            let node = node
                .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
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
            prop.set_default_f32(1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
            node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            let mut shape = VectorShape::new();

            let x1 = expr::const_f32(0.);
            let y1 = expr::const_f32(0.);
            let x2 = expr::load_var("w");
            let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);

            let (color1, color2) = ([0., 0.02, 0.02, 0.5], [0., 0.04, 0.04, 0.7]);

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

            node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
            let node = node
                .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
                .await;
            setting_layer_node.link(node);
        }

        // Label Key
        let label_value_node = create_text("key_label");
        let prop = label_value_node.get_property("rect").unwrap();
        prop.set_default_f32(0, SETTING_LABEL_X).unwrap();
        prop.set_default_f32(1, 0.).unwrap();
        prop.set_default_expr(
            2,
            cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE - RESET_BTN_W - SETTING_LABEL_X").unwrap(),
        )
        .unwrap();
        prop.set_default_f32(3, 100.).unwrap();
        label_value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        prop.set_default_f32(1, SETTING_LABEL_Y).unwrap();
        label_value_node
            .set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE)
            .unwrap();
        label_value_node.set_property_str(atom, Role::App, "text", setting_name.clone()).unwrap();
        if setting.is_default() {
            label_value_node
                .get_property("text_color")
                .unwrap()
                .set_default_f32_multi(&[0.62, 0.62, 0.62, 1.])
                .unwrap();
        } else {
            label_value_node
                .get_property("text_color")
                .unwrap()
                .set_default_f32_multi(&[0.92, 0.92, 0.92, 1.])
                .unwrap();
        }
        label_value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

        let label_value_node = label_value_node
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
        setting_layer_node.link(label_value_node);

        let editz_text: Option<PropertyStr>;
        let mut row_edit_node: Option<SceneNodePtr> = None;
        if !is_enum {
            // Text edit
            let editbox_node = create_singleline_edit("value_editbox");
            editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
            let prop = editbox_node.get_property("rect").unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    0,
                    cc.compile("parent_w * X_RATIO + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("parent_w * X_RATIO - BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
            editbox_node
                .get_property("baseline")
                .unwrap()
                .set_default_f32(0, SETTING_EDIT_BASELINE)
                .unwrap();
            editbox_node
                .get_property("font_size")
                .unwrap()
                .set_default_f32(0, SETTING_EDIT_FONTSIZE)
                .unwrap();
            editbox_node.set_property_f32(atom, Role::App, "cursor_ascent", CURSOR_ASCENT).unwrap();
            editbox_node
                .set_property_f32(atom, Role::App, "cursor_descent", CURSOR_DESCENT)
                .unwrap();
            editbox_node.set_property_f32(atom, Role::App, "select_ascent", SELECT_ASCENT).unwrap();
            editbox_node
                .set_property_f32(atom, Role::App, "select_descent", SELECT_DESCENT)
                .unwrap();
            editbox_node
                .get_property("text_color")
                .unwrap()
                .set_default_f32_multi(&[0.7, 0.7, 0.7, 1.])
                .unwrap();
            editbox_node
                .get_property("cursor_color")
                .unwrap()
                .set_default_f32_multi(&[0.5, 0.5, 0.5, 1.])
                .unwrap();
            editbox_node
                .get_property("hi_bg_color")
                .unwrap()
                .set_default_f32_multi(&[0.5, 0.5, 0.5, 1.])
                .unwrap();
            editbox_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
            editbox_node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
            editbox_node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
            editbox_node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

            editz_text = Some(PropertyStr::wrap(&editbox_node, Role::App, "text", 0).unwrap());

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
                app.tasks.lock().push(listen_enter);
            }

            let node = editbox_node
                .setup(|me| {
                    BaseEdit::new(
                        me,
                        window_scale.clone(),
                        app.renderer.clone(),
                        app.redraw_trigger.clone(),
                        BaseEditType::SingleLine,
                        app.ex.clone(),
                    )
                })
                .await;
            setting_layer_node.link(node.clone());
            edit_nodes.push(node.clone());
            row_edit_node = Some(node);
        } else {
            editz_text = None;
            let value_prop = setting_clone.prop.clone();
            let items = value_prop.enum_items.clone().unwrap_or_default();
            let current = match setting_clone.get_value() {
                PropertyValue::Enum(cur) => cur,
                _ => String::new(),
            };
            let selected = items.iter().position(|item| *item == current).unwrap_or(0);

            let node = create_dropdown("value_dropdown");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            {
                let prop = node.get_property("items").unwrap();
                prop.set_str_vec(
                    atom,
                    Role::App,
                    items.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )
                .unwrap();
            }
            node.set_property_u32(atom, Role::App, "selected", selected as u32).unwrap();
            node.set_property_f32(atom, Role::App, "item_height", SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_f32(atom, Role::App, "list_width", 0.).unwrap();
            node.set_property_f32(atom, Role::App, "font_size", SETTING_EDIT_FONTSIZE).unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    0,
                    cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(1, 0.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE").unwrap(),
                )
                .unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
            let node = node
                .setup(|me| {
                    Dropdown::new(
                        me,
                        app.renderer.clone(),
                        app.redraw_trigger.clone(),
                        window_scale.clone(),
                    )
                })
                .await;
            setting_layer_node.link(node.clone());

            let (slot, selection_recvr) = Slot::new("dropdown_selection");
            node.register("selection_changed", slot).unwrap();
            let redraw = app.redraw_trigger.clone();
            let value_prop2 = value_prop.clone();
            let setting2 = setting.clone();
            let row_root2 = setting_layer_node.clone();
            let select_task = app.ex.spawn(async move {
                while let Ok(data) = selection_recvr.recv().await {
                    let Some((idx, item)) = decode_selection_payload(&data) else {
                        error!(target: "app::settings", "dropdown: bad selection payload");
                        continue
                    };
                    let _ = idx;
                    let atom = &mut redraw.make_guard(gfxtag!("dropdown selection"));
                    if let Err(err) = value_prop2.set_enum(atom, Role::User, 0, item) {
                        error!(target: "app::settings", "dropdown: failed to set enum: {err}");
                        continue
                    }
                    refresh_setting(setting2.clone(), row_root2.clone());
                }
            });
            setting_layer_node.push_task(select_task);
        }

        if setting_name == "win.scale" {
            let node = create_slider("value_slider");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            node.set_property_f32(atom, Role::App, "min", 0.8).unwrap();
            node.set_property_f32(atom, Role::App, "max", 1.2).unwrap();
            node.set_property_f32(atom, Role::App, "step", 0.02).unwrap();
            node.set_property_bool(atom, Role::App, "show_stepper", true).unwrap();

            let saved = match setting_clone.get_value() {
                PropertyValue::Float32(v) if (0.8..=1.2).contains(&v) => v,
                _ => 1.,
            };
            node.set_property_f32(atom, Role::App, "value", saved).unwrap();

            let prop = node.get_property("rect").unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    0,
                    cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE + SLIDER_PAD").unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 1, (SETTING_LABEL_LINESPACE - SLIDER_H) / 2.).unwrap();
            prop.clone()
                .set_expr(
                    atom,
                    Role::App,
                    2,
                    cc.compile("w - (w * X_RATIO - BORDER_RIGHT_SCALE + SLIDER_PAD) - SLIDER_PAD")
                        .unwrap(),
                )
                .unwrap();
            prop.set_f32(atom, Role::App, 3, SLIDER_H).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

            let mut thumb_shape = shape::create_thumb([1., 1., 1., 1.]);
            thumb_shape.join(shape::create_circle([0., 0.94, 1., 1.]).scaled(0.65));
            node.set_property_shape(atom, Role::App, "thumb_shape", thumb_shape).unwrap();

            spawn_win_scale_listener(app, setting.clone(), &setting_layer_node, &node);

            let node = node
                .setup(|me| Slider::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
                .await;
            setting_layer_node.link(node);
        }

        // Is this setting the one that is currently active
        let cloned_active_setting = active_setting.clone();
        let is_active_setting = match active_setting.lock().unwrap().as_ref() {
            Some(active) => Arc::ptr_eq(active, &setting),
            None => false,
        };

        if !is_active_setting && !is_enum {
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
                prop.set_default_f32(1, SETTING_LABEL_LINESPACE / 2.).unwrap();
                prop.set_default_f32(2, 0.).unwrap();
                prop.set_default_f32(3, 0.).unwrap();
                node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                node.set_property_bool(
                    atom,
                    Role::App,
                    "is_visible",
                    matches!(setting_clone.get_value(), PropertyValue::Bool(false)),
                )
                .unwrap();

                let shape = shape::create_circle([0.9, 0.4, 0.4, 0.7]).scaled(5.);
                node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
                let node = node
                    .setup(|me| {
                        VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())
                    })
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
                prop.set_default_f32(1, SETTING_LABEL_LINESPACE / 2.).unwrap();
                prop.set_default_f32(2, 0.).unwrap();
                prop.set_default_f32(3, 0.).unwrap();
                node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                node.set_property_bool(
                    atom,
                    Role::App,
                    "is_visible",
                    matches!(setting_clone.get_value(), PropertyValue::Bool(true)),
                )
                .unwrap();

                let shape = shape::create_circle([0., 0.94, 1., 1.]).scaled(5.);
                node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
                let node = node
                    .setup(|me| {
                        VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())
                    })
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
                prop.set_default_f32(1, 0.).unwrap();
                prop.clone()
                    .set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap())
                    .unwrap();
                prop.set_default_f32(3, 100.).unwrap();
                value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                prop.set_default_f32(1, SETTING_LABEL_Y).unwrap();
                value_node
                    .set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE)
                    .unwrap();
                value_node
                    .set_property_str(atom, Role::App, "text", setting_clone.value_as_string())
                    .unwrap();
                if matches!(setting_clone.get_value(), PropertyValue::Bool(false)) {
                    value_node
                        .get_property("text_color")
                        .unwrap()
                        .set_default_f32_multi(&[0.9, 0.4, 0.4, 1.])
                        .unwrap();
                } else {
                    value_node
                        .get_property("text_color")
                        .unwrap()
                        .set_default_f32_multi(&[0.0, 0.94, 1., 1.])
                        .unwrap();
                }
                value_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

                let node = value_node
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
                setting_layer_node.link(node);
            } else if setting_name != "win.scale" {
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
                prop.set_default_f32(1, 0.).unwrap();
                prop.clone()
                    .set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap())
                    .unwrap();
                prop.set_default_f32(3, 100.).unwrap();
                value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
                prop.set_default_f32(1, SETTING_LABEL_Y).unwrap();
                value_node
                    .set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE)
                    .unwrap();
                value_node
                    .set_property_str(atom, Role::App, "text", setting_clone.value_as_string())
                    .unwrap();
                value_node
                    .get_property("text_color")
                    .unwrap()
                    .set_default_f32_multi(&[0.92, 0.92, 0.92, 1.])
                    .unwrap();
                value_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

                let node = value_node
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
                setting_layer_node.link(node);
            }

            let node = create_button("selector_btn");
            node.set_property_bool(
                atom,
                Role::App,
                "is_active",
                setting_name != "win.scale" && !is_enum,
            )
            .unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.set_default_expr(0, cc.compile("w * X_RATIO").unwrap()).unwrap();
            prop.set_default_f32(1, 0.).unwrap();
            prop.clone()
                .set_expr(atom, Role::App, 2, cc.compile("w * (1-X_RATIO)").unwrap())
                .unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();

            let sg_root2 = app.sg_root.clone();
            let setting_clone2 = setting_clone.clone();
            let setting_root2 = setting_layer_node.clone();
            let select = move || {
                let atom = &mut PropertyAtomicGuard::none();
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
                    if let Some(node) = old_node.lookup_node("/value_label") {
                        let text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
                        text.set(atom, &s.value_as_string());
                    }

                    // Hide the selected setting editbox
                    // of the selected setting, if there's one
                    if !_was_active {
                        if let Some(node) = old_node.lookup_node("/value_editbox") {
                            node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
                            node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
                            node.set_property_str(atom, Role::App, "text", "").unwrap();
                        }

                        // Re-enable the selector button of the deselected setting
                        if let Some(node) = old_node.lookup_node("/selector_btn") {
                            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
                        }
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
                        setting_clone2.prop.set_bool(atom, Role::User, 0, true).unwrap();
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
                        let prop = node.get_property("text_color").unwrap();
                        prop.set_f32(atom, Role::App, 0, 0.75).unwrap();
                        prop.set_f32(atom, Role::App, 1, 0.75).unwrap();
                        prop.set_f32(atom, Role::App, 2, 0.75).unwrap();
                        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
                    } else {
                        setting_clone2.prop.set_bool(atom, Role::User, 0, false).unwrap();
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
                        let prop = node.get_property("text_color").unwrap();
                        prop.set_f32(atom, Role::App, 0, 0.9).unwrap();
                        prop.set_f32(atom, Role::App, 1, 0.4).unwrap();
                        prop.set_f32(atom, Role::App, 2, 0.4).unwrap();
                        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
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
                    if !was_active {
                        node.set_property_str(
                            atom,
                            Role::App,
                            "text",
                            setting_clone2.value_as_string(),
                        )
                        .unwrap();
                    }

                    // Disable the selector button so the editbox receives clicks
                    setting_root2
                        .lookup_node("/selector_btn")
                        .unwrap()
                        .set_property_bool(atom, Role::App, "is_active", false)
                        .unwrap();

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
                let row_edit_node2 = if is_bool { None } else { row_edit_node.clone() };
                let listen_click = app.ex.spawn(async move {
                    while let Ok(_) = recvr.recv().await {
                        select2();
                        if let Some(edit_node) = &row_edit_node2 {
                            edit_node.call_method("focus", vec![]).await.unwrap();
                        }
                    }
                });
                app.tasks.lock().push(listen_click);

                let node = node
                    .setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
                    .await;
                setting_layer_node.link(node.clone());
            }
        }

        if is_bool {
            // Switch icon
            let node = create_vector_art("switch_btn_bg");
            let prop = node.get_property("rect").unwrap();
            prop.set_default_expr(0, cc.compile("w - SWITCH_X_OFFSET").unwrap()).unwrap();
            prop.set_default_f32(1, SETTING_LABEL_LINESPACE / 2.).unwrap();
            prop.set_default_f32(2, 0.).unwrap();
            prop.set_default_f32(3, 0.).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

            let shape = shape::create_switch([0., 0.94, 1., 1.]).scaled(10.);
            node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
            let node = node
                .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
                .await;
            setting_layer_node.link(node);
        } else if setting_name != "win.scale" {
            let node = create_vector_art("confirm_btn_bg");
            let prop = node.get_property("rect").unwrap();
            prop.set_default_expr(0, cc.compile("w - CONFIRM_X_OFFSET").unwrap()).unwrap();
            prop.set_default_f32(1, SETTING_LABEL_LINESPACE / 2.).unwrap();
            prop.set_default_f32(2, 0.).unwrap();
            prop.set_default_f32(3, 0.).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
            node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();

            let shape = shape::create_confirm([0., 0.94, 1., 1.]).scaled(10.);
            node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
            let node = node
                .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
                .await;
            setting_layer_node.link(node.clone());

            let node = create_button("confirm_btn");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.set_default_expr(0, cc.compile("w - CONFIRM_BTN_W").unwrap()).unwrap();
            prop.set_default_f32(1, 0.).unwrap();
            prop.set_default_f32(2, CONFIRM_BTN_W).unwrap();
            prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
            node.set_property_u32(atom, Role::App, "priority", 2).unwrap();

            let node = node
                .setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
                .await;
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
                app.tasks.lock().push(listen_click);
            }
        }

        let node = create_vector_art("reset_btn_bg");
        let prop = node.get_property("rect").unwrap();
        prop.set_default_expr(
            0,
            cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE - RESET_GLYPH_X").unwrap(),
        )
        .unwrap();
        prop.set_default_f32(1, SETTING_LABEL_LINESPACE / 2.).unwrap();
        prop.set_default_f32(2, 0.).unwrap();
        prop.set_default_f32(3, 0.).unwrap();
        node.set_property_bool(atom, Role::App, "is_visible", !setting.is_default()).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

        let shape = shape::create_reset([0., 0.94, 1., 1.]).scaled(10.);
        node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
        let node = node
            .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
        setting_layer_node.link(node);

        let node = create_button("reset_btn");
        node.set_property_bool(atom, Role::App, "is_active", !setting.is_default()).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_default_expr(
            0,
            cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE - RESET_BTN_W").unwrap(),
        )
        .unwrap();
        prop.set_default_f32(1, 0.).unwrap();
        prop.set_default_f32(2, RESET_BTN_W).unwrap();
        prop.set_default_f32(3, SETTING_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

        let node = node
            .setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
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

                    let atom = &mut PropertyAtomicGuard::none();

                    if setting2.is_bool() {
                        refresh_bool_row(&setting2, &sg_root2, atom);
                    } else if setting2.name == "win.scale" {
                        reset_win_scale_row(&setting2, &sg_root2, atom);
                    }

                    // Show the selected setting value label (set its text empty)
                    // of the selected setting, if there's one
                    if let Some(node) = sg_root2.lookup_node("/value_label") {
                        let text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
                        text.set(atom, setting2.value_as_string());
                    }

                    if let Some(node) = sg_root2.lookup_node("/value_dropdown") {
                        sync_dropdown(&node, &setting2, atom);
                    }

                    if let Some(node) = sg_root2.lookup_node("/value_editbox") {
                        node.set_property_str(atom, Role::App, "text", setting2.value_as_string())
                            .unwrap();
                    }

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
            app.tasks.lock().push(listen_click);
        }
    }

    edit_switch(&mut app.tasks.lock(), &edit_nodes, app.ex.clone());

    // Sync with current settings
    {
        let sg_root2 = app.sg_root.clone();
        let redraw2 = app.redraw_trigger.clone();
        let settings_map2 = settings_map.clone();
        let is_visible = layer_node.get_property("is_visible").unwrap();
        let is_visible_sub = is_visible.subscribe_modify();
        let listen_visible = app.ex.spawn(async move {
            while is_visible_sub.receive().await.is_ok() {
                if !is_visible.get_bool(0).unwrap() {
                    continue
                }
                let atom = &mut redraw2.make_guard(gfxtag!("settings resync"));
                for (name, setting) in settings_map2.iter() {
                    let path = format!("/window/content/settings_layer/settings/{name}");
                    let Some(row) = sg_root2.lookup_node(&path) else { continue };
                    if setting.is_bool() {
                        refresh_bool_row(setting, &row, atom);
                    } else if let Some(dropdown) = row.lookup_node("/value_dropdown") {
                        sync_dropdown(&dropdown, setting, atom);
                    } else if let Some(label) = row.lookup_node("/value_label") {
                        label
                            .set_property_str(atom, Role::App, "text", setting.value_as_string())
                            .unwrap();
                    }
                    refresh_setting(setting.clone(), row);
                }
            }
        });
        app.tasks.lock().push(listen_visible);
    }

    let settings_node = app.sg_root.lookup_node("/window/content/settings_layer").unwrap();
    settings_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();

    // Searchbar results count
    let node = app.sg_root.lookup_node("/window/content/settings_layer/settings").unwrap();
    let counter_text = node.get_children().len().to_string();
    let node = app.sg_root.lookup_node("/window/content/settings_layer/search_count").unwrap();
    node.set_property_str(atom, Role::App, "text", &counter_text).unwrap();
}

fn spawn_win_scale_listener(
    app: &App,
    setting: Arc<Setting>,
    row_root: &SceneNodePtr,
    slider: &SceneNode,
) {
    let (slot, recvr) = Slot::new("slider_changed");
    slider.register("changed", slot).unwrap();
    let row_root = row_root.clone();
    let redraw = app.redraw_trigger.clone();
    let task = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let Ok(val) = deserialize::<f32>(&data) else { continue };
            let atom = &mut redraw.make_guard(gfxtag!("settings slider changed"));
            if let Err(e) = setting.prop.set_f32(atom, Role::User, 0, val) {
                error!(target: "app::settings", "failed to set win.scale: {e}");
                continue
            }
            info!(target: "app::settings", "Applied win.scale live: {val}");
            refresh_setting(setting.clone(), row_root.clone());
        }
    });
    app.tasks.lock().push(task);
}

fn sync_dropdown(dropdown: &SceneNodePtr, setting: &Setting, atom: &mut PropertyAtomicGuard) {
    let items = setting.prop.enum_items.clone().unwrap_or_default();
    let current = match setting.get_value() {
        PropertyValue::Enum(cur) => cur,
        _ => return,
    };
    let selected = items.iter().position(|item| *item == current).unwrap_or(0) as u32;

    let prop = dropdown.get_property("items").unwrap();
    prop.set_str_vec(atom, Role::App, items).unwrap();
    dropdown.set_property_u32(atom, Role::App, "selected", selected).unwrap();
}

fn decode_selection_payload(data: &[u8]) -> Option<(u32, String)> {
    let mut cur = std::io::Cursor::new(data);
    let idx = u32::decode(&mut cur).ok()?;
    let item = String::decode(&mut cur).ok()?;
    Some((idx, item))
}

fn reset_win_scale_row(setting: &Setting, row_root: &SceneNodePtr, atom: &mut PropertyAtomicGuard) {
    let def = match setting.get_default() {
        PropertyValue::Float32(v) => v.clamp(0.8, 1.2),
        _ => 1.,
    };
    if let Some(slider) = row_root.lookup_node("/value_slider") {
        slider.set_property_f32(atom, Role::App, "value", def).unwrap();
    }
}
fn refresh_bool_row(setting: &Setting, sn: &SceneNodePtr, atom: &mut PropertyAtomicGuard) {
    let on = matches!(setting.get_value(), PropertyValue::Bool(true));
    sn.lookup_node("/value_bg_bool_true")
        .unwrap()
        .set_property_bool(atom, Role::App, "is_visible", on)
        .unwrap();
    sn.lookup_node("/value_bg_bool_false")
        .unwrap()
        .set_property_bool(atom, Role::App, "is_visible", !on)
        .unwrap();
    sn.lookup_node("/bool_icon_bg_true")
        .unwrap()
        .set_property_bool(atom, Role::App, "is_visible", on)
        .unwrap();
    sn.lookup_node("/bool_icon_bg_false")
        .unwrap()
        .set_property_bool(atom, Role::App, "is_visible", !on)
        .unwrap();
    let label = sn.lookup_node("/value_label").unwrap();
    label.set_property_str(atom, Role::App, "text", setting.value_as_string()).unwrap();
    let prop = label.get_property("text_color").unwrap();
    let color = if on { [0., 0.94, 1., 1.] } else { [0.9, 0.4, 0.4, 1.] };
    for (i, c) in color.iter().enumerate() {
        prop.set_f32(atom, Role::App, i, *c).unwrap();
    }
}

fn refresh_setting(setting: Arc<Setting>, sn: SceneNodePtr) {
    let atom = &mut PropertyAtomicGuard::none();

    let node = sn.lookup_node("/key_label").unwrap();
    if setting.clone().is_default() {
        let prop = node.get_property("text_color").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.65).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.87).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.83).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else {
        node.get_property("text_color")
            .unwrap()
            .set_default_f32_multi(&[0.92, 0.92, 0.92, 1.])
            .unwrap();
    }

    let node = sn.lookup_node("/reset_btn_bg").unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", !setting.clone().is_default()).unwrap();
    let node = sn.lookup_node("/reset_btn").unwrap();
    node.set_property_bool(atom, Role::App, "is_active", !setting.clone().is_default()).unwrap();
}

async fn update_setting(
    setting: Arc<Setting>,
    sn: SceneNodePtr,
    active_setting: Arc<Mutex<Option<Arc<Setting>>>>,
    editz_text: Option<PropertyStr>,
) {
    let atom = &mut PropertyAtomicGuard::none();

    // Snapshot the edited text before the editbox is cleared below, since
    // editz_text wraps the very property that gets reset to empty
    let edited_text = editz_text.map(|prop| prop.get());

    if let Some(node) = sn.lookup_node("/value_editbox") {
        node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
        node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
        node.set_property_str(atom, Role::App, "text", "").unwrap();
        node.call_method("unfocus", vec![]).await.unwrap();
    }

    // Re-enable the selector button so the row can be selected again
    // The win.scale row is excluded: it uses a slider and its selector must
    // stay disabled, since select() would unwrap a "/value_label" node that
    // this row does not have
    if setting.name != "win.scale" {
        // Enum rows have no selector button, and bool rows keep theirs
        // always active, so the lookup is skipped or a no-op for them
        if let Some(node) = sn.lookup_node("/selector_btn") {
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        }
    }

    let Some(value_str) = edited_text else { return };

    match &setting.get_value() {
        PropertyValue::Uint32(_) => {
            let parsed = value_str.parse::<u32>();
            if let Ok(value) = parsed {
                if let Some(node) = sn.lookup_node("/value_label") {
                    node.set_property_str(atom, Role::App, "text", &value_str).unwrap();
                }
                if let Some(node) = sn.lookup_node("/confirm_btn_bg") {
                    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                setting.prop.set_u32(atom, Role::User, 0, value).unwrap();
                let mut active_setting_value = active_setting.lock().unwrap();
                *active_setting_value = None;
            }
        }
        PropertyValue::Float32(_) => {
            let parsed = value_str.parse::<f32>();
            if let Ok(value) = parsed {
                if let Some(node) = sn.lookup_node("/value_label") {
                    node.set_property_str(atom, Role::App, "text", &value_str).unwrap();
                }
                if let Some(node) = sn.lookup_node("/confirm_btn_bg") {
                    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                setting.prop.set_f32(atom, Role::User, 0, value).unwrap();
                let mut active_setting_value = active_setting.lock().unwrap();
                *active_setting_value = None;
            }
        }
        PropertyValue::Str(_) => {
            if let Some(node) = sn.lookup_node("/value_label") {
                node.set_property_str(atom, Role::App, "text", &value_str).unwrap();
            }
            if let Some(node) = sn.lookup_node("/confirm_btn_bg") {
                node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
            }
            setting.prop.set_str(atom, Role::User, 0, &value_str).unwrap();
            let mut active_setting_value = active_setting.lock().unwrap();
            *active_setting_value = None;
        }
        _ => {}
    };
}
