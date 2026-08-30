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
        node::create_text,
        schema::{
            create_layer, create_vector_art,
            wallet::data::{
                NETLOGO_SCALE, NETSTATUS_ICON_SIZE, PROGRESS_FONTSIZE, PROGRESS_MARGIN,
            },
        },
        App,
    },
    expr,
    prop::{PropertyAtomicGuard, PropertyFloat32, Role},
    scene::SceneNodePtr,
    shape,
    ui::{Layer, Text, VectorArt},
    util::i18n::I18nBabelFish,
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
    cc.add_const_f32("PROGRESS_MARGIN", PROGRESS_MARGIN);

    let netlayer_node = create_layer("netstatus_layer");
    let prop = netlayer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    let code = cc.compile("w").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    netlayer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    netlayer_node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let netlayer_node = netlayer_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    wallet_layer.link(netlayer_node.clone());

    // Scan progress text
    let node = create_text("progress");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2. - PROGRESS_FONTSIZE / 2.).unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE - PROGRESS_MARGIN").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, PROGRESS_FONTSIZE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", PROGRESS_FONTSIZE).unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "end").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let node = node
        .setup(|me| {
            Text::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                i18n_fish.clone(),
                app.redraw_trigger.clone(),
            )
        })
        .await;
    netlayer_node.link(node);

    let node = create_vector_art("net0");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([1., 0.15, 0.30, 1.]);
    shape.join(shape::create_blockchain_netlogo2([0.27, 0.4, 0.4, 1.]));
    shape.join(shape::create_blockchain_netlogo3([0.27, 0.4, 0.4, 1.]));
    shape.join(shape::create_blockchain_netlogo4([0.27, 0.4, 0.4, 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net0_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(net0_node);

    let node = create_vector_art("net1");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([0.73, 0.62, 1., 1.]);
    shape.join(shape::create_blockchain_netlogo2([0.73, 0.62, 1., 1.]));
    shape.join(shape::create_blockchain_netlogo3([0.27, 0.4, 0.4, 1.]));
    shape.join(shape::create_blockchain_netlogo4([0.27, 0.4, 0.4, 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net1_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(net1_node);

    let node = create_vector_art("net2");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([0.49, 0.57, 1., 1.]);
    shape.join(shape::create_blockchain_netlogo2([0.49, 0.57, 1., 1.]));
    shape.join(shape::create_blockchain_netlogo3([0.49, 0.57, 1., 1.]));
    shape.join(shape::create_blockchain_netlogo4([0.27, 0.4, 0.4, 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net2_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(net2_node);

    let node = create_vector_art("net3");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_blockchain_netlogo1([0., 0.94, 1., 1.]);
    shape.join(shape::create_blockchain_netlogo2([0., 0.94, 1., 1.]));
    shape.join(shape::create_blockchain_netlogo3([0., 0.94, 1., 1.]));
    shape.join(shape::create_blockchain_netlogo4([0., 0.94, 1., 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net3_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(net3_node);

    netlayer_node
}
