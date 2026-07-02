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

use darkfi::util::parse::decode_base10;
use darkfi_serial::Encodable;

use crate::{
    app::{
        App,
        node::{create_button, create_layer, create_decimal_edit, create_singleline_edit, create_text, create_vector_art},
        schema::COLOR_SCHEME,
    },
    expr,
    gfx::gfxtag,
    mesh::COLOR_CYAN,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyRect, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{BaseEdit, BaseEditType, Button, Layer, Text, VectorArt},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
    send_tx_data: Arc<std::sync::Mutex<SendTxData>>,
    step1_is_visible: PropertyBool,
    step2_is_visible: PropertyBool,
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
    // Step 3: Amount layer
    // ============================================
    let send_step3_layer = create_layer("send_step3_layer");
    let prop = send_step3_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step3_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step3_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step3_layer = send_step3_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    wallet_layer.link(send_step3_layer.clone());
    let step3_is_visible = PropertyBool::wrap(&send_step3_layer, Role::App, "is_visible", 0).unwrap();

    let step4_is_visible = app.sg_root.lookup_node("/window/content/wallet/send_step4_layer")
        .and_then(|l| PropertyBool::wrap(&l, Role::App, "is_visible", 0).ok());

    create_bg_mesh(app, atom, &send_step3_layer, "send_bg3").await;
    create_header_bg(app, atom, &send_step3_layer, "send_header_bg3").await;

    // Back button
    let node = create_vector_art("send_back_btn_bg3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step3_layer.link(node);

    let mut y = 0.;

    let node = create_button("send_back_btn3");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let step2_is_visible3 = step2_is_visible.clone();
    let step3_is_visible1 = step3_is_visible.clone();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("send_back_clicked3");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send step3 back button"));
            step3_is_visible1.set(atom, false);
            step2_is_visible3.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step3_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step3_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Selected token display (same layout as step2)
    let node = create_text("send_selected_token_symbol3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    // Set initial token value from send_tx_data
    let token_symbol_text = send_tx_data.lock().unwrap().token_symbol.clone().unwrap_or_else(|| "".to_string());
    node.set_property_str(atom, Role::App, "text", &token_symbol_text).unwrap();
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
    send_step3_layer.link(node);

    let node = create_text("send_selected_token_name3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X + TOKEN_NAME_OFFSET).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    // Set initial token value from send_tx_data
    let token_name_text = send_tx_data.lock().unwrap().token_name.clone().unwrap_or_else(|| "".to_string());
    node.set_property_str(atom, Role::App, "text", &token_name_text).unwrap();
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
    let selected_token_text3 = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step3_layer.link(selected_token_text3.clone());

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(&app.renderer, atom, &send_step3_layer, "send_token_separator3", &mut y).await;

    // Recipient display
    let node = create_text("send_recipient_label3");
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
    send_step3_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    // Recipient address value
    let node = create_text("send_recipient_value3");
    let addr_h_prop = node.get_property("height").unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
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
    send_step3_layer.link(node);

    // Separator line
    let mut y_ = y.clone();
    let y2 = format!("{y} + (PADDING_Y * 2. + addr_height) + 1");
    let node = create_separator(&app.renderer, atom, &send_step3_layer, "send_amount_label_separator", &mut y_).await;
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile(&y2).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");

    // Available balance text
    let available_balance_node = create_text("send_available_balance");
    let prop = available_balance_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - BASE_FONTSIZE - 1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    available_balance_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    available_balance_node.set_property_str(atom, Role::App, "text", "").unwrap();
    let prop = available_balance_node.get_property("text_color").unwrap();
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
    available_balance_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let available_balance_node = available_balance_node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    send_step3_layer.link(available_balance_node.clone());

    // Separator line below available balance
    create_separator_expr(
        app,
        atom,
        &send_step3_layer,
        "available_balance_separator",
        &mut cc,
        "h - PADDING_X * 2 - BUTTON_HEIGHT",
    ).await;

    // Amount wrapper layer
    let amount_y = format!("({y2}) + (h - ({y2}) - BUTTON_HEIGHT - PADDING_X * 2 - PADDING_Y - BASE_FONTSIZE - AMOUNT_FONTSIZE - 1) / 2");
    let amount_wrapper = create_layer("send_amount_wrapper");
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
    send_step3_layer.link(amount_wrapper.clone());

    // Amount input
    let input_node = create_decimal_edit("send_amount_input");
    input_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = input_node.get_property("rect").unwrap();
    // Position at (0, 0) within wrapper - wrapper controls the position
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    cc.add_const_f32("AMOUNT_CHAR_WIDTH", AMOUNT_CHAR_WIDTH+6.); // Initial width for "0"
    let code = cc.compile("AMOUNT_CHAR_WIDTH").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    input_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    let prop = input_node.get_property("text_color").unwrap();
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
    let prop = input_node.get_property("hi_bg_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    let prop = input_node.get_property("text_hi_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.44).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.96).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = input_node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    input_node.set_property_f32(atom, Role::App, "cursor_ascent", 0.).unwrap();
    input_node.set_property_f32(atom, Role::App, "cursor_descent", AMOUNT_FONTSIZE*1.3).unwrap();
    input_node.set_property_f32(atom, Role::App, "select_ascent", AMOUNT_FONTSIZE*1.3).unwrap();
    input_node.set_property_f32(atom, Role::App, "select_descent", AMOUNT_FONTSIZE/3.).unwrap();
    input_node.set_property_f32(atom, Role::App, "handle_descent", AMOUNT_FONTSIZE/2.5).unwrap();
    input_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    input_node.set_property_str(atom, Role::App, "placeholder_text", "0").unwrap();
    let prop = input_node.get_property("placeholder_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let input_node = input_node
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
    amount_wrapper.link(input_node.clone());

    // Token symbol node (displayed next to amount)
    // Singleline edit so that it has the exact same Y position as the amount input
    let token_symbol_node = create_singleline_edit("send_amount_token_symbol");
    token_symbol_node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
    let prop = token_symbol_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    token_symbol_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
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
    let prop = token_symbol_node.get_property("hi_bg_color").unwrap();
    if COLOR_SCHEME == ColorScheme::PaperLight {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    let prop = token_symbol_node.get_property("text_hi_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.44).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.96).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = token_symbol_node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    token_symbol_node.set_property_f32(atom, Role::App, "cursor_ascent", 0.).unwrap();
    token_symbol_node.set_property_f32(atom, Role::App, "cursor_descent", AMOUNT_FONTSIZE*1.3).unwrap();
    token_symbol_node.set_property_f32(atom, Role::App, "select_ascent", AMOUNT_FONTSIZE*1.3).unwrap();
    token_symbol_node.set_property_f32(atom, Role::App, "select_descent", AMOUNT_FONTSIZE/3.).unwrap();
    token_symbol_node.set_property_f32(atom, Role::App, "handle_descent", AMOUNT_FONTSIZE/2.5).unwrap();
    token_symbol_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let token_symbol_node = token_symbol_node
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
    amount_wrapper.link(token_symbol_node.clone());

    y += PADDING_Y * 2. + BUTTON_HEIGHT + 10.;

    // Add amount button with states
    let (node, btn_bg_valid, btn_bg_invalid, add_amount_label_node) = create_bottom_button_with_states(
        app,
        atom,
        &send_step3_layer,
        "send_add_amount_btn",
        &mut cc,
        "add amount",
        &window_scale,
        i18n_fish,
        false, // Start with invalid state (grey)
    ).await;

    // Amount input validation listener
    let amount_input2 = input_node.clone();
    let amount_wrapper2 = amount_wrapper.clone();
    let available_balance_node2 = available_balance_node.clone();
    let token_symbol_node2 = token_symbol_node.clone();
    let send_tx_data5 = send_tx_data.clone();
    let amount_text = amount_input2.get_property("text").unwrap();
    let amount_text_sub = amount_text.subscribe_modify();
    let renderer = app.renderer.clone();
    let sg_root = app.sg_root.clone();
    let btn_bg_valid_clone = btn_bg_valid.clone();
    let btn_bg_invalid_clone = btn_bg_invalid.clone();
    let add_amount_label_node_for_validation = add_amount_label_node.clone();
    let listen_amount_text = app.ex.spawn(async move {
        while let Ok(_) = amount_text_sub.receive().await {
            let atom = &mut renderer.make_guard(gfxtag!("wallet amount input recv"));
            let label_text_color = add_amount_label_node_for_validation.get_property("text_color").unwrap();
            let btn_bg_valid_visible = btn_bg_valid_clone.get_property("is_visible").unwrap();
            let btn_bg_invalid_visible = btn_bg_invalid_clone.get_property("is_visible").unwrap();
            let amount = amount_input2.get_property_str("text").unwrap();

            let sanitized_amount = if amount.is_empty() {
                "0".to_string()
            } else {
                // Validate by parsing as f64, return "0" if invalid
                amount.parse::<f64>().map_or("0".to_string(), |_| amount.clone())
            };

            let (token_id, token_symbol) = {
                let data = send_tx_data5.lock().unwrap();
                (data.token_id, data.token_symbol.clone().unwrap_or_default())
            };
            if let Some(token_id) = token_id {
                update_amount_screen(
                    atom,
                    &sg_root,
                    &sanitized_amount,
                    &token_id,
                    &token_symbol,
                    &amount_wrapper2,
                    &amount_input2,
                    &token_symbol_node2,
                    Some(&available_balance_node2),
                ).await;
                let available_balance = get_balance(&sg_root, &token_id).await;
                let is_valid = if sanitized_amount == "0" {
                    false
                } else {
                    match decode_base10(&sanitized_amount, BALANCE_BASE10_DECIMALS, false) {
                        Ok(v) if v > 0 && v <= available_balance => true, // TODO: fees
                        _ => false,
                    }
                };

                if is_valid {
                    label_text_color.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
                    label_text_color.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
                    label_text_color.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
                    label_text_color.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
                    btn_bg_valid_visible.set_bool(atom, Role::App, 0, true).unwrap();
                    btn_bg_invalid_visible.set_bool(atom, Role::App, 0, false).unwrap();
                } else {
                    label_text_color.set_f32(atom, Role::App, 0, 0.5).unwrap();
                    label_text_color.set_f32(atom, Role::App, 1, 0.5).unwrap();
                    label_text_color.set_f32(atom, Role::App, 2, 0.5).unwrap();
                    label_text_color.set_f32(atom, Role::App, 3, 1.).unwrap();
                    btn_bg_valid_visible.set_bool(atom, Role::App, 0, false).unwrap();
                    btn_bg_invalid_visible.set_bool(atom, Role::App, 0, true).unwrap();
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_amount_text);

    let renderer = app.renderer.clone();
    let amount_input2 = input_node.clone();
    let send_tx_data4 = send_tx_data.clone();
    let step3_is_visible2 = step3_is_visible.clone();
    let sg_root = app.sg_root.clone();
    let (slot, recvr) = Slot::new("send_add_amount_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let text = amount_input2.get_property_str("text").unwrap();
            // Use sanitized amount (empty text becomes "0")
            let sanitized_text = if text.is_empty() { "0".to_string() } else { text.clone() };
            let token_id = send_tx_data4.lock().unwrap().token_id.clone().unwrap();
            let available_balance = get_balance(&sg_root, &token_id).await;
            let is_valid = if sanitized_text.is_empty() {
                false
            } else {
                match decode_base10(&sanitized_text, BALANCE_BASE10_DECIMALS, false) {
                    Ok(v) if v > 0 && v <= available_balance => true,
                    _ => false,
                }
            };
            if is_valid {
                let atom = &mut renderer.make_guard(gfxtag!("switch to step4 and show building"));

                // Store the amount (use sanitized value)
                let mut data = send_tx_data4.lock().unwrap();
                data.amount = Some(sanitized_text.clone());

                // Update step4 display with transaction data
                if let Some(token_symbol_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_selected_token_symbol4") {
                    token_symbol_node.set_property_str(atom, Role::App, "text", &data.token_symbol.clone().unwrap()).unwrap();
                }
                if let Some(amount_token_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_amount_wrapper4/send_amount_token_symbol4") {
                    amount_token_node.set_property_str(atom, Role::App, "text", &data.token_symbol.clone().unwrap()).unwrap();
                }
                if let Some(recipient_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_recipient_value4") {
                    recipient_node.set_property_str(atom, Role::App, "text", &data.recipient_str.clone().unwrap()).unwrap();
                }
                if let Some(amount_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_amount_wrapper4/send_amount_text4") {
                    amount_node.set_property_str(atom, Role::App, "text", &data.amount.clone().unwrap()).unwrap();
                }

                // Switch to step4 and set send button to "building tx..."
                step3_is_visible2.set(atom, false);
                if let Some(step4) = sg_root.lookup_node("/window/content/wallet/send_step4_layer") {
                    step4.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                }
                if let Some(send_label_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_label") {
                    send_label_node.set_property_str(atom, Role::App, "text", "building tx...").unwrap();
                    let prop = send_label_node.get_property("text_color").unwrap();
                    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
                    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
                    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
                    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
                }
                if let Some(send_bg_grey_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_bg_grey") {
                    send_bg_grey_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                }
                if let Some(send_bg_node) = sg_root.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_bg") {
                    send_bg_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }

                // Build the transaction via DrkPlugin
                if let (Some(drk_node), Some(token_id), Some(recipient)) = (
                    sg_root.lookup_node("/plugin/drk"),
                    data.token_id,
                    data.recipient,
                ) {
                    let mut encoded_data = vec![];
                    data.amount.clone().unwrap().encode(&mut encoded_data).unwrap();
                    token_id.encode(&mut encoded_data).unwrap();
                    recipient.public_key().encode(&mut encoded_data).unwrap();

                    // Call build_tx - returns immediately, emits tx_built signal when done
                    let _ = smol::block_on(drk_node.call_method("build_tx", encoded_data));
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Add listener for step3 visibility to focus/unfocus amount input
    let step3_is_visible_clone = step3_is_visible.clone();
    let renderer_clone = app.renderer.clone();
    let sg_root = app.sg_root.clone();
    let amount_wrapper_clone = amount_wrapper.clone();
    let input_node_clone = input_node.clone();
    let token_symbol_node_clone = token_symbol_node.clone();
    let available_balance_node_clone = available_balance_node.clone();
    let send_tx_data_clone = send_tx_data.clone();
    let step3_is_visible_sub = step3_is_visible.prop().subscribe_modify();
    let listen_step3_visible = app.ex.spawn(async move {
        while let Ok(_) = step3_is_visible_sub.receive().await {
            if step3_is_visible_clone.get() {
                loop { // TODO: this waits for draw, there's probably a better way
                    darkfi::system::msleep(50).await;
                    let input_rect = PropertyRect::wrap(&input_node_clone, Role::App, "rect").unwrap();
                    if input_rect.has_cached() {
                        break;
                    }
                }

                // Focus the amount input
                input_node_clone.call_method("focus", vec![]).await.unwrap();

                let (token_id, token_symbol) = {
                    let data = send_tx_data_clone.lock().unwrap();
                    (data.token_id.clone(), data.token_symbol.clone().unwrap_or_else(|| "".to_string()))
                };
                if let Some(token_id) = token_id {
                    if !token_symbol.is_empty() {
                        let atom = &mut renderer_clone.make_guard(gfxtag!("update amount positions on visible"));
                        let current_amount = input_node_clone.get_property_str("text").unwrap();
                        update_amount_screen(
                            atom,
                            &sg_root,
                            &current_amount,
                            &token_id,
                            &token_symbol,
                            &amount_wrapper_clone,
                            &input_node_clone,
                            &token_symbol_node_clone,
                            Some(&available_balance_node_clone),
                        ).await;
                    }
                }
            } else {
                // Unfocus when becoming hidden
                input_node_clone.call_method("unfocus", vec![]).await.unwrap();
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_step3_visible);

    send_step3_layer
}
