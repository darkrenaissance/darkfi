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

use darkfi_serial::{Decodable, Encodable};

use crate::{
    app::{
        App,
        node::{create_button, create_layer, create_text, create_tokentable, create_vector_art},
        schema::COLOR_SCHEME,
    },
    expr,
    gfx::{gfxtag},
    mesh::{COLOR_CYAN, COLOR_TEAL},
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{Button, Layer, Text, TokenTable, TokenRow, VectorArt, VectorShape},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

pub async fn make_main_wallet_layer(
    app: &App,
    content: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("HEADER_HEIGHT", HEADER_HEIGHT);
    cc.add_const_f32("TOKEN_ROW_HEIGHT", TOKEN_ROW_HEIGHT);
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("PADDING_Y", PADDING_Y);
    cc.add_const_f32("TITLE_FONTSIZE", TITLE_FONTSIZE);
    cc.add_const_f32("BUTTON_HEIGHT", BUTTON_HEIGHT);
    cc.add_const_f32("BUTTON_FONTSIZE", BUTTON_FONTSIZE);
    cc.add_const_f32("ROW_HEIGHT", ROW_HEIGHT);

    // ============================================
    // Main Wallet Layer
    // ============================================
    let wallet_layer = create_layer("wallet_main_layer");
    let prop = wallet_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    wallet_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    wallet_layer.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let wallet_layer = wallet_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(wallet_layer.clone());

    let wallet_is_visible = PropertyBool::wrap(&wallet_layer, Role::App, "is_visible", 0).unwrap();

    create_bg_mesh(app, atom, &wallet_layer, "wallet_bg").await;
    create_header_bg(app, atom, &wallet_layer, "wallet_header_bg").await;

    // Back button
    let node = create_vector_art("wallet_back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    wallet_layer.link(node);

    let node = create_button("wallet_back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let sg_root = app.sg_root.clone();
    let renderer = app.renderer.clone();
    let menu_is_visible =
        PropertyBool::wrap(&sg_root.lookup_node("/window/content/menu_layer").unwrap(), Role::App, "is_visible", 0).unwrap();
    let wallet_is_visible1 = wallet_is_visible.clone();
    let goback = async move || {
        info!(target: "app::wallet", "clicked back from wallet");
        let atom = &mut renderer.make_guard(gfxtag!("wallet goback action"));
        wallet_is_visible1.set(atom, false);
        menu_is_visible.set(atom, true);
    };

    let (slot, recvr) = Slot::new("wallet_back_clicked");
    node.register("click", slot).unwrap();
    let goback2 = goback.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            goback2().await;
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    wallet_layer.link(node);

    let mut y = HEADER_HEIGHT;

    // Balance display
    let node = create_text("wallet_balance");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + TITLE_PADDING).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "DRK 0").unwrap();
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
    wallet_layer.link(node);

    y += TITLE_PADDING * 2. + TITLE_FONTSIZE + 1.;

    create_separator(&app.renderer, atom, &wallet_layer, "wallet_balance_separator", &mut y).await;

    // Receive button bg
    let node = create_vector_art("wallet_receive_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        COLOR_TEAL,
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    wallet_layer.link(node);

    // Receive button click handler
    let node = create_button("wallet_receive_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();

    let renderer = app.renderer.clone();
    let wallet_is_visible2 = wallet_is_visible.clone();
    let sg_root = app.sg_root.clone();
    let (slot, recvr) = Slot::new("receive_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while recvr.recv().await.is_ok() {
            let atom = &mut renderer.make_guard(gfxtag!("receive button click"));
            wallet_is_visible2.set(atom, false);

            let receive_layer = sg_root.lookup_node("/window/content/wallet_receive_layer").unwrap();
            receive_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            // Get the default address from drk plugin and update the UI
            if let Some(drk_node) = sg_root.lookup_node("/plugin/drk") {
                if let Ok(Some(response_data)) = drk_node.call_method("get_default_address", vec![]).await {
                    let mut cur = std::io::Cursor::new(response_data);
                    if let Ok(address) = String::decode(&mut cur) {
                        d!("Got default address from drk: {address}");
                        if let Some(receive_address_node) = receive_layer.lookup_node("/receive_address") {
                            receive_address_node.set_property_str(atom, Role::App, "text", address).unwrap();
                        }
                    } else {
                        e!("Failed to decode default address response");
                    }
                } else {
                    e!("Failed to call get_default_address method");
                }
            } else {
                e!("Failed to lookup drk plugin node");
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    wallet_layer.link(node);

    // Receive label
    let node = create_text("wallet_receive_label");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X + (w / 2 - PADDING_X * 1.5) / 2 - (BUTTON_FONTSIZE * 0.6 * 7) / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_X + BUTTON_HEIGHT / 2. - BUTTON_FONTSIZE / 1.8).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "receive").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
        prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
        prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
        prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    wallet_layer.link(node);

    // Send button bg
    let node = create_vector_art("wallet_send_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X / 2 + w / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        COLOR_TEAL,
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    wallet_layer.link(node);

    // Send button click handler
    let node = create_button("wallet_send_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X / 2 + w / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();

    let renderer = app.renderer.clone();
    let sg_root = app.sg_root.clone();
    let wallet_is_visible3 = wallet_is_visible.clone();
    let (slot, recvr) = Slot::new("send_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send button click"));
            wallet_is_visible3.set(atom, false);
            let send_layer = sg_root.lookup_node("/window/content/wallet_send_step1_layer").unwrap();
            send_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    wallet_layer.link(node);

    // Send label
    let node = create_text("wallet_send_label");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X / 2 + w / 2 + (w / 2 - PADDING_X * 1.5) / 2 - (BUTTON_FONTSIZE * 0.6 * 4) / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_X + BUTTON_HEIGHT / 2. - BUTTON_FONTSIZE / 1.8).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "send").unwrap();
    let prop = node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
        prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
        prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
        prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    wallet_layer.link(node);

    y += PADDING_X * 2. + BUTTON_HEIGHT + 1.;

    create_separator(&app.renderer, atom, &wallet_layer, "wallet_buttons_separator", &mut y).await;

    create_title(app, atom, &wallet_layer, &window_scale, i18n_fish, "TOKENS", &mut y).await;

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("TOKENS_Y", y);

    let tokens_table = create_tokentable("tokens_table");
    let prop = tokens_table.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("TOKENS_Y").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    tokens_table.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    tokens_table.set_property_f32(atom, Role::App, "column_spacing", TOKEN_NAME_OFFSET).unwrap();
    tokens_table.set_property_f32(atom, Role::App, "padding_x", PADDING_X).unwrap();
    tokens_table.set_property_f32(atom, Role::App, "padding_y", PADDING_Y).unwrap();
    tokens_table.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    tokens_table.set_property_u32(atom, Role::App, "priority", 0).unwrap();

    let prop = tokens_table.get_property("text_color").unwrap();
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

    let prop = tokens_table.get_property("separator_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.2).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.2745).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.2784).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let tokens_table = tokens_table
        .setup(|me| TokenTable::new(me, app.renderer.clone(), app.sg_root.clone()))
        .await;
    wallet_layer.link(tokens_table.clone());

    let wallet_layer = wallet_layer.clone();
    let wallet_is_visible2 = wallet_is_visible.clone();
    let tokens_table2 = tokens_table.clone();
    let sg_root2 = app.sg_root.clone();
    let renderer2 = app.renderer.clone();
    let wallet_is_visible_sub = wallet_is_visible.prop().subscribe_modify();
    let listen_wallet_visible = app.ex.spawn(async move {
        while let Ok(_) = wallet_is_visible_sub.receive().await {
            if wallet_is_visible2.get() {
                let atom = &mut renderer2.make_guard(gfxtag!("wallet - refresh tokens"));

                if let Some(drk_node) = sg_root2.lookup_node("/plugin/drk") {
                    if let Ok(Some(response_data)) = drk_node.call_method("get_balances", vec![]).await {
                        let mut cur = std::io::Cursor::new(response_data);
                        if let Ok(balances) = Vec::<(String, darkfi_money_contract::model::TokenId, f32)>::decode(&mut cur) {
                            let token_rows: Vec<TokenRow> = balances
                                .iter()
                                .enumerate()
                                .map(|(i, (symbol, token_id, balance))| TokenRow {
                                    id: *token_id,
                                    symbol: symbol.clone(),
                                    balance: balance.to_string(),
                                })
                                .collect();

                            let mut data: Vec<u8> = vec![];
                            for row in &token_rows {
                                let _ = TokenRow::encode(row, &mut data);
                            }

                            let _ = tokens_table2.call_method("set_tokens", data).await;

                            // Update main wallet balance
                            use darkfi_money_contract::model::DARK_TOKEN_ID;
                            if let Some(drk_balance) = balances.iter().find(|(_, token_id, _)| *token_id == *DARK_TOKEN_ID) {
                                if let Some(balance_node) = sg_root2.lookup_node("/window/content/wallet_main_layer/wallet_balance") {
                                    balance_node.set_property_str(atom, Role::App, "text", format!("DRK {}", drk_balance.2)).unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_wallet_visible);

    wallet_layer
}
