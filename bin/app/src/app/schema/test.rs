/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
        node::{
            create_layer, create_singleline_edit, create_text, create_vector_art, create_video,
        },
        App,
    },
    expr,
    mesh::COLOR_PURPLE,
    prop::{PropertyAtomicGuard, PropertyFloat32, Role},
    scene::SceneNodePtr,
    ui::{BaseEdit, BaseEditType, Layer, Text, VectorArt, VectorShape, Video},
    util::i18n::I18nBabelFish,
};

const LIGHTMODE: bool = false;

#[cfg(target_os = "android")]
mod ui_consts {
    //pub const CHATDB_PATH: &str = "/data/data/darkfi.app/chatdb/";
    //pub const KING_PATH: &str = "king.png";
    pub const VID_PATH: &str = "forest_720x1280.ivf";
}

#[cfg(not(target_os = "android"))]
mod ui_consts {
    //pub const CHATDB_PATH: &str = "chatdb";
    //pub const KING_PATH: &str = "assets/king.png";
    pub const VID_PATH: &str = "assets/forest_1920x1080.ivf";
}

use ui_consts::*;

#[allow(dead_code)]
pub async fn make(app: &App, window: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let atom = &mut PropertyAtomicGuard::none();

    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/setting/scale").unwrap(),
        Role::Internal,
        "value",
        0,
    )
    .unwrap();

    // Create a layer called view
    let layer_node = create_layer("view");
    let prop = layer_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    layer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    let layer_node = layer_node.setup(|me| Layer::new(me, app.render_api.clone())).await;
    window.link(layer_node.clone());

    // Create a bg mesh
    let node = create_vector_art("bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();

    let c = if LIGHTMODE { 1. } else { 0. };
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [c, c, c, 1.],
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        COLOR_PURPLE,
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.render_api.clone())).await;
    layer_node.link(node);

    /*
    // Create button bg
    let node = create_vector_art("btnbg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - 210").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 10.).unwrap();
    prop.set_f32(atom, Role::App, 2, 200.).unwrap();
    prop.set_f32(atom, Role::App, 3, 60.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    // Setup the pimpl
    let verts = if LIGHTMODE {
        vec![
            ShapeVertex::from_xy(0., 0., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(200., 0., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(0., 60., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(200., 60., [1., 0., 0., 1.]),
        ]
    } else {
        vec![
            ShapeVertex::from_xy(0., 0., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(200., 0., [1., 0., 1., 1.]),
            ShapeVertex::from_xy(0., 60., [0., 0., 1., 1.]),
            ShapeVertex::from_xy(200., 60., [1., 1., 0., 1.]),
        ]
    };
    let indices = vec![0, 2, 1, 1, 2, 3];
    let shape = VectorShape { verts, indices };
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Create the button
    let node = create_button("btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - 220").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 10.).unwrap();
    prop.set_f32(atom, Role::App, 2, 200.).unwrap();
    prop.set_f32(atom, Role::App, 3, 60.).unwrap();

    //let (sender, btn_click_recvr) = async_channel::unbounded();
    //let slot_click = Slot { name: "button_clicked".to_string(), notify: sender };
    //node.register("click", slot_click).unwrap();

    let node = node.setup(|me| Button::new(me, app.ex.clone())).await;
    layer_node.link(node);

    // Create another mesh
    let node = create_vector_art("box");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 10.).unwrap();
    prop.set_f32(atom, Role::App, 1, 10.).unwrap();
    prop.set_f32(atom, Role::App, 2, 60.).unwrap();
    prop.set_f32(atom, Role::App, 3, 60.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    // Setup the pimpl
    let verts = if LIGHTMODE {
        vec![
            ShapeVertex::from_xy(0., 0., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(60., 0., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(0., 60., [0., 0., 0., 1.]),
            ShapeVertex::from_xy(60., 60., [1., 0., 0., 1.]),
        ]
    } else {
        vec![
            ShapeVertex::from_xy(0., 0., [1., 0., 0., 1.]),
            ShapeVertex::from_xy(60., 0., [1., 0., 1., 1.]),
            ShapeVertex::from_xy(0., 60., [0., 0., 1., 1.]),
            ShapeVertex::from_xy(60., 60., [1., 1., 0., 1.]),
        ]
    };
    let indices = vec![0, 2, 1, 1, 2, 3];
    let shape = VectorShape { verts, indices };
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Debugging tool
    let node = create_vector_art("debugtool");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile("h/2").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h/2 - 200").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(5.),
        [0., 1., 0., 1.],
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        cc.compile("h - 5").unwrap(),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 1., 0., 1.],
    );
    let node =
        node.setup(|me| VectorArt::new(me, shape, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Create KING GNU!
    let node = create_image("king");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 80.).unwrap();
    prop.set_f32(atom, Role::App, 1, 10.).unwrap();
    prop.set_f32(atom, Role::App, 2, 60.).unwrap();
    prop.set_f32(atom, Role::App, 3, 60.).unwrap();
    node.set_property_str(atom, Role::App, "path", KING_PATH).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let node = node.setup(|me| Image::new(me, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);
    */

    // Create KING GNU!
    let node = create_video("king");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 80.).unwrap();
    prop.set_f32(atom, Role::App, 1, 400.).unwrap();
    prop.set_f32(atom, Role::App, 2, 600.).unwrap();
    prop.set_f32(atom, Role::App, 3, 600.).unwrap();
    node.set_property_str(atom, Role::App, "path", VID_PATH).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let node = node.setup(|me| Video::new(me, app.render_api.clone(), app.ex.clone())).await;
    layer_node.link(node);

    // Create some text
    let node = create_text("label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 100.).unwrap();
    prop.set_f32(atom, Role::App, 1, 100.).unwrap();
    prop.set_f32(atom, Role::App, 2, 2000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", 60.).unwrap();
    node.set_property_str(
        atom,
        Role::App,
        "text",
        // Monero custom icon
        //"\u{f0007}",
        "yoyo 🚀 'پل خواجو' he", //"hel \u{01f3f3}\u{fe0f}\u{200d}\u{26a7}\u{fe0f} 123 '\u{01f44d}\u{01f3fe}' br",
    )
    .unwrap();
    //node.set_property_str(atom, Role::App, "text", "anon1").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let node = node
        .setup(|me| Text::new(me, window_scale.clone(), app.render_api.clone(), i18n_fish.clone()))
        .await;
    layer_node.link(node);

    /*
    // ChatView
    let node = create_chatview("chatty");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 10.).unwrap();
    let code = cc.compile("h/2").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    let code = cc.compile("h/2 - 200").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", 20.).unwrap();
    node.set_property_f32(atom, Role::App, "timestamp_font_size", 10.).unwrap();
    node.set_property_f32(atom, Role::App, "timestamp_width", 80.).unwrap();
    node.set_property_f32(atom, Role::App, "line_height", 30.).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", 20.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    //node.set_property_bool(atom, Role::App, "debug", true).unwrap();

    let prop = node.get_property("timestamp_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.5).unwrap();
    let prop = node.get_property("text_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }

    let prop = node.get_property("nick_colors").unwrap();
    #[rustfmt::skip]
    let nick_colors = [
        0.00, 0.94, 1.00, 1.,
        0.36, 1.00, 0.69, 1.,
        0.29, 1.00, 0.45, 1.,
        0.00, 0.73, 0.38, 1.,
        0.21, 0.67, 0.67, 1.,
        0.56, 0.61, 1.00, 1.,
        0.84, 0.48, 1.00, 1.,
        1.00, 0.61, 0.94, 1.,
        1.00, 0.36, 0.48, 1.,
        1.00, 0.30, 0.00, 1.
    ];
    for c in nick_colors {
        prop.push_f32(Role::App, c).unwrap();
    }

    let prop = node.get_property("hi_bg_color").unwrap();
    if LIGHTMODE {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }

    let db = sled::open(CHATDB_PATH).expect("cannot open sleddb");
    let chat_tree = db.open_tree(b"chat").unwrap();
    if chat_tree.is_empty() {
        populate_tree(&chat_tree);
    }
    debug!(target: "app", "db has {} lines", chat_tree.len());
    let node = node
        .setup(|me| {
            ChatView::new(
                me,
                chat_tree,
                window_scale.clone(),
                app.render_api.clone(),
                app.text_shaper.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.link(node);
    */

    // Text edit
    let node = create_singleline_edit("editz");
    //let node = create_multiline_edit("editz");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_bool(atom, Role::App, "is_focused", true).unwrap();

    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 100.).unwrap();
    prop.set_f32(atom, Role::App, 1, 300.).unwrap();

    //prop.set_expr(atom, Role::App, 2, expr::load_var("parent_w")).unwrap();

    // Only used in singleline edit
    prop.set_f32(atom, Role::App, 2, 600.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();

    node.set_property_bool(atom, Role::App, "debug", true).unwrap();

    node.set_property_f32(atom, Role::App, "baseline", 34.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", 40.).unwrap();

    //////////////////////////////////////////////////////////////////////
    // Only for multiline edit
    //////////////////////////////////////////////////////////////////////

    //let prop = node.get_property("height_range").unwrap();
    //prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    //prop.set_f32(atom, Role::App, 1, 400.).unwrap();

    let prop = node.get_property("padding").unwrap();
    prop.set_f32(atom, Role::App, 0, 20.).unwrap();
    prop.set_f32(atom, Role::App, 1, 20.).unwrap();
    prop.set_f32(atom, Role::App, 2, 20.).unwrap();
    prop.set_f32(atom, Role::App, 3, 20.).unwrap();

    #[cfg(target_os = "android")]
    {
        //let prop = node.get_property("padding").unwrap();
        //prop.set_f32(atom, Role::App, 0, 80.).unwrap();
        //prop.set_f32(atom, Role::App, 1, 80.).unwrap();
        //prop.set_f32(atom, Role::App, 2, 80.).unwrap();
        //prop.set_f32(atom, Role::App, 3, 80.).unwrap();

        node.set_property_f32(atom, Role::App, "font_size", 60.).unwrap();
    }

    //////////////////////////////////////////////////////////////////////

    //node.set_property_str(atom, Role::App, "text", "hello king!😁🍆jelly 🍆1234").unwrap();
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
    let prop = node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_f32(atom, Role::App, "cursor_ascent", 35.).unwrap();
    node.set_property_f32(atom, Role::App, "cursor_descent", 20.).unwrap();
    node.set_property_f32(atom, Role::App, "select_ascent", 35.).unwrap();
    node.set_property_f32(atom, Role::App, "select_descent", 20.).unwrap();
    node.set_property_f32(atom, Role::App, "handle_descent", 25.).unwrap();
    let prop = node.get_property("hi_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let node = node
        .setup(|me| {
            BaseEdit::new(
                me,
                window_scale.clone(),
                app.render_api.clone(),
                BaseEditType::SingleLine,
                //BaseEditType::MultiLine,
            )
        })
        .await;
    layer_node.link(node.clone());

    // android: show on screen keyboard
    let focus_task = app.ex.spawn(async move {
        darkfi::system::sleep(1).await;
        node.call_method("focus", vec![]).await.unwrap();
    });
    app.tasks.lock().unwrap().push(focus_task);
}
