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

use self::ui_consts::*;

use crate::{
    app::{
        node::{create_button, create_layer, create_text, create_vector_art},
        App,
    },
    expr,
    gfx::gfxtag,
    mesh::{CANCEL_MENU_BTN_GRADIENT, COLOR_CYAN, COLOR_RED, DONE_MENU_BTN_GRADIENT},
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    ui::{Button, Layer, Text, VectorArt, VectorShape},
    util::i18n::I18nBabelFish,
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]

mod android_ui_consts {
    // Button constants
    pub const LABEL_X: f32 = 40.;
    pub const MENU_BTN_W_L: f32 = 250.;
    pub const MENU_BTN_W_R: f32 = 200.;
    pub const MENU_BTN_H: f32 = 130.;
    pub const EDIT_BTN_OUTLINE_T: f32 = 2.;
    pub const BTN_TEXT_FONTSIZE: f32 = 50.;
    pub const BTN_TEXT_Y: f32 = 30.;
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
    // Button constants
    pub const LABEL_X: f32 = 20.;
    pub const MENU_BTN_W_L: f32 = 110.;
    pub const MENU_BTN_W_R: f32 = 85.;
    pub const MENU_BTN_H: f32 = 60.;
    pub const EDIT_BTN_OUTLINE_T: f32 = 1.;
    pub const BTN_TEXT_FONTSIZE: f32 = 20.;
    pub const BTN_TEXT_Y: f32 = 14.;
}

pub struct EditButtons {
    pub layer: SceneNodePtr,
    pub editlayer_is_visible: PropertyBool,
    pub cancel_btn: SceneNodePtr,
    pub done_btn: SceneNodePtr,
}

pub async fn create_edit_buttons(
    app: &App,
    parent: SceneNodePtr,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
) -> EditButtons {
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("LABEL_X", LABEL_X);
    cc.add_const_f32("MENU_BTN_W_L", MENU_BTN_W_L);
    cc.add_const_f32("MENU_BTN_W_R", MENU_BTN_W_R);
    cc.add_const_f32("MENU_BTN_H", MENU_BTN_H);

    let atom = &mut PropertyAtomicGuard::none();

    // Make buttons for cancel and done
    let node = create_layer("editbtn_layer");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, LABEL_X).unwrap();
    #[cfg(any(target_os = "android", feature = "emulate-android"))]
    let code = cc.compile("h - MENU_BTN_H - LABEL_X ").unwrap();
    #[cfg(not(any(target_os = "android", feature = "emulate-android")))]
    let code = cc.compile("h - MENU_BTN_H - LABEL_X - 10").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - 2 * LABEL_X").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, MENU_BTN_H).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let editlayer_node =
        node.setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    parent.link(editlayer_node.clone());

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
        expr::const_f32(MENU_BTN_W_L),
        expr::load_var("h"),
        CANCEL_MENU_BTN_GRADIENT,
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(MENU_BTN_W_L),
        expr::load_var("h"),
        EDIT_BTN_OUTLINE_T,
        COLOR_RED,
    );
    shape.add_gradient_box(
        cc.compile("w - MENU_BTN_W_R").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        DONE_MENU_BTN_GRADIENT,
    );
    shape.add_outline(
        cc.compile("w - MENU_BTN_W_R").unwrap(),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        EDIT_BTN_OUTLINE_T,
        COLOR_CYAN,
    );

    let node = node
        .setup(|me| VectorArt::new(me, shape, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    editlayer_node.link(node);

    // Create the cancel button
    let node = create_button("cancel_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_L).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();

    let cancel_btn =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    editlayer_node.link(cancel_btn.clone());

    // Text for cancel button
    let node = create_text("cancel_text");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, BTN_TEXT_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_L).unwrap();
    prop.set_f32(atom, Role::App, 3, MENU_BTN_H).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BTN_TEXT_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "cancel").unwrap();
    let prop = node.get_property("text_align").unwrap();
    prop.set_enum(atom, Role::App, 0, "center").unwrap();
    node.set_property_bool(atom, Role::App, "use_i18n", false).unwrap();

    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_RED[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_RED[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_RED[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_RED[3]).unwrap();

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
    editlayer_node.link(node);

    // Create the done button
    let node = create_button("done_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - MENU_BTN_W_R").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_R).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();

    let done_btn =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    editlayer_node.link(done_btn.clone());

    // Text for done button
    let node = create_text("done_text");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - MENU_BTN_W_R").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, BTN_TEXT_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, MENU_BTN_W_R).unwrap();
    prop.set_f32(atom, Role::App, 3, MENU_BTN_H).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BTN_TEXT_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "done").unwrap();
    let prop = node.get_property("text_align").unwrap();
    prop.set_enum(atom, Role::App, 0, "center").unwrap();
    node.set_property_bool(atom, Role::App, "use_i18n", false).unwrap();

    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();

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
    editlayer_node.link(node);

    EditButtons { layer: editlayer_node, editlayer_is_visible, cancel_btn, done_btn }
}

impl EditButtons {
    pub fn connect_edit_handlers(
        &self,
        app: &App,
        menu_node: &SceneNodePtr,
        sibling: Option<PropertyBool>,
    ) {
        // Subscribe to edit_active signal
        let (slot, recvr) = Slot::new("edit_activated");
        menu_node.register("edit_active", slot).unwrap();
        let redraw = app.redraw_trigger.clone();
        let editlayer = self.editlayer_is_visible.clone();
        let sibling_on = sibling.clone();
        let task = app.ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                debug!(target: "app::menu", "menu edit active");
                let atom = &mut redraw.make_guard(gfxtag!("edit_active"));
                if let Some(s) = &sibling_on {
                    s.set(atom, false);
                }
                editlayer.set(atom, true);
            }
        });
        app.tasks.lock().unwrap().push(task);

        // Cancel and done button click handlers
        self.connect_btn_handler(
            app,
            &self.cancel_btn,
            menu_node,
            "cancel_clicked",
            "cancel_edit",
            sibling.clone(),
        );
        self.connect_btn_handler(
            app,
            &self.done_btn,
            menu_node,
            "done_clicked",
            "done_edit",
            sibling.clone(),
        );
    }

    // Shared click handler for the cancel and done buttons.
    fn connect_btn_handler(
        &self,
        app: &App,
        btn: &SceneNodePtr,
        menu_node: &SceneNodePtr,
        slot_name: &'static str,
        method: &'static str,
        sibling: Option<PropertyBool>,
    ) {
        let (slot, recvr) = Slot::new(slot_name);
        btn.register("click", slot).unwrap();
        let menu_node = menu_node.clone();
        let redraw = app.redraw_trigger.clone();
        let editlayer = self.editlayer_is_visible.clone();
        let task = app.ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                menu_node.call_method(method, vec![]).await.unwrap();
                let atom = &mut redraw.make_guard(gfxtag!(slot_name));
                editlayer.set(atom, false);
                if let Some(s) = &sibling {
                    s.set(atom, true);
                }
            }
        });
        app.tasks.lock().unwrap().push(task);
    }
}
