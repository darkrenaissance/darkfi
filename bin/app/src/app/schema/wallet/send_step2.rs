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
use darkfi::util::parse::encode_base10;
use darkfi_sdk::crypto::keypair::Address;

use crate::{
    app::{
        App,
        node::{create_button, create_layer, create_singleline_edit, create_text, create_vector_art},
        schema::COLOR_SCHEME,
    },
    clipboard,
    expr,
    gfx::gfxtag,
    mesh::COLOR_CYAN,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{Pimpl, SceneNodePtr, Slot},
    shape,
    ui::{BaseEdit, BaseEditType, Button, Layer, Text, VectorArt, VectorShape},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const PASTE_WIDTH: f32 = 200.;
    pub const PASTE_SCALE: f32 = 30.;
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
    pub const PASTE_WIDTH: f32 = 100.;
    pub const PASTE_SCALE: f32 = 15.;
}

pub use ui_consts::*;

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
    send_tx_data: Arc<std::sync::Mutex<SendTxData>>,
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
    cc.add_const_f32("PASTE_WIDTH", PASTE_WIDTH);

    // ============================================
    // Step 2: Recipient layer
    // ============================================
    let send_step2_layer = create_layer("send_step2_layer");
    let prop = send_step2_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step2_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step2_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step2_layer = send_step2_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    wallet_layer.link(send_step2_layer.clone());
    let step2_is_visible = PropertyBool::wrap(&send_step2_layer, Role::App, "is_visible", 0).unwrap();

    let step3_is_visible = app.sg_root.lookup_node("/window/content/wallet/send_step3_layer")
        .and_then(|l| PropertyBool::wrap(&l, Role::App, "is_visible", 0).ok());

    create_bg_mesh(app, atom, &send_step2_layer, "send_bg2").await;
    create_header_bg(app, atom, &send_step2_layer, "send_header_bg2").await;

    // Back button
    let node = create_vector_art("send_back_btn_bg2");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step2_layer.link(node);

    let mut y = 0.;

    let node = create_button("send_back_btn2");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let step1_is_visible2 = step1_is_visible.clone();
    let step2_is_visible1 = step2_is_visible.clone();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("send_back_clicked2");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send step2 back button"));
            step2_is_visible1.set(atom, false);
            step1_is_visible2.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step2_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step2_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Selected token display
    let node = create_text("send_selected_token_symbol");
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
    send_step2_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(&app.renderer, atom, &send_step2_layer, "send_token_separator", &mut y).await;

    // Recipient label
    let node = create_text("send_recipient_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
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
    send_step2_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE;

    // Recipient input outline
    let node = create_vector_art("send_recipient_input_outline");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, RECIPIENT_INPUT_MARGIN).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    let code = cc.compile("w - RECIPIENT_INPUT_MARGIN * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, RECIPIENT_INPUT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        [0.2, 0.2745, 0.2784, 1.],
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step2_layer.link(node);

    // Recipient input
    let recipient_input = create_singleline_edit("send_recipient_input");
    recipient_input.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    recipient_input.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
    let prop = recipient_input.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, RECIPIENT_INPUT_MARGIN + RECIPIENT_INPUT_PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    let code = cc.compile("parent_w - RECIPIENT_INPUT_MARGIN * 2 - RECIPIENT_INPUT_PADDING_X - PASTE_WIDTH").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, RECIPIENT_INPUT_HEIGHT).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "font_size", RECIPIENT_INPUT_FONTSIZE).unwrap();
    recipient_input.set_property_str(atom, Role::App, "placeholder_text", "recipient address...").unwrap();
    let prop = recipient_input.get_property("placeholder_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.45).unwrap();
    let prop = recipient_input.get_property("text_color").unwrap();
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
    let prop = recipient_input.get_property("hi_bg_color").unwrap();
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
    let prop = recipient_input.get_property("text_hi_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.44).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.96).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = recipient_input.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "cursor_ascent", 0.).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "cursor_descent", RECIPIENT_INPUT_FONTSIZE*1.3).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "select_ascent", RECIPIENT_INPUT_FONTSIZE*1.3).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "select_descent", RECIPIENT_INPUT_FONTSIZE/3.).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "handle_descent", RECIPIENT_INPUT_FONTSIZE/2.5).unwrap();
    recipient_input.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let recipient_input = recipient_input
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
    send_step2_layer.link(recipient_input.clone());

    // Paste button
    let node = create_vector_art("send_paste_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - RECIPIENT_INPUT_MARGIN - PASTE_WIDTH / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, y + RECIPIENT_INPUT_HEIGHT / 2.).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_copy(COLOR_CYAN).scaled(PASTE_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step2_layer.link(node);

    let node = create_button("send_paste_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - RECIPIENT_INPUT_MARGIN - PASTE_WIDTH").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, PASTE_WIDTH).unwrap();
    prop.set_f32(atom, Role::App, 3, RECIPIENT_INPUT_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let (slot, recvr) = Slot::new("send_paste_clicked");
    node.register("click", slot).unwrap();
    let recipient_input2 = recipient_input.clone();
    let renderer_clone = app.renderer.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            if let Some(clipboard_text) = clipboard::get() {
                let text_prop = recipient_input2.get_property("text").unwrap();
                let atom = &mut renderer_clone.make_guard(gfxtag!("step2 recipient paste"));
                text_prop.set_str(atom, Role::App, 0, &clipboard_text).unwrap();
                if let crate::scene::Pimpl::Edit(edit) = recipient_input2.pimpl() {
                    edit.on_text_prop_changed();
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step2_layer.link(node);

    y += RECIPIENT_INPUT_MARGIN + RECIPIENT_INPUT_HEIGHT;

    // Add recipient button
    let (node, btn_bg_valid, btn_bg_invalid, add_recipient_label_node) = create_bottom_button_with_states(
        app,
        atom,
        &send_step2_layer,
        "send_add_recipient_btn",
        &mut cc,
        "add recipient",
        &window_scale,
        i18n_fish,
        false,
    ).await;
    let recipient_input2 = recipient_input.clone();
    let recipient_text = recipient_input.get_property("text").unwrap();
    let recipient_text_sub = recipient_text.subscribe_modify();
    let renderer = app.renderer.clone();
    let btn_bg_valid_clone = btn_bg_valid.clone();
    let btn_bg_invalid_clone = btn_bg_invalid.clone();
    let listen_recipient_text = app.ex.spawn(async move {
        while let Ok(_) = recipient_text_sub.receive().await {
            let atom = &mut renderer.make_guard(gfxtag!("wallet recipient input recv"));
            let label_text_color = add_recipient_label_node.get_property("text_color").unwrap();
            let btn_bg_valid_visible = btn_bg_valid_clone.get_property("is_visible").unwrap();
            let btn_bg_invalid_visible = btn_bg_invalid_clone.get_property("is_visible").unwrap();
            let addr = recipient_input2.get_property_str("text").unwrap();
            // Grey color for invalid
            if addr.is_empty() {
                label_text_color.set_f32(atom, Role::App, 0, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 1, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 2, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 3, 1.).unwrap();
                btn_bg_valid_visible.set_bool(atom, Role::App, 0, false).unwrap();
                btn_bg_invalid_visible.set_bool(atom, Role::App, 0, true).unwrap();
            } else {
                // Cyan color for valid
                if addr.clone().parse::<Address>().is_ok() {
                    label_text_color.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
                    label_text_color.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
                    label_text_color.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
                    label_text_color.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
                    btn_bg_valid_visible.set_bool(atom, Role::App, 0, true).unwrap();
                    btn_bg_invalid_visible.set_bool(atom, Role::App, 0, false).unwrap();
                } else {
                    // Grey color for invalid
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
    app.tasks.lock().unwrap().push(listen_recipient_text);

    let step2_is_visible2 = step2_is_visible.clone();
    let renderer = app.renderer.clone();
    let recipient_input2 = recipient_input.clone();
    let send_tx_data3 = send_tx_data.clone();
    let sg_root = app.sg_root.clone();
    let (slot, recvr) = Slot::new("send_add_recipient_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let text = recipient_input2.get_property_str("text").unwrap();
            // Only proceed if address is valid
            let Ok(addr) = text.clone().parse::<Address>() else {
                continue;
            };

            let atom = &mut renderer.make_guard(gfxtag!("add recipient button"));

            let data = {
                let mut tx_data = send_tx_data3.lock().unwrap();
                tx_data.recipient_str = Some(text.clone());
                tx_data.recipient = Some(addr);
                tx_data.clone()
            };

            // Update step3
            if let Some(token_symbol) = &data.token_symbol {
                let token_symbol_node = sg_root.lookup_node("/window/content/wallet/send_step3_layer/send_selected_token_symbol3").unwrap();
                token_symbol_node.set_property_str(atom, Role::App, "text", token_symbol).unwrap();

                // Update amount token symbol
                if let Some(token_symbol_node) = sg_root.lookup_node("/window/content/wallet/send_step3_layer/send_amount_wrapper/send_amount_token_symbol") {
                    token_symbol_node.set_property_str(atom, Role::Internal, "text", token_symbol).unwrap();
                    if let Pimpl::Edit(edit) = token_symbol_node.pimpl() {
                        edit.on_text_prop_changed();
                    }

                    // Update available balance
                    let available_balance = encode_base10(get_balance(&sg_root, &data.token_id.unwrap()).await, BALANCE_BASE10_DECIMALS);
                    if let Some(available_balance_node) = sg_root.lookup_node("/window/content/wallet/send_step3_layer/send_available_balance") {
                        available_balance_node.set_property_str(atom, Role::App, "text", format!("{available_balance} available")).unwrap();
                    }
                }
            }
            if let Some(recipient_str) = &data.recipient_str {
                let recipient_node = sg_root.lookup_node("/window/content/wallet/send_step3_layer/send_recipient_value3").unwrap();
                recipient_node.set_property_str(atom, Role::App, "text", recipient_str).unwrap();
            }

            step2_is_visible2.set(atom, false);
            if let Some(step3) = sg_root.lookup_node("/window/content/wallet/send_step3_layer") {
                step3.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    send_step2_layer
}
