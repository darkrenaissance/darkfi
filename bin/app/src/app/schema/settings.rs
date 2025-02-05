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

use sled_overlay::sled;


use crate::{
    app::{
        node::{
            create_button, create_editbox,
            create_layer, create_text, create_vector_art,
        },
        App,
    },
    expr::{self, Compiler},
    prop::{
        PropertyAtomicGuard, PropertyFloat32, PropertyStr, Property, PropertyValue, Role,
    },
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{
        Button, EditBox, Layer, ShapeVertex, Text, VectorArt,
        VectorShape,
    },
};

use super::{ColorScheme, CHANNELS, COLOR_SCHEME};

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

mod android_ui_consts {
    pub const SETTING_LABEL_X: f32 = 40.;
    pub const SETTING_LABEL_LINESPACE: f32 = 140.;
    pub const SETTING_LABEL_BASELINE: f32 = 82.;
    pub const SUBTITLE_LABEL_FONTSIZE: f32 = 36.;
    pub const SETTING_LABEL_FONTSIZE: f32 = 24.;
    pub const SETTING_EDIT_FONTSIZE: f32 = 24.;
    pub const SETTING_TITLE_X: f32 = 150.;
    pub const SETTING_TITLE_FONTSIZE: f32 = 40.;
    pub const SETTING_TITLE_BASELINE: f32 = 82.;
    pub const BORDER_RIGHT_SCALE: f32 = 15.;

    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 50.;
    pub const BACKARROW_Y: f32 = 70.;
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
    pub const SUBTITLE_LABEL_FONTSIZE: f32 = 16.;
    pub const SETTING_LABEL_FONTSIZE: f32 = 14.;
    pub const SETTING_EDIT_FONTSIZE: f32 = 14.;
    pub const SETTING_TITLE_X: f32 = 100.;
    pub const SETTING_TITLE_FONTSIZE: f32 = 20.;
    pub const SETTING_TITLE_BASELINE: f32 = 37.;
    pub const BORDER_RIGHT_SCALE: f32 = 30.;

    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 26.;
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
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            PropertyValue::Float32(fl) => fl.to_string(),
            _ => "unknown".to_string(),
        }
    }
    fn get_value(&self) -> PropertyValue {
        self.node.get_property("value").unwrap().get_value(0).ok().unwrap()
    }
}

pub async fn make(app: &App, window: SceneNodePtr) {
    let mut cc = Compiler::new();
    cc.add_const_f32("BORDER_RIGHT_SCALE", BORDER_RIGHT_SCALE);
    cc.add_const_f32("X_RATIO", 1./3.);
    let window_scale = PropertyFloat32::wrap(&window, Role::Internal, "scale", 0).unwrap();
    let atom = &mut PropertyAtomicGuard::new();

    // Main view
    let layer_node = create_layer("settings_layer");
    let prop = layer_node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.clone().set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.clone().set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    layer_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let layer_node =
        layer_node.setup(|me| Layer::new(me, app.render_api.clone(), app.ex.clone())).await;
    window.clone().link(layer_node.clone());

    let mut setting_y = 0.;

    // Create the back button
    let node = create_vector_art("back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 3, 0.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.clone().link(node);


    let node = create_button("back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 2, 100.).unwrap();
    prop.clone().set_f32(atom, Role::App, 3, 100.).unwrap();

    let sg_root = app.sg_root.clone();
    let goback = move || {
        let atom = &mut PropertyAtomicGuard::new();

        // Disable visilibity of all relevant window nodes
        // This is needed since for example all chats have a different node name.
        let windows = sg_root.clone().lookup_node("/window").unwrap().get_children();
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
        let menu_node = sg_root.clone().lookup_node("/window/dev_chat_layer").unwrap();
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
    layer_node.clone().link(node.clone());
    
    // Label: "SETTINGS" title
    let node = create_text("settings_label_fontsize");
    let prop = node.get_property("rect").unwrap();
    prop.clone().set_f32(atom, Role::App, 0, SETTING_TITLE_X).unwrap();
    prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.clone().set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.clone().set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", SETTING_TITLE_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", SETTING_TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "SETTINGS").unwrap();
    //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.clone().set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.clone().set_f32(atom, Role::App, 2, 1.).unwrap();
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

    setting_y += 20.;

    // Create a BTreeMap to store settings
    let mut settings_map: BTreeMap<String, Arc<Setting>> = BTreeMap::new();

    // Get the settings from all plugins
    let sg_root_children = app.sg_root.clone().get_children();
    let plugin_node = sg_root_children.iter().find(|node| node.name == "plugin");
    if let Some(pnode) = plugin_node {
        for plugin in pnode.get_children().iter() {
            let plugin_children = plugin.get_children();
            let setting_root = plugin_children.iter().find(|node| node.name == "setting");
            if let Some(sroot) = setting_root {
                for setting in sroot.get_children().iter() {
                    let name = vec![plugin.name.clone(), setting.name.clone()].join(".");
                    settings_map.insert(
                        name.clone(),
                        Arc::new(Setting {
                            name,
                            node: setting.clone(),
                        }),
                    );
                }
            }
        }
    }

    // Setting currently being edited
    let active_setting: Arc<Mutex<Option<Arc<Setting>>>> = Arc::new(Mutex::new(None));

    // Iterate over the map and process each setting
    for (key, setting) in settings_map {
        let setting_clone = setting.clone();
        let setting_name = setting_clone.name.clone();

        setting_y += SETTING_LABEL_LINESPACE;

        // Background Label
        let node = create_vector_art("settings_label_bg");
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, setting_y).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, cc.compile("w  * X_RATIO - BORDER_RIGHT_SCALE").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

        let mut shape = VectorShape::new();

        let x1 = expr::const_f32(0.);
        let y1 = expr::const_f32(0.);
        let x2 = expr::load_var("w");
        let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);
        let (color1, color2) = match COLOR_SCHEME {
            ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0.11, 0.11, 1.]),
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

        let node =
            node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
        layer_node.clone().link(node);

        // Border right arrow
        let node = create_vector_art("right_arrow");
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE/2").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, setting_y+30.).unwrap();
        prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, 0.).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
        let shape = shape::create_right_border([0., 0.11, 0.11, 1.]).scaled(30.);
        let node =
            node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
        layer_node.clone().link(node);

        // Background Value
        let node = create_vector_art(&(setting_name.to_string() + "_settings_value_bg"));
        let prop = node.get_property("rect").unwrap();
        prop.clone().set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE/2").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, setting_y).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, cc.compile("w * (1-X_RATIO) + BORDER_RIGHT_SCALE/2").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();

        let mut shape = VectorShape::new();

        let x1 = expr::const_f32(0.);
        let y1 = expr::const_f32(0.);
        let x2 = expr::load_var("w");
        let y2 = expr::const_f32(SETTING_LABEL_LINESPACE);
        let (color1, color2) = match COLOR_SCHEME {
            ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0., 0.11, 0.11, 1.]),
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

        let node =
            node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
        layer_node.clone().link(node);

        // Label: "SETTINGS" title
        let label_value_node = create_text(&(setting_name.to_string() + "_settings_label"));
        let prop = label_value_node.get_property("rect").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, SETTING_LABEL_X).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, setting_y).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, 100.).unwrap();
        label_value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        label_value_node.set_property_f32(atom, Role::App, "baseline", SETTING_LABEL_BASELINE).unwrap();
        label_value_node.set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE).unwrap();
        label_value_node.set_property_str(atom, Role::App, "text", setting_name.clone()).unwrap();
        let prop = label_value_node.get_property("text_color").unwrap();
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
        layer_node.clone().link(label_value_node);

        // Text edit
        let editbox_node = create_editbox(&(setting_name.to_string() + "_editz"));
        editbox_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        editbox_node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
        let prop = editbox_node.get_property("rect").unwrap();
        prop.clone().set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, setting_y + 15. ).unwrap();
        prop.clone().set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO - BORDER_RIGHT_SCALE").unwrap()).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "baseline", 16.).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "font_size", SETTING_EDIT_FONTSIZE).unwrap();
        let prop = editbox_node.get_property("text_color").unwrap();
        if COLOR_SCHEME == ColorScheme::DarkMode {
            prop.clone().set_f32(atom, Role::App, 0, 0.7).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, 0.7).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 0.7).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
        } else if COLOR_SCHEME == ColorScheme::PaperLight {
            prop.clone().set_f32(atom, Role::App, 0, 1.).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, 1.).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 1.).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
        }
        let prop = editbox_node.get_property("cursor_color").unwrap();
        prop.clone().set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.clone().set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.clone().set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "cursor_ascent", SETTING_EDIT_FONTSIZE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "cursor_descent", SETTING_EDIT_FONTSIZE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "select_ascent", SETTING_EDIT_FONTSIZE).unwrap();
        editbox_node.set_property_f32(atom, Role::App, "select_descent", SETTING_EDIT_FONTSIZE).unwrap();
        let prop = editbox_node.get_property("hi_bg_color").unwrap();
        if COLOR_SCHEME == ColorScheme::DarkMode {
            prop.clone().set_f32(atom, Role::App, 0, 0.5).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, 0.5).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 0.5).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
        } else if COLOR_SCHEME == ColorScheme::PaperLight {
            prop.clone().set_f32(atom, Role::App, 0, 1.).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, 1.).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 1.).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 0.5).unwrap();
        }
        let prop = editbox_node.get_property("selected").unwrap();
        prop.clone().set_null(atom, Role::App, 0).unwrap();
        prop.clone().set_null(atom, Role::App, 1).unwrap();
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
            let sg_root2 = app.sg_root.clone();
            let active_setting2 = active_setting.clone();
            let editz_text2 = editz_text.clone();
            let listen_enter = app.ex.spawn(async move {
                while let Ok(_) = recvr.recv().await {
                    update_setting(setting2.clone(), sg_root2.clone(), active_setting2.clone(), editz_text2.clone()).await;
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
        layer_node.clone().link(node);

        // Is this setting the one that is currently active
        let cloned_active_setting = active_setting.clone();
        let is_active_setting = match active_setting.lock().unwrap().as_ref() {
            Some(active) => Arc::ptr_eq(active, &setting),
            None => false,
        };
        let is_bool = matches!(setting_clone.get_value(), PropertyValue::Bool(_));

        if !is_active_setting {
            // Label showing the setting's current value
            let value_node = create_text(&(setting_name.to_string() + "_settings_value_label"));
            let prop = value_node.get_property("rect").unwrap();
            prop.clone().set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO + BORDER_RIGHT_SCALE").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, setting_y).unwrap();
            prop.clone().set_expr(atom, Role::App, 2, cc.compile("w * X_RATIO").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 100.).unwrap();
            value_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
            value_node.set_property_f32(atom, Role::App, "baseline", SETTING_LABEL_BASELINE).unwrap();
            value_node.set_property_f32(atom, Role::App, "font_size", SETTING_LABEL_FONTSIZE).unwrap();
            value_node.set_property_str(atom, Role::App, "text", setting_clone.value_as_string()).unwrap();
            let prop = value_node.get_property("text_color").unwrap();
            if COLOR_SCHEME == ColorScheme::DarkMode {
                prop.clone().set_f32(atom, Role::App, 0, 1.).unwrap();
                prop.clone().set_f32(atom, Role::App, 1, 1.).unwrap();
                prop.clone().set_f32(atom, Role::App, 2, 1.).unwrap();
                prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
            } else if COLOR_SCHEME == ColorScheme::PaperLight {
                prop.clone().set_f32(atom, Role::App, 0, 0.).unwrap();
                prop.clone().set_f32(atom, Role::App, 1, 0.).unwrap();
                prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
                prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
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
            layer_node.clone().link(node);

            // A wide button useful to select the current setting
            let node = create_button("setting_selector_btn");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.clone().set_expr(atom, Role::App, 0, cc.compile("w * X_RATIO").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, setting_y).unwrap();
            prop.clone().set_expr(atom, Role::App, 2, cc.compile("w * (1-X_RATIO)").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();

            let sg_root = app.sg_root.clone();
            let select = move || {
                let atom = &mut PropertyAtomicGuard::new();
                let sg_root = sg_root.clone();
                let mut lock = cloned_active_setting.lock().unwrap();

                let was_active = if let Some(s) = lock.as_ref() {
                    let _was_active = s.name == setting_clone.clone().name;

                    // Hide the selected setting value label (set its text empty)
                    // of the selected setting, if there's one
                    let path = format!("/window/settings_layer/{}_settings_value_label", &s.name);
                    let node = sg_root.clone().lookup_node(&path).unwrap();
                    let text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
                    text.set(atom, &s.value_as_string());

                    // Hide the selected setting value label (set its text empty)
                    // of the selected setting, if there's one
                    if !_was_active {
                        let path = format!("/window/settings_layer/{}_editz", &s.name);
                        let node = sg_root.clone().lookup_node(&path).unwrap();
                        node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
                        node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
                        node.set_property_f32(atom, Role::App, "font_size", 0.).unwrap();
                        node.set_property_str(atom, Role::App, "text", "").unwrap();
                    }
                    
                    // Hide confirm button
                    // (Bool settings don't have a confirm button so we have to check for it to
                    // not panic)
                    let is_bool = matches!(lock.clone().unwrap().get_value(), PropertyValue::Bool(_));
                    if !is_bool {
                        let path = format!("/window/settings_layer/{}_confirm_btn_bg", &s.name);
                        let node = sg_root.clone().lookup_node(&path).unwrap();
                        node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                    }

                    _was_active
                } else {
                    false
                };

                // Update what setting active_setting points to 
                *lock = Some(setting_clone.clone());
                debug!("active setting set to: {}", lock.clone().unwrap().name);

                let is_bool = matches!(lock.clone().unwrap().get_value(), PropertyValue::Bool(_));
                if is_bool {
                    // Hide the setting value label (set its text empty) 
                    let label_path = format!("/window/settings_layer/{}_settings_value_label", &setting_clone.name);
                    let editbox = sg_root.clone().lookup_node(&label_path).unwrap();
                    let label_text = PropertyStr::wrap(&editbox, Role::App, "text", 0).unwrap();
                    let value = &lock.clone().unwrap().get_value();
                    if matches!(value, PropertyValue::Bool(false)) {
                        &lock.clone().unwrap().node.set_property_bool(atom, Role::User, "value", true);
                        label_text.set(atom, "true");
                    } else {
                        &lock.clone().unwrap().node.set_property_bool(atom, Role::User, "value", false);
                        label_text.set(atom, "false");
                    }
                } else {
                    // Hide the setting value label (set its text empty) 
                    // TODO?: Visilibity property on labels
                    let label_path = format!("/window/settings_layer/{}_settings_value_label", &setting_clone.name);
                    let editbox = sg_root.clone().lookup_node(&label_path).unwrap();
                    let label_text = PropertyStr::wrap(&editbox, Role::App, "text", 0).unwrap();
                    label_text.set(atom, "");

                    // Show the editbox
                    let path = format!("/window/settings_layer/{}_editz", &setting_clone.name);
                    let node = sg_root.clone().lookup_node(&path).unwrap();
                    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
                    node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();
                    node.set_property_f32(atom, Role::App, "font_size", 16.).unwrap();
                    if !was_active {
                        node.set_property_str(atom, Role::App, "text", setting_clone.value_as_string()).unwrap();
                    }

                    // Show confirm button
                    let path = format!("/window/settings_layer/{}_confirm_btn_bg", &setting_clone.name);
                    let node = sg_root.clone().lookup_node(&path).unwrap();
                    let _ = node.set_property_bool(atom, Role::App, "is_visible", true);

                    // Hide the value background
                    let path = format!("/window/settings_layer/{}_settings_value_bg", &setting_clone.name);
                    let node = sg_root.clone().lookup_node(&path).unwrap();
                    let _ = node.set_property_bool(atom, Role::App, "is_visible", false);
                }
            };

            let (slot, recvr) = Slot::new("back_clicked");
            node.register("click", slot).unwrap();
            let select2 = select.clone();
            let listen_click = app.ex.spawn(async move {
                while let Ok(_) = recvr.recv().await {
                    select2();
                }
            });
            app.tasks.lock().unwrap().push(listen_click);

            let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
            layer_node.clone().link(node.clone());
        }

        if is_bool {
            // Switch icon
            let node = create_vector_art(&(setting_name.to_string() + "_switch_btn_bg"));
            let prop = node.get_property("rect").unwrap();
            prop.clone().set_expr(atom, Role::App, 0, cc.compile("w - 50").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, setting_y + SETTING_LABEL_LINESPACE / 2.).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 0.).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

            let shape = shape::create_switch([0., 0.94, 1., 1.]).scaled(10.);
            let node =
                node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
            layer_node.clone().link(node);
        } else {
            // Confirm button
            let node = create_vector_art(&(setting_name.to_string() + "_confirm_btn_bg"));
            let prop = node.get_property("rect").unwrap();
            prop.clone().set_expr(atom, Role::App, 0, cc.compile("w - 50").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, setting_y + SETTING_LABEL_LINESPACE / 2.).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 0.).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, 0.).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
            node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();

            let shape = shape::create_confirm([0., 0.94, 1., 1.]).scaled(10.);
            let node =
                node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
            layer_node.clone().link(node.clone());

            let node = create_button("confirm_btn");
            node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
            let prop = node.get_property("rect").unwrap();
            prop.clone().set_expr(atom, Role::App, 0, cc.compile("w - 100").unwrap()).unwrap();
            prop.clone().set_f32(atom, Role::App, 1, setting_y).unwrap();
            prop.clone().set_f32(atom, Role::App, 2, 100.).unwrap();
            prop.clone().set_f32(atom, Role::App, 3, SETTING_LABEL_LINESPACE).unwrap();
            node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

            let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
            layer_node.clone().link(node.clone());

            // Handle confirm button click
            {
                let (slot, recvr) = Slot::new("confirm_clicked");
                node.register("click", slot).unwrap();
                let setting2 = setting.clone();
                let sg_root2 = app.sg_root.clone();
                let active_setting2 = active_setting.clone();
                let editz_text2 = editz_text.clone();
                let listen_click = app.ex.spawn(async move {
                    while let Ok(_) = recvr.recv().await {
                        info!("confirm clicked");
                        update_setting(setting2.clone(), sg_root2.clone(), active_setting2.clone(), editz_text2.clone()).await;
                    }
                });
                app.tasks.lock().unwrap().push(listen_click);
            }
        }
    }

    let settings_node = app.sg_root.clone().lookup_node("/window/settings_layer").unwrap();
    settings_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
}

async fn update_setting(setting: Arc<Setting>, sg_root: SceneNodePtr, active_setting: Arc<Mutex<Option<Arc<Setting>>>>, editz_text: PropertyStr) {
    let atom = &mut PropertyAtomicGuard::new();
    let setting_name = &setting.clone().name;

    let path = format!("/window/settings_layer/{}_editz", setting_name);
    if let Some(node) = sg_root.clone().lookup_node(path) {
        node.set_property_f32(atom, Role::App, "font_size", 0.).unwrap();
        node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
        node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
    }

    match &setting.get_value() {
        PropertyValue::Uint32(_) => {
            let value_str = editz_text.get();
            let parsed = value_str.parse::<u32>();
            if let Ok(value) = parsed {
                let path = format!("/window/settings_layer/{}_settings_value_label", setting_name);
                if let Some(node) = sg_root.clone().lookup_node(path) {
                    node.set_property_str(atom, Role::App, "text", value_str).unwrap();
                }
                let path = format!("/window/settings_layer/{}_confirm_btn_bg", setting_name);
                if let Some(node) = sg_root.clone().lookup_node(path) {
                    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                setting.node.set_property_u32(atom, Role::User, "value", value);
                let mut active_setting_value = active_setting.lock().unwrap();
                *active_setting_value = None;
            }
        },
        PropertyValue::Float32(_) => {
            let value_str = editz_text.get();
            let parsed = value_str.parse::<f32>();
            if let Ok(value) = parsed {
                let path = format!("/window/settings_layer/{}_settings_value_label", setting_name);
                if let Some(node) = sg_root.clone().lookup_node(path) {
                    node.set_property_str(atom, Role::App, "text", value_str).unwrap();
                }
                let path = format!("/window/settings_layer/{}_confirm_btn_bg", setting_name);
                if let Some(node) = sg_root.clone().lookup_node(path) {
                    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                setting.node.set_property_f32(atom, Role::User, "value", value);
                let mut active_setting_value = active_setting.lock().unwrap();
                *active_setting_value = None;
            }
        },
        PropertyValue::Str(_) => {
            let value_str = editz_text.get();
            let path = format!("/window/settings_layer/{}_settings_value_label", setting_name);
            if let Some(node) = sg_root.clone().lookup_node(path) {
                node.set_property_str(atom, Role::App, "text", &value_str).unwrap();
            }
            let path = format!("/window/settings_layer/{}_confirm_btn_bg", setting_name);
            if let Some(node) = sg_root.clone().lookup_node(path) {
                node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
            }
            setting.node.set_property_str(atom, Role::User, "value", value_str);
            let mut active_setting_value = active_setting.lock().unwrap();
            *active_setting_value = None;
        },
        _ => {},
    };
}