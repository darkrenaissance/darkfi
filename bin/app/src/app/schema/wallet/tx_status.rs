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

use std::sync::Arc;

use darkfi_serial::Decodable;
use darkfi::tx::Transaction;

use crate::{
    app::{
        App,
        node::{create_layer, create_text},
        schema::COLOR_SCHEME,
    },
    expr,
    gfx::gfxtag,
    prop::{Property, PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertySubType, PropertyType, Role},
    scene::{SceneNodePtr, Slot},
    ui::{Layer, Text},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
    send_tx_data: Arc<std::sync::Mutex<SendTxData>>,
) -> SceneNodePtr {
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("HEADER_HEIGHT", HEADER_HEIGHT);
    cc.add_const_f32("BUTTON_HEIGHT", BUTTON_HEIGHT);
    cc.add_const_f32("BUTTON_FONTSIZE", BUTTON_FONTSIZE);
    cc.add_const_f32("AMOUNT_FONTSIZE", AMOUNT_FONTSIZE);
    cc.add_const_f32("BASE_FONTSIZE", BASE_FONTSIZE);
    cc.add_const_f32("HINT_FONTSIZE", HINT_FONTSIZE);
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("PADDING_Y", PADDING_Y);
    cc.add_const_f32("RECIPIENT_INPUT_MARGIN", RECIPIENT_INPUT_MARGIN);
    cc.add_const_f32("RECIPIENT_INPUT_PADDING_X", RECIPIENT_INPUT_PADDING_X);

    let main_layer = wallet_layer.lookup_node("/main_layer").unwrap();

    // ============================================
    // Transaction status layer
    // ============================================
    let mut tx_status_layer = create_layer("tx_status_layer");

    // Add methods for plugin calls
    tx_status_layer
        .add_method(
            "set_tx_status",
            vec![
                ("tx_id", "Transaction ID", crate::scene::CallArgType::Str),
                ("status", "Status text", crate::scene::CallArgType::Str),
                ("amount", "Amount", crate::scene::CallArgType::Str),
                ("token_symbol", "Token symbol", crate::scene::CallArgType::Str),
                ("recipient", "Recipient address", crate::scene::CallArgType::Str),
            ],
            None,
        )
        .unwrap();

    tx_status_layer
        .add_method(
            "set_built_tx",
            vec![("tx", "Transaction", crate::scene::CallArgType::Hash)],
            None,
        )
        .unwrap();

    // Add tx_id property to store the transaction ID for status polling
    let mut prop = Property::new("tx_id", PropertyType::Str, PropertySubType::Null);
    prop.set_defaults_str(vec!["".to_string()]).unwrap();
    tx_status_layer.add_property(prop).unwrap();

    let prop = tx_status_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    tx_status_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    tx_status_layer.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let tx_status_layer = tx_status_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    wallet_layer.link(tx_status_layer.clone());
    let tx_status_is_visible = PropertyBool::wrap(&tx_status_layer, Role::App, "is_visible", 0).unwrap();

    create_bg_mesh(app, atom, &tx_status_layer, "tx_status_bg").await;
    create_header_bg(app, atom, &tx_status_layer, "tx_status_header_bg").await;

    let mut y = HEADER_HEIGHT;

    create_title(app, atom, &tx_status_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Status text
    let node = create_text("status");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Broadcasting...").unwrap();
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
    tx_status_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(&app.renderer, atom, &tx_status_layer, "tx_status_separator", &mut y).await;

    // Transaction info text: "Sending {amount} {token_symbol} to {recipient_address}"
    let node = create_text("tx_info");
    let info_h_prop = node.get_property("height").unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Sending 0 DRK to recipient").unwrap();
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
    tx_status_layer.link(node);

    let sep = create_separator(&app.renderer, atom, &tx_status_layer, "tx_info_separator", &mut 0.).await;
    let prop = sep.get_property("rect").unwrap();
    let code = cc.compile(format!("{y} + PADDING_Y * 2 + info_height + 1")).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.add_depend(&info_h_prop, 0, "info_height");

    // Hint text
    let hint_node = create_text("tx_status_hint");
    let prop = hint_node.get_property("rect").unwrap();
    let code = cc.compile("w / 2 - (HINT_FONTSIZE * 0.7 * 31) / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - HINT_FONTSIZE * 2").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("HINT_FONTSIZE * 0.7 * 31").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, HINT_FONTSIZE/2.).unwrap();
    hint_node.set_property_f32(atom, Role::App, "font_size", HINT_FONTSIZE).unwrap();
    hint_node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
    hint_node.set_property_str(atom, Role::App, "text", "You can close this screen while the transaction is confirming.").unwrap();
    let prop = hint_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.45).unwrap();
    hint_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let hint_node = hint_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    tx_status_layer.link(hint_node.clone());

    // Separator line
    create_separator_expr(
        app,
        atom,
        &tx_status_layer,
        "tx_status_hint_separator",
        &mut cc,
        "h - PADDING_X * 2 - BUTTON_HEIGHT",
    ).await;

    // Close button
    let node = create_bottom_button(
        app,
        atom,
        &tx_status_layer,
        "tx_status_close_btn",
        &mut cc,
        Some("close"),
        &window_scale,
        i18n_fish,
    ).await;

    let main_is_visible = PropertyBool::wrap(&main_layer, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let tx_status_is_visible1 = tx_status_is_visible.clone();
    let send_tx_data2 = send_tx_data.clone();
    let (slot, recvr) = Slot::new("tx_status_close_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("tx status close button"));
            tx_status_is_visible1.set(atom, false);
            main_is_visible.set(atom, true);

            // Reset send_tx_data
            *send_tx_data2.lock().unwrap() = SendTxData::new();

            // TODO: reset all send inputs
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Subscribe to method for receiving tx status updates from drk plugin
    let set_tx_status_sub = tx_status_layer.subscribe_method_call("set_tx_status").unwrap();
    let tx_status_layer_clone = tx_status_layer.clone();
    let sg_root = app.sg_root.clone();
    let renderer = app.renderer.clone();
    app.tasks.lock().unwrap().push(app.ex.spawn(async move {
        while let Ok(mcall) = set_tx_status_sub.receive().await {
            let atom = &mut renderer.make_guard(gfxtag!("set_tx_status"));

            let mut cur = std::io::Cursor::new(mcall.data);
            let tx_id = Option::<String>::decode(&mut cur).unwrap();
            let status_text = Option::<String>::decode(&mut cur).unwrap();
            let amount = Option::<String>::decode(&mut cur).unwrap();
            let token_symbol = Option::<String>::decode(&mut cur).unwrap();
            let recipient_str = Option::<String>::decode(&mut cur).unwrap();

            // Store the tx_id
            if let Some(id) = tx_id.as_ref() {
                tx_status_layer_clone.set_property_str(atom, Role::App, "tx_id", id).unwrap();
            }

            // Update status text
            if let Some(text) = status_text {
                if let Some(status_node) = sg_root.lookup_node("/window/content/wallet/tx_status_layer/status") {
                    status_node.set_property_str(atom, Role::App, "text", &text).unwrap();
                }
            }

            // Update transaction info display
            if let (Some(amount), Some(token_symbol), Some(recipient_str)) = (amount, token_symbol, recipient_str) {
                if let Some(tx_info) = sg_root.lookup_node("/window/content/wallet/tx_status_layer/tx_info") {
                    let tx_text = format!("Sending {} {} to {}", amount, token_symbol, recipient_str);
                    tx_info.set_property_str(atom, Role::App, "text", tx_text).unwrap();
                }
            }
        }
    }));

    // Subscribe to method for receiving built transaction
    let set_built_tx_sub = tx_status_layer.subscribe_method_call("set_built_tx").unwrap();
    let send_tx_data2 = send_tx_data.clone();
    app.tasks.lock().unwrap().push(app.ex.spawn(async move {
        while let Ok(mcall) = set_built_tx_sub.receive().await {
            let mut cur = std::io::Cursor::new(mcall.data);
            let tx = Transaction::decode(&mut cur).unwrap();

            send_tx_data2.lock().unwrap().tx = Some(tx);
            send_tx_data2.lock().unwrap().tx_built = true;
        }
    }));

    tx_status_layer
}
