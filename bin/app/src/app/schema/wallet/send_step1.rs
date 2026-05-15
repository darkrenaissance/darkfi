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

use darkfi_money_contract::model::DARK_TOKEN_ID;
use darkfi_serial::{Decodable, Encodable};

use crate::{
    app::{
        App,
        node::{create_button, create_layer, create_tokentable, create_vector_art},
        schema::COLOR_SCHEME,
    },
    expr,
    gfx::gfxtag,
    mesh::COLOR_CYAN,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{SceneNodePtr, Slot},
    shape,
    ui::{Button, Layer, TokenTable, TokenRow, VectorArt},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*, util::*};

pub async fn make_send_step1_layer(
    app: &App,
    content: SceneNodePtr,
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

    // ============================================
    // Step 1: Pick token layer
    // ============================================
    let send_step1_layer = create_layer("wallet_send_step1_layer");
    let prop = send_step1_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step1_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step1_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step1_layer = send_step1_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(send_step1_layer.clone());
    let step1_is_visible = PropertyBool::wrap(&send_step1_layer, Role::App, "is_visible", 0).unwrap();

    create_bg_mesh(app, atom, &send_step1_layer, "send_bg").await;
    create_header_bg(app, atom, &send_step1_layer, "send_header_bg").await;

    // Back button
    let node = create_vector_art("send_back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step1_layer.link(node);

    let mut y = 0.;

    let node = create_button("send_back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let wallet_is_visible = PropertyBool::wrap(&wallet_layer, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let wallet_is_visible1 = wallet_is_visible.clone();
    let step1_is_visible1 = step1_is_visible.clone();
    let (slot, recvr) = Slot::new("send_back_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send back button"));
            wallet_is_visible1.set(atom, true);
            step1_is_visible1.set(atom, false);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step1_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step1_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    create_subtitle(app, atom, &send_step1_layer, &window_scale, i18n_fish, "pick_label", "Pick a token to send", &mut y).await;

    let send_tokens_table = create_tokentable("tokens_table");
    let prop = send_tokens_table.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_tokens_table.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    send_tokens_table.set_property_f32(atom, Role::App, "column_spacing", TOKEN_NAME_OFFSET).unwrap();
    send_tokens_table.set_property_f32(atom, Role::App, "padding_x", PADDING_X).unwrap();
    send_tokens_table.set_property_f32(atom, Role::App, "padding_y", PADDING_Y).unwrap();
    send_tokens_table.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    send_tokens_table.set_property_u32(atom, Role::App, "priority", 0).unwrap();

    let prop = send_tokens_table.get_property("text_color").unwrap();
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

    let prop = send_tokens_table.get_property("separator_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.2).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.2745).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.2784).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let send_tokens_table = send_tokens_table
        .setup(|me| TokenTable::new(me, app.renderer.clone(), app.sg_root.clone()))
        .await;
    send_step1_layer.link(send_tokens_table.clone());

    let (slot, recvr) = Slot::new("token_row_clicked");
    send_tokens_table.register("row_click", slot).unwrap();
    let sg_root = app.sg_root.clone();
    let renderer = app.renderer.clone();
    let send_tx_data2 = send_tx_data.clone();
    let step1_is_visible3 = step1_is_visible.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let mut cur = std::io::Cursor::new(data);
            if let Ok(row) = TokenRow::decode(&mut cur) {
                let mut data = send_tx_data2.lock().unwrap();
                data.token_symbol = Some(row.symbol.clone());
                data.token_id = Some(row.id);
                drop(data);

                let atom = &mut renderer.make_guard(gfxtag!("token selection"));
                if let Some(selected_token_symbol) = sg_root.lookup_node("/window/content/wallet_send_step2_layer/send_selected_token_symbol") {
                    selected_token_symbol.set_property_str(atom, Role::App, "text", &row.symbol).unwrap();
                }

                step1_is_visible3.set(atom, false);
                if let Some(step2) = sg_root.lookup_node("/window/content/wallet_send_step2_layer") {
                    step2.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                }
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let step1_is_visible3 = step1_is_visible.clone();
    let sg_root2 = app.sg_root.clone();
    let renderer2 = app.renderer.clone();
    let send_tokens_table2 = send_tokens_table.clone();
    let step1_is_visible_sub = step1_is_visible.prop().subscribe_modify();
    let listen_step1_visible = app.ex.spawn(async move {
        while let Ok(_) = step1_is_visible_sub.receive().await {
            if step1_is_visible3.get() {
                let atom = &mut renderer2.make_guard(gfxtag!("wallet - refresh send tokens"));

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

                            let _ = send_tokens_table2.call_method("set_tokens", data).await;

                            // Update main wallet balance
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
    app.tasks.lock().unwrap().push(listen_step1_visible);

    send_step1_layer
}
