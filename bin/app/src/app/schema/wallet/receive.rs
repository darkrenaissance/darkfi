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

use crate::{
    app::{
        App,
        node::{create_button, create_layer, create_text, create_vector_art},
        schema::COLOR_SCHEME,
    },
    clipboard,
    expr,
    gfx::gfxtag,
    mesh::COLOR_CYAN,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{Button, Layer, Text, VectorArt},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("PADDING_Y", PADDING_Y);
    cc.add_const_f32("COPY_WIDTH", COPY_WIDTH);

    let main_layer = wallet_layer.lookup_node("/main_layer").unwrap();

    // Receive layer
    let receive_layer = create_layer("receive_layer");
    let prop = receive_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    receive_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    receive_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let receive_layer = receive_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    wallet_layer.link(receive_layer.clone());

    create_bg_mesh(app, atom, &receive_layer, "receive_bg").await;
    create_header_bg(app, atom, &receive_layer, "receive_header_bg").await;

    // Back button
    let node = create_vector_art("receive_back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    receive_layer.link(node);

    let mut y = 0.;

    let node = create_button("receive_back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let main_is_visible = PropertyBool::wrap(&main_layer, Role::App, "is_visible", 0).unwrap();
    let receive_is_visible = PropertyBool::wrap(&receive_layer, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("receive_back_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("receive back button"));
            receive_is_visible.set(atom, false);
            main_is_visible.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    receive_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &receive_layer, &window_scale, i18n_fish, "RECEIVE", &mut y).await;

    create_subtitle(app, atom, &receive_layer, &window_scale, i18n_fish, "address", "Address", &mut y).await;

    // Address display
    let node = create_text("receive_address");
    let addr_h_prop = node.get_property("height").unwrap();
    let receive_address_text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - COPY_WIDTH").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE * 2.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "").unwrap();
    node.set_property_enum(atom, Role::App, "overflow_wrap", "anywhere").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    receive_layer.link(node);

    // Copy button
    let node = create_vector_art("receive_copy_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - COPY_WIDTH / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile(format!("{y} + (PADDING_Y * 2. + addr_height) / 2")).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_copy(COLOR_CYAN).scaled(COPY_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    receive_layer.link(node);

    let node = create_button("receive_copy_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    let code = cc.compile("w").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    let code = cc.compile("PADDING_Y * 2 + addr_height").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let (slot, recvr) = Slot::new("receive_copy_clicked");
    node.register("click", slot).unwrap();
    let receive_address_text2 = receive_address_text.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let addr = receive_address_text2.get();
            info!(target: "app", "Copy receive address: {addr}");
            clipboard::set(&addr);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    receive_layer.link(node);

    let sep = create_separator_expr(app, atom, &receive_layer, "receive_address_separator", &mut cc, &format!("{y} + PADDING_Y * 2 + addr_height + 1")).await;
    let prop = sep.get_property("rect").unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");

    receive_layer
}
