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

use darkfi_serial::{Decodable, Encodable};

use crate::{
    app::{
        App,
        node::{create_button, create_layer, create_text, create_vector_art},
        schema::COLOR_SCHEME,
    },
    expr,
    gfx::gfxtag,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{Button, Layer, Text, VectorArt},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

pub async fn make_send_step4_layer(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
    send_tx_data: Arc<std::sync::Mutex<SendTxData>>,
    step2_is_visible: PropertyBool,
    step3_is_visible: PropertyBool,
    step1_is_visible: PropertyBool,
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

    // ============================================
    // Step 4: Confirmation layer content
    // ============================================
    let send_step4_layer = create_layer("send_step4_layer");
    let prop = send_step4_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step4_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step4_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step4_layer = send_step4_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    wallet_layer.link(send_step4_layer.clone());
    let step4_is_visible = PropertyBool::wrap(&send_step4_layer, Role::App, "is_visible", 0).unwrap();

    let tx_status_is_visible = app.sg_root.lookup_node("/window/content/wallet/tx_status_layer")
        .and_then(|l| PropertyBool::wrap(&l, Role::App, "is_visible", 0).ok());

    create_bg_mesh(app, atom, &send_step4_layer, "send_bg4").await;
    create_header_bg(app, atom, &send_step4_layer, "send_header_bg4").await;

    // Back button
    let node = create_vector_art("send_back_btn_bg4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step4_layer.link(node);

    let mut y = 0.;

    let node = create_button("send_back_btn4");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let step3_is_visible2 = step3_is_visible.clone();
    let step4_is_visible1 = step4_is_visible.clone();
    let renderer = app.renderer.clone();
    let sg_root2 = app.sg_root.clone();
    let (slot, recvr) = Slot::new("send_back_clicked4");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send step4 back button"));
            step4_is_visible1.set(atom, false);
            step3_is_visible2.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step4_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step4_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Selected token display
    let node = create_text("send_selected_token_symbol4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Token").unwrap();
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
    send_step4_layer.link(node);

    let node = create_text("send_selected_token_name4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X + TOKEN_NAME_OFFSET).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "").unwrap();
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
    send_step4_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(&app.renderer, atom, &send_step4_layer, "send_token_separator4", &mut y).await;

    // Recipient label
    let node = create_text("send_recipient_label4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Recipient").unwrap();
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
    send_step4_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    // Recipient address value
    let recipient_addr_node = create_text("send_recipient_value4");
    let addr_h_prop = recipient_addr_node.get_property("height").unwrap();
    let prop = recipient_addr_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    recipient_addr_node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
    recipient_addr_node.set_property_str(atom, Role::App, "text", "").unwrap();
    recipient_addr_node.set_property_enum(atom, Role::App, "overflow_wrap", "anywhere").unwrap();
    let prop = recipient_addr_node.get_property("text_color").unwrap();
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
    recipient_addr_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let recipient_addr_node = recipient_addr_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step4_layer.link(recipient_addr_node.clone());

    // Separator line
    let mut y_ = y.clone();
    let y2 = format!("{y} + (PADDING_Y * 2. + addr_height) + 1");
    let node = create_separator(&app.renderer, atom, &send_step4_layer, "send_amount_label_separator4", &mut y_).await;
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile(&y2).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");

    // Amount display - same layout as step3's amount input
    let amount_y = format!("({y2}) + (h - ({y2}) - BUTTON_HEIGHT - PADDING_X * 2 - PADDING_Y - BASE_FONTSIZE - AMOUNT_FONTSIZE - 1) / 2 - AMOUNT_FONTSIZE * 0.1");
    let amount_wrapper = create_layer("send_amount_wrapper4");
    amount_wrapper.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    let prop = amount_wrapper.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(&amount_y).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    amount_wrapper.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let amount_wrapper = amount_wrapper.setup(|me| Layer::new(me, app.renderer.clone())).await;
    send_step4_layer.link(amount_wrapper.clone());

    // Amount text
    let amount_text_node = create_text("send_amount_text4");
    let prop = amount_text_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    amount_text_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    let prop = amount_text_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    amount_text_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    amount_text_node.set_property_str(atom, Role::App, "text", "0").unwrap();
    let amount_text_node = amount_text_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    amount_wrapper.link(amount_text_node.clone());

    // Token symbol text node (displayed next to amount)
    let token_symbol_node = create_text("send_amount_token_symbol4");
    let prop = token_symbol_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(amount_y).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    token_symbol_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    token_symbol_node.set_property_str(atom, Role::App, "text", "").unwrap();
    let prop = token_symbol_node.get_property("text_color").unwrap();
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
    token_symbol_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let token_symbol_node = token_symbol_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step4_layer.link(token_symbol_node.clone());

    // Transaction fee label
    let tx_fee_label_node = create_text("send_fee_label");
    let prop = tx_fee_label_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - BASE_FONTSIZE - 1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    tx_fee_label_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    tx_fee_label_node.set_property_str(atom, Role::App, "text", "Transaction fee").unwrap();
    let prop = tx_fee_label_node.get_property("text_color").unwrap();
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
    tx_fee_label_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    tx_fee_label_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    let prop = tx_fee_label_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();
    let tx_fee_label_node = tx_fee_label_node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    send_step4_layer.link(tx_fee_label_node.clone());

    // Transaction fee value
    let tx_fee_value_node = create_text("send_fee_value");
    let prop = tx_fee_value_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - BASE_FONTSIZE - 1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    tx_fee_value_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    tx_fee_value_node.set_property_str(atom, Role::App, "text", "0 DRK").unwrap();
    tx_fee_value_node.set_property_enum(atom, Role::App, "text_align", "end").unwrap();
    tx_fee_value_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let prop = tx_fee_value_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();
    let tx_fee_value_node = tx_fee_value_node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    send_step4_layer.link(tx_fee_value_node.clone());

    // Separator line
    create_separator_expr(
        app,
        atom,
        &send_step4_layer,
        "tx_fee_separator",
        &mut cc,
        "h - PADDING_X * 2 - BUTTON_HEIGHT",
    ).await;

    y += PADDING_Y * 2. + BUTTON_HEIGHT + 10.;

    // Send button (bottom button)
    let (node, _bg_valid, _bg_invalid, _label) = create_bottom_button_with_states(
        app,
        atom,
        &send_step4_layer,
        "send_send_btn",
        &mut cc,
        "send",
        &window_scale,
        i18n_fish,
        true,
    ).await;

    let renderer = app.renderer.clone();
    let sg_root = app.sg_root.clone();
    let step4_is_visible1 = step4_is_visible.clone();
    let send_tx_data_for_send = send_tx_data.clone();
    let (slot, recvr) = Slot::new("send_send_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            // Skip if the button is disabled
            if let Some(btn_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_bg") {
                if !btn_node.get_property_bool("is_visible").unwrap() {
                    continue;
                }
            }
            let atom = &mut renderer.make_guard(gfxtag!("send button"));

            step4_is_visible1.set(atom, false);
            if let Some(tx_status) = sg_root.lookup_node("/window/content/wallet/tx_status_layer") {
                tx_status.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
            }

            // Broadcast
            let tx = send_tx_data_for_send.lock().unwrap().tx.clone();
            if let (Some(tx), Some(drk_node)) = (tx, sg_root.lookup_node("/plugin/drk")) {
                let mut encoded_data = vec![];
                tx.encode(&mut encoded_data).unwrap();
                if let Ok(Some(response_data)) = drk_node.call_method("broadcast_tx", encoded_data).await {
                    let mut cur = std::io::Cursor::new(response_data);
                    if let Ok(tx_id) = String::decode(&mut cur) {
                        d!("Transaction broadcasted: {tx_id}");
                        let mut tx_id_data = vec![];
                        tx_id.encode(&mut tx_id_data).unwrap();
                        if let Ok(Some(data)) = drk_node.call_method("get_tx_status", tx_id_data).await {
                            let mut cur = std::io::Cursor::new(data);
                            if let Ok(status_text) = String::decode(&mut cur) {
                                if let Some(tx_status) = sg_root.lookup_node("/window/content/wallet/tx_status_layer/status") {
                                    tx_status.set_property_str(atom, Role::App, "text", status_text).unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Add listener for step4 visibility to update amount positions
    let step4_is_visible_clone = step4_is_visible.clone();
    let renderer_clone = app.renderer.clone();
    let amount_wrapper_clone = amount_wrapper.clone();
    let amount_text_node_clone = amount_text_node.clone();
    let token_symbol_node_clone = token_symbol_node.clone();
    let send_tx_data_clone2 = send_tx_data.clone();
    let sg_root = app.sg_root.clone();
    let step4_is_visible_sub = step4_is_visible.prop().subscribe_modify();
    let listen_step4_visible = app.ex.spawn(async move {
        while let Ok(_) = step4_is_visible_sub.receive().await {
            if step4_is_visible_clone.get() {
                loop {
                    darkfi::system::msleep(50).await;
                    let text_rect = crate::prop::PropertyRect::wrap(&amount_text_node_clone, Role::App, "rect").unwrap();
                    if text_rect.has_cached() {
                        break;
                    }
                }

                let atom = &mut renderer_clone.make_guard(gfxtag!("update step4 amount positions"));
                let data = send_tx_data_clone2.lock().unwrap().clone();

                let amount_text = data.amount.unwrap_or_else(|| "0".to_string());
                let token_symbol = data.token_symbol.unwrap_or_else(|| "".to_string());
                let token_id = data.token_id.unwrap();

                // Update positions to center amount and token symbol
                update_amount_screen(
                    atom,
                    &sg_root,
                    &amount_text,
                    &token_id,
                    &token_symbol,
                    &amount_wrapper_clone,
                    &amount_text_node_clone,
                    &token_symbol_node_clone,
                    None,
                ).await;

                if data.tx_built {
                    // Set send button label
                    if let Some(send_label_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_label") {
                        send_label_node.set_property_str(atom, Role::App, "text", "send").unwrap();
                    }
                    // Show transaction fee
                    if let Some(tx_fee_label) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_fee_label") {
                        let prop = tx_fee_label.get_property("text_color").unwrap();
                        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
                        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
                        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
                        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
                    }
                    if let Some(tx_fee_value) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_fee_value") {
                        let prop = tx_fee_value.get_property("text_color").unwrap();
                        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
                        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
                        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
                        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
                    }
                } else {
                    // Hide transaction fee
                    if let Some(tx_fee_label) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_fee_label") {
                        let prop = tx_fee_label.get_property("text_color").unwrap();
                        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
                        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
                        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
                        prop.set_f32(atom, Role::App, 3, 0.).unwrap();
                    }
                    if let Some(tx_fee_value) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_fee_value") {
                        let prop = tx_fee_value.get_property("text_color").unwrap();
                        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
                        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
                        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
                        prop.set_f32(atom, Role::App, 3, 0.).unwrap();
                    }
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_step4_visible);

    send_step4_layer
}
