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

use darkfi::util::parse::encode_base10;
use darkfi::tx::Transaction;
use darkfi_serial::Decodable;

use crate::{
    app::App,
    gfx::gfxtag,
    mesh::COLOR_CYAN,
    prop::{PropertyBool, PropertyFloat32, Role},
    scene::SceneNodePtr,
    util::i18n::I18nBabelFish,
};

use super::{data::SendTxData, data::BALANCE_BASE10_DECIMALS, send_step1, send_step2, send_step3, send_step4, tx_status};

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let send_tx_data = std::sync::Arc::new(std::sync::Mutex::new(SendTxData::new()));

    let send_step1_layer = send_step1::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        send_tx_data.clone(),
    ).await;

    let step1_is_visible = PropertyBool::wrap(&send_step1_layer, Role::App, "is_visible", 0).unwrap();

    let send_step2_layer = send_step2::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        send_tx_data.clone(),
        step1_is_visible.clone(),
    ).await;

    let step2_is_visible = PropertyBool::wrap(&send_step2_layer, Role::App, "is_visible", 0).unwrap();

    let send_step3_layer = send_step3::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        send_tx_data.clone(),
        step1_is_visible.clone(),
        step2_is_visible.clone(),
    ).await;

    let step3_is_visible = PropertyBool::wrap(&send_step3_layer, Role::App, "is_visible", 0).unwrap();

    let send_step4_layer = send_step4::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        send_tx_data.clone(),
        step2_is_visible.clone(),
        step3_is_visible.clone(),
        step1_is_visible.clone(),
    ).await;

    let step4_is_visible = PropertyBool::wrap(&send_step4_layer, Role::App, "is_visible", 0).unwrap();

    let tx_status_layer = tx_status::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
        send_tx_data,
        step3_is_visible,
        step4_is_visible,
    ).await;

    // Add listener for tx built signal to update send button label and show fee
    let set_built_tx_sub = tx_status_layer.subscribe_method_call("set_built_tx").unwrap();
    let renderer_for_built = app.renderer.clone();
    let sg_root_for_built = app.sg_root.clone();
    app.tasks.lock().unwrap().push(app.ex.spawn(async move {
        while let Ok(mcall) = set_built_tx_sub.receive().await {
            let mut cur = std::io::Cursor::new(mcall.data);
            let tx = Transaction::decode(&mut cur).unwrap();
            let mut fees: u64 = 0;
            for (i, call) in tx.calls.iter().enumerate() {
                if call.data.is_money_fee() {
                    if let Ok(fee) = darkfi_serial::deserialize(&call.data.data[1..9]) {
                        fees = fees.saturating_add(fee);
                    }
                }
            }

            let atom = &mut renderer_for_built.make_guard(gfxtag!("tx built - update send button"));

            // Make send button active
            if let Some(send_label_node) = sg_root_for_built.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_label") {
                send_label_node.set_property_str(atom, Role::App, "text", "send").unwrap();
                let prop = send_label_node.get_property("text_color").unwrap();
                prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
                prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
                prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
                prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
            }
            if let Some(send_bg_grey_node) = sg_root_for_built.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_bg_grey") {
                send_bg_grey_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
            }
            if let Some(send_bg_node) = sg_root_for_built.lookup_node("/window/content/wallet/send_step4_layer/send_send_btn_bg") {
                send_bg_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
            }

            // Show transaction fee
            if let Some(tx_fee_label) = sg_root_for_built.lookup_node("/window/content/wallet/send_step4_layer/send_fee_label") {
                let prop = tx_fee_label.get_property("text_color").unwrap();
                prop.set_f32(atom, Role::App, 0, 1.).unwrap();
                prop.set_f32(atom, Role::App, 1, 1.).unwrap();
                prop.set_f32(atom, Role::App, 2, 1.).unwrap();
                prop.set_f32(atom, Role::App, 3, 1.).unwrap();
            }
            if let Some(tx_fee_value) = sg_root_for_built.lookup_node("/window/content/wallet/send_step4_layer/send_fee_value") {
                tx_fee_value.set_property_str(atom, Role::App, "text", encode_base10(fees, BALANCE_BASE10_DECIMALS)).unwrap();
                let prop = tx_fee_value.get_property("text_color").unwrap();
                prop.set_f32(atom, Role::App, 0, 1.).unwrap();
                prop.set_f32(atom, Role::App, 1, 1.).unwrap();
                prop.set_f32(atom, Role::App, 2, 1.).unwrap();
                prop.set_f32(atom, Role::App, 3, 1.).unwrap();
            }
        }
    }));

    // Setup step2 visibility listener for auto-focusing recipient input
    let step2_is_visible_for_focus = step2_is_visible.clone();
    let sg_root_for_focus = app.sg_root.clone();
    let step2_is_visible_sub = step2_is_visible.prop().subscribe_modify();
    let listen_step2_visible = app.ex.spawn(async move {
        while let Ok(_) = step2_is_visible_sub.receive().await {
            let recipient_input_node = sg_root_for_focus.lookup_node("/window/content/wallet/send_step2_layer/send_recipient_input").unwrap();
            if step2_is_visible_for_focus.get() {
                // Focus when becoming visible
                loop {
                    darkfi::system::msleep(16).await;
                    let input_rect = crate::prop::PropertyRect::wrap(&recipient_input_node, Role::App, "rect").unwrap();
                    if input_rect.has_cached() {
                        break;
                    }
                }
                recipient_input_node.call_method("focus", vec![]).await.unwrap();
            } else {
                // Unfocus when becoming hidden
                recipient_input_node.call_method("unfocus", vec![]).await.unwrap();
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_step2_visible);

    send_step1_layer
}
