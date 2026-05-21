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
        schema::{create_vector_art, create_layer},
        schema::wallet::data::{NETSTATUS_ICON_SIZE, NETLOGO_SCALE}
    },
    expr,
    prop::{PropertyAtomicGuard, PropertyFloat32, Role},
    shape,
    scene::SceneNodePtr,
    ui::{Layer, VectorArt},
    util::i18n::I18nBabelFish
};

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("NETSTATUS_ICON_SIZE", NETSTATUS_ICON_SIZE);

    let netlayer_node = create_layer("netstatus_layer");
    let prop = netlayer_node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    netlayer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    netlayer_node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let netlayer_node = netlayer_node.setup(|me| Layer::new(me, app.renderer.clone())).await;
    wallet_layer.link(netlayer_node.clone());

    let node = create_vector_art("net0");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([1., 0., 0.25, 1.]);
    shape.join(shape::create_blockchain_netlogo2([0.27, 0.4, 0.4, 1.]));
    let net0_node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    netlayer_node.link(net0_node);

    let node = create_vector_art("net1");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([1., 0.6, 0., 1.]);
    shape.join(shape::create_blockchain_netlogo2([0.27, 0.4, 0.4, 1.]));
    let net1_node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    netlayer_node.link(net1_node);

    let node = create_vector_art("net2");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([0.49, 0.57, 1., 1.]);
    shape.join(shape::create_blockchain_netlogo2([0.49, 0.57, 1., 1.]));
    let net2_node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    netlayer_node.link(net2_node);

    netlayer_node
}
