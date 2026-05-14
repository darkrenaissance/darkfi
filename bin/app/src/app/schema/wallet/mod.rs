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

macro_rules! d { ($($arg:tt)*) => { debug!(target: "app::schema::wallet", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "app::schema::wallet", $($arg)*); } }

pub mod data;
pub mod util;
pub mod main;
pub mod receive;
pub mod send;
pub mod send_step1;
pub mod send_step2;
pub mod send_step3;
pub mod send_step4;
pub mod tx_status;

use crate::{
    app::App,
    gfx::gfxtag,
    prop::{PropertyAtomicGuard, PropertyFloat32, Role},
    scene::SceneNodePtr,
    util::i18n::I18nBabelFish,
};

use main::make_main_wallet_layer;
use receive::make_receive_layer;
use send::make_send_layer;

pub async fn make(app: &App, content: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/window").unwrap(),
        Role::Internal,
        "scale",
        0,
    )
    .unwrap();

    // Create main wallet layer
    let wallet_layer = make_main_wallet_layer(app, content.clone(), i18n_fish, window_scale.clone()).await;

    // Create receive layer
    let _ = make_receive_layer(
        app,
        content.clone(),
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
    ).await;

    // Create send layer
    let _ = make_send_layer(
        app,
        content.clone(),
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
    ).await;
}
