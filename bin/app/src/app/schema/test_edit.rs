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

#![allow(unused_imports, unused_variables, dead_code)]

use crate::{
    app::{
        node::{create_layer, create_multiline_edit, create_text, create_vector_art, create_video},
        App,
    },
    expr::{self, Compiler},
    mesh::COLOR_PURPLE,
    prop::{PropertyAtomicGuard, PropertyFloat32, PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
    ui::{BaseEdit, BaseEditType, Layer, Text, VectorArt, VectorShape, Video},
    util::i18n::I18nBabelFish,
};

#[allow(dead_code)]
pub async fn make(app: &App, window: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let atom = &mut PropertyAtomicGuard::none();

    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/window").unwrap(),
        Role::Internal,
        "scale",
        0,
    )
    .unwrap();

    let renderer = &app.renderer;
    let ex = &app.ex;
    let cc = Compiler::new();

    // Create a layer called view
    let layer_node = create_layer("view");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    let layer_node = layer_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    window.link(layer_node.clone());

    // Create a bg mesh
    let node = create_vector_art("bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 0., 0., 1.],
    );
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    layer_node.link(node);

    // Text edit
    let node = create_multiline_edit("editz");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", false).unwrap();

    let prop = node.get_property("height_range").unwrap();
    prop.set_f32(atom, Role::App, 0, 160.).unwrap();
    prop.set_f32(atom, Role::App, 1, 500.).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 100.).unwrap();
    prop.set_f32(atom, Role::App, 1, 100.).unwrap();
    let code = cc.compile("parent_w - 100").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, 140.).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, 40. * 0.4).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 40. / 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.).unwrap();

    node.set_property_f32(atom, Role::App, "baseline", 40.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", 50.).unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("text_hi_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.44).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.96).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("text_cmd_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.64).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.83).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_f32(atom, Role::App, "cursor_ascent", 50.).unwrap();
    node.set_property_f32(atom, Role::App, "cursor_descent", 20.).unwrap();
    node.set_property_f32(atom, Role::App, "select_ascent", 50.).unwrap();
    node.set_property_f32(atom, Role::App, "select_descent", 20.).unwrap();
    node.set_property_f32(atom, Role::App, "handle_descent", 10.).unwrap();
    node.set_property_f32(atom, Role::App, "action_padding", 32.).unwrap();
    node.set_property_f32(atom, Role::App, "action_spacing", 8.).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.27).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.22).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = node.get_property("cmd_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.30).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.25).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 6).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 3).unwrap();

    let editz_text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
    let editz_select_text = node.get_property("select_text").unwrap();

    let node = node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                renderer.clone(),
                app.redraw_trigger.clone(),
                BaseEditType::MultiLine,
                ex.clone(),
            )
        })
        .await;
    let chatedit_node = node.clone();
    layer_node.link(node);

    let (slot, recvr) = Slot::new("focus_request");
    chatedit_node.register("focus_request", slot).unwrap();
    let focus_task = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            chatedit_node.call_method("focus", vec![]).await.unwrap();
        }
    });
    app.tasks.lock().unwrap().push(focus_task);

    /*
    #[cfg(target_os = "android")]
    {
        use crate::android::textinput::{AndroidTextInput, AndroidTextInputState};
        use darkfi::system::msleep;

        let (sender, recvr) = async_channel::unbounded::<AndroidTextInputState>();
        let input = AndroidTextInput::new(sender);

        let event_task = ex.spawn(async move {
            loop {
                match recvr.recv().await {
                    Ok(state) => info!(target: "test_edit", "IME event: {state:?}"),
                    Err(_) => break,
                }
            }
        });
        app.tasks.lock().unwrap().push(event_task);

        let test_task = ex.spawn(async move {
            msleep(3000).await;

            info!(target: "test_edit", "=== STEP 1: show() ===");
            input.show();
            msleep(5000).await;

            info!(target: "test_edit", "=== STEP 2: set_state(hello world) ===");
            input.set_state(AndroidTextInputState {
                text: "hello world".to_string(),
                select: (11, 11),
                compose: None,
            });
            msleep(5000).await;

            info!(target: "test_edit", "=== STEP 3: set_select(1, 1) — cursor to start ===");
            input.set_select(1, 1);
            msleep(5000).await;

            /*
            info!(target: "test_edit", "=== STEP 4: set_select(6, 6) — cursor into 'world' ===");
            input.set_select(6, 6);
            msleep(5000).await;

            info!(target: "test_edit", "=== STEP 5: set_select(0, 5) — select 'hello' ===");
            input.set_select(0, 5);
            msleep(5000).await;

            info!(target: "test_edit", "=== STEP 6: hide() ===");
            input.hide();
            msleep(3000).await;

            info!(target: "test_edit", "=== STEP 7: show() again ===");
            input.show();
            msleep(5000).await;
            */

            info!(target: "test_edit", "=== DONE ===");
        });
        app.tasks.lock().unwrap().push(test_task);
    }
    */
}
