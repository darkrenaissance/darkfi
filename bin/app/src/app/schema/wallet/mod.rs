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
pub mod netstatus;

use crate::{
    app::{App, node::create_layer},
    expr,
    gfx::gfxtag,
    prop::{PropertyAtomicGuard, PropertyFloat32, Role},
    scene::SceneNodePtr,
    ui::Layer,
    util::i18n::I18nBabelFish,
};

pub async fn make(app: &App, content: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/window").unwrap(),
        Role::Internal,
        "scale",
        0,
    )
    .unwrap();

    let atom = &mut PropertyAtomicGuard::none();
    let wallet_layer = create_layer("wallet");
    let prop = wallet_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    wallet_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    wallet_layer.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let wallet_layer = wallet_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(wallet_layer.clone());

    // Create main wallet layer
    let _ = main::make(app, wallet_layer.clone(), i18n_fish, window_scale.clone()).await;

    // Create blockchain network status indicator layer
    let _ = netstatus::make(app, wallet_layer.clone(), i18n_fish, window_scale.clone()).await;

    // Create receive layer
    let _ = receive::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
    ).await;

    // Create send layer
    let _ = send::make(
        app,
        wallet_layer.clone(),
        i18n_fish,
        window_scale.clone(),
    ).await;
}
