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

use darkfi::system::msleep;
use super::{ColorScheme, COLOR_SCHEME};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use crate::{
    app::{
        App, node::{
            create_button, create_layer, create_menu, create_singleline_edit, create_text,
            create_vector_art,
        }
    },
    expr,
    gfx::gfxtag,
    mesh::{COLOR_CYAN, COLOR_TEAL},
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr, Role},
    scene::{Pimpl, SceneNode, SceneNodePtr, SceneNodeType, Slot},
    shape,
    ui::{BaseEdit, BaseEditType, Button, Layer, Menu, ShapeVertex, Shortcut, Text, VectorArt, VectorShape},
    util::i18n::I18nBabelFish,
};

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const BACKARROW_SCALE: f32 = 30.;
    pub const BACKARROW_X: f32 = 50.;
    pub const BACKARROW_Y: f32 = 70.;
    pub const TITLE_FONTSIZE: f32 = 56.;
    pub const BUTTON_FONTSIZE: f32 = 48.;
    pub const BASE_FONTSIZE: f32 = 48.;
    pub const HINT_FONTSIZE: f32 = BASE_FONTSIZE * 0.8;
    pub const AMOUNT_FONTSIZE: f32 = 118.;
    pub const AMOUNT_CHAR_WIDTH: f32 = AMOUNT_FONTSIZE * 0.6;
    pub const AMOUNT_TOKEN_SPACING: f32 = AMOUNT_FONTSIZE * 0.35;
    pub const BUTTON_HEIGHT: f32 = 200.;
    pub const TOKEN_ROW_HEIGHT: f32 = 80.;
    pub const TITLE_PADDING: f32 = 50.;
    pub const TOKEN_NAME_OFFSET: f32 = 200.;
    pub const PADDING_X: f32 = 40.;
    pub const PADDING_Y: f32 = 30.;
    pub const RECIPIENT_INPUT_MARGIN: f32 = 30.;
    pub const RECIPIENT_INPUT_PADDING_X: f32 = 40.;
    pub const RECIPIENT_INPUT_HEIGHT: f32 = 120.;
    pub const RECIPIENT_INPUT_FONTSIZE: f32 = 48.;
    pub const HEADER_HEIGHT: f32 = 140.;
    pub const ROW_HEIGHT: f32 = 80.;
    pub const WALLET_BTN_SIZE: f32 = 50.;
    pub const COPY_WIDTH: f32 = 200.;
    pub const COPY_SCALE: f32 = 30.;
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
    pub const BACKARROW_SCALE: f32 = 15.;
    pub const BACKARROW_X: f32 = 38.;
    pub const BACKARROW_Y: f32 = 26.;
    pub const TITLE_FONTSIZE: f32 = 20.;
    pub const BUTTON_FONTSIZE: f32 = 20.;
    pub const BASE_FONTSIZE: f32 = 20.;
    pub const HINT_FONTSIZE: f32 = BASE_FONTSIZE * 0.8;
    pub const AMOUNT_FONTSIZE: f32 = 56.;
    pub const AMOUNT_CHAR_WIDTH: f32 = AMOUNT_FONTSIZE * 0.6;
    pub const AMOUNT_TOKEN_SPACING: f32 = AMOUNT_FONTSIZE * 0.35;
    pub const BUTTON_HEIGHT: f32 = 90.;
    pub const TOKEN_ROW_HEIGHT: f32 = 40.;
    pub const TITLE_PADDING: f32 = 25.;
    pub const TOKEN_NAME_OFFSET: f32 = 90.;
    pub const PADDING_X: f32 = 20.;
    pub const PADDING_Y: f32 = 15.;
    pub const RECIPIENT_INPUT_MARGIN: f32 = 15.;
    pub const RECIPIENT_INPUT_PADDING_X: f32 = 20.;
    pub const RECIPIENT_INPUT_HEIGHT: f32 = 60.;
    pub const RECIPIENT_INPUT_FONTSIZE: f32 = 20.;
    pub const HEADER_HEIGHT: f32 = 60.;
    pub const ROW_HEIGHT: f32 = 80.;
    pub const WALLET_BTN_SIZE: f32 = 50.;
    pub const COPY_WIDTH: f32 = 100.;
    pub const COPY_SCALE: f32 = 15.;
}

use ui_consts::*;

const MOCK_TOKENS: &[(&str, &str, f32)] = &[
    ("DRK", "Dark", 100.24),
    ("wXMR", "wrapped Monero", 5.56487),
    ("wBTC", "wrapped Bitcoin", 0.78956413),
    ("RNDM", "Random", 0.78956413),
    ("OTHER", "Other token", 0.78956413),
    ("OTHER", "Other token", 0.78956413),
    ("OTHER", "Other token", 0.78956413),
    ("OTHER", "Other token", 0.78956413),
];

const MOCK_RECEIVE_ADDRESS: &str = "8siycy4q5rsf3rwxa9ptw5v7syfj1m1ov27tt78rj7kprth1ux41lqpvut6cht7w";
const MOCK_TX_FEE: &str = "0.001 DRK";

fn is_valid_address(address: &str) -> bool {
    address.len() > 3 // TODO
}

fn get_balance(token_symbol: &str) -> f32 {
    MOCK_TOKENS
        .iter()
        .find(|(symbol, _, _)| *symbol == token_symbol)
        .map(|(_, _, balance)| *balance)
        .unwrap_or(0.0)
}

// Send transaction data shared across all send pages
#[derive(Debug, Clone)]
struct SendTxData {
    token_symbol: Option<String>,
    token_name: Option<String>,
    recipient: Option<String>,
    amount: Option<String>,
}

impl SendTxData {
    fn new() -> Self {
        Self {
            token_symbol: None,
            token_name: None,
            recipient: None,
            amount: None,
        }
    }
}

pub async fn make(app: &App, content: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let window_scale = PropertyFloat32::wrap(
        &app.sg_root.lookup_node("/window").unwrap(),
        Role::Internal,
        "scale",
        0,
    )
    .unwrap();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("HEADER_HEIGHT", HEADER_HEIGHT);
    cc.add_const_f32("TOKEN_ROW_HEIGHT", TOKEN_ROW_HEIGHT);
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("PADDING_Y", PADDING_Y);
    cc.add_const_f32("TITLE_FONTSIZE", TITLE_FONTSIZE);
    cc.add_const_f32("BUTTON_HEIGHT", BUTTON_HEIGHT);
    cc.add_const_f32("BUTTON_FONTSIZE", BUTTON_FONTSIZE);
    cc.add_const_f32("ROW_HEIGHT", ROW_HEIGHT);

    let atom = &mut PropertyAtomicGuard::none();

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
    node.set_property_str(atom, Role::App, "text", "DRK 100.24").unwrap();
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

    create_separator(app, atom, &wallet_layer, "wallet_balance_separator", &mut y).await;

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

    let receive_layer = wallet_receive_layer(app, content.clone(), wallet_layer.clone(), i18n_fish, window_scale.clone()).await;
    let receive_is_visible =
        PropertyBool::wrap(&receive_layer, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let wallet_is_visible2 = wallet_is_visible.clone();
    let (slot, recvr) = Slot::new("receive_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("receive button click"));
            wallet_is_visible2.set(atom, false);
            receive_is_visible.set(atom, true);
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

    let send_layer = wallet_send_layer(app, content.clone(), wallet_layer.clone(), i18n_fish, window_scale.clone()).await;
    let send_is_visible = PropertyBool::wrap(&send_layer, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let wallet_is_visible3 = wallet_is_visible.clone();
    let (slot, recvr) = Slot::new("send_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send button click"));
            wallet_is_visible3.set(atom, false);
            send_is_visible.set(atom, true);
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

    create_separator(app, atom, &wallet_layer, "wallet_buttons_separator", &mut y).await;

    create_title(app, atom, &wallet_layer, &window_scale, i18n_fish, "TOKENS", &mut y).await;

    create_tokens_table(app, atom, &window_scale, i18n_fish, &wallet_layer, MOCK_TOKENS, &mut y, |_, _, _, _| {}).await;
}

async fn wallet_receive_layer(
    app: &App,
    content: SceneNodePtr,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("PADDING_Y", PADDING_Y);
    cc.add_const_f32("COPY_WIDTH", COPY_WIDTH);

    // Receive layer
    let receive_layer = create_layer("wallet_receive_layer");
    let prop = receive_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    receive_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    receive_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let receive_layer = receive_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(receive_layer.clone());

    create_bg_mesh(app, atom, &receive_layer, "receive_bg").await;
    create_header_bg(app, atom, &receive_layer, "receive_header_bg").await;

    // Back button
    let node = create_vector_art("receive_back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    receive_layer.link(node);

    let mut y = 0.;

    let node = create_button("receive_back_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let wallet_is_visible = PropertyBool::wrap(&wallet_layer, Role::App, "is_visible", 0).unwrap();
    let receive_is_visible = PropertyBool::wrap(&receive_layer, Role::App, "is_visible", 0).unwrap();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("receive_back_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("receive back button"));
            receive_is_visible.set(atom, false);
            wallet_is_visible.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    receive_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &receive_layer, &window_scale, i18n_fish, "RECEIVE", &mut y).await;

    create_subtitle(app, atom, &receive_layer, &window_scale, i18n_fish, "Address", &mut y).await;

    // Address display
    let node = create_text("receive_address");
    let addr_h_prop = node.get_property("height").unwrap();
    let receive_address_text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - COPY_WIDTH").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE * 2.).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", MOCK_RECEIVE_ADDRESS).unwrap();
    node.set_property_enum(atom, Role::App, "overflow_wrap", "anywhere").unwrap();
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
    receive_layer.link(node);

    // Copy button
    let node = create_vector_art("receive_copy_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - COPY_WIDTH / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile(format!("{y} + (PADDING_Y * 2. + addr_height) / 2")).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_copy(COLOR_CYAN).scaled(COPY_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    receive_layer.link(node);

    let node = create_button("receive_copy_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    let code = cc.compile("w").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    let code = cc.compile("PADDING_Y * 2 + addr_height").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();

    let (slot, recvr) = Slot::new("receive_copy_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let addr = receive_address_text.get();
            miniquad::window::clipboard_set(&addr);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    receive_layer.link(node);

    let sep = create_separator_expr(app, atom, &receive_layer, "receive_address_separator", &mut cc, &format!("{y} + PADDING_Y * 2 + addr_height + 1")).await;
    let prop = sep.get_property("rect").unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");

    receive_layer
}

async fn wallet_send_layer(
    app: &App,
    content: SceneNodePtr,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let mut cc = expr::Compiler::new();
    cc.add_const_f32("HEADER_HEIGHT", HEADER_HEIGHT);
    cc.add_const_f32("BUTTON_HEIGHT", BUTTON_HEIGHT);
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("PADDING_Y", PADDING_Y);

    let send_tx_data: Arc<Mutex<SendTxData>> = Arc::new(Mutex::new(SendTxData::new()));
    let send_step1_layer =
        wallet_send_layers(app, content.clone(), wallet_layer.clone(), i18n_fish, window_scale.clone(), send_tx_data).await;

    send_step1_layer
}

/// Update positions for amount input wrapper and token symbol to center them together.
fn update_amount_screen(
    atom: &mut PropertyAtomicGuard,
    amount_text: &str,
    token_symbol: &str,
    amount_wrapper_node: &SceneNodePtr,
    amount_input_node: &SceneNodePtr,
    token_node: &SceneNodePtr,
    available_balance_node: Option<&SceneNodePtr>,
) {
    let mut cc = expr::Compiler::new();

    let display_text = if amount_text.is_empty() { "0" } else { amount_text };
    let char_count = display_text.chars().count() as f32;
    let token_char_count = token_symbol.chars().count() as f32;

    let text_width = char_count * AMOUNT_CHAR_WIDTH + 6.;
    let token_width = token_char_count * AMOUNT_CHAR_WIDTH;
    let total_width = text_width + AMOUNT_TOKEN_SPACING + token_width;
    cc.add_const_f32("AMOUNT_TOTAL_WIDTH", total_width);
    cc.add_const_f32("AMOUNT_WIDTH", text_width);
    cc.add_const_f32("AMOUNT_TOKEN_SPACING", AMOUNT_TOKEN_SPACING);
    cc.add_const_f32("AMOUNT_FONTSIZE", AMOUNT_FONTSIZE);

    let wrapper_rect = amount_wrapper_node.get_property("rect").unwrap();
    wrapper_rect.set_expr(atom, Role::App, 0, cc.compile("(w - AMOUNT_TOTAL_WIDTH) / 2").unwrap()).unwrap();

    let width_code = cc.compile("AMOUNT_WIDTH").unwrap();
    let amount_rect = amount_input_node.get_property("rect").unwrap();
    amount_rect.set_expr(atom, Role::App, 2, width_code).unwrap();

    // Reset scroll to prevent content from being cropped
    if let Pimpl::Edit(edit) = amount_input_node.pimpl() {
        edit.reset_scroll();
    }

    // Update token symbol rect (x position)
    let token_rect = token_node.get_property("rect").unwrap();
    token_rect.set_expr(atom, Role::App, 0, cc.compile("(w - AMOUNT_TOTAL_WIDTH) / 2 + AMOUNT_TOKEN_SPACING + AMOUNT_WIDTH").unwrap()).unwrap();

    // Set available balance
    if let Some(available_balance_node) = available_balance_node {
        let available_balance = get_balance(token_symbol);
        available_balance_node.set_property_str(atom, Role::App, "text", format!("{available_balance} available")).unwrap();
    }
}

async fn wallet_send_layers(
    app: &App,
    content: SceneNodePtr,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
    send_tx_data: Arc<Mutex<SendTxData>>,
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

    create_subtitle(app, atom, &send_step1_layer, &window_scale, i18n_fish, "Pick a token to send", &mut y).await;

    // ============================================
    // Step 2: Recipient layer
    // ============================================
    let send_step2_layer = create_layer("wallet_send_step2_layer");
    let prop = send_step2_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step2_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step2_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step2_layer = send_step2_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(send_step2_layer.clone());
    let step2_is_visible = PropertyBool::wrap(&send_step2_layer, Role::App, "is_visible", 0).unwrap();

    // ============================================
    // Step 3: Amount layer
    // ============================================
    let send_step3_layer = create_layer("wallet_send_step3_layer");
    let prop = send_step3_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step3_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step3_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step3_layer = send_step3_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(send_step3_layer.clone());
    let step3_is_visible = PropertyBool::wrap(&send_step3_layer, Role::App, "is_visible", 0).unwrap();

    // ============================================
    // Step 4: Confirmation layer (amount as text, send button)
    // ============================================
    let send_step4_layer = create_layer("wallet_send_step4_layer");
    let prop = send_step4_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step4_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step4_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step4_layer = send_step4_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(send_step4_layer.clone());
    let step4_is_visible = PropertyBool::wrap(&send_step4_layer, Role::App, "is_visible", 0).unwrap();

    // ============================================
    // Step 5: Transaction in progress layer
    // ============================================
    let send_step5_layer = create_layer("wallet_send_step5_layer");
    let prop = send_step5_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    send_step5_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    send_step5_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let send_step5_layer = send_step5_layer.setup(|me| Layer::new(me, app.renderer.clone())).await;
    content.link(send_step5_layer.clone());
    let step5_is_visible = PropertyBool::wrap(&send_step5_layer, Role::App, "is_visible", 0).unwrap();

    // ========

    let step2_is_visible2 = step2_is_visible.clone();
    let send_tx_data2 = send_tx_data.clone();
    let renderer = app.renderer.clone();
    let step1_is_visible2 = step1_is_visible.clone();

    // Subscribe to step2 visibility changes to call focus/unfocus when layer becomes visible/hidden
    let step2_is_visible_for_focus = step2_is_visible.clone();
    let sg_root_for_focus = app.sg_root.clone();
    let renderer_for_focus = app.renderer.clone();
    let step2_is_visible_sub = step2_is_visible.prop().subscribe_modify();
    let listen_step2_visible = app.ex.spawn(async move {
        while let Ok(_) = step2_is_visible_sub.receive().await {
            let recipient_input_node = sg_root_for_focus.lookup_node("/window/content/wallet_send_step2_layer/send_recipient_input").unwrap();
            if step2_is_visible_for_focus.get() {
                // Focus when becoming visible
                loop {
                    msleep(16).await;
                    let input_rect = PropertyRect::wrap(&recipient_input_node, Role::App, "rect").unwrap();
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

    create_tokens_table(app, atom, &window_scale, i18n_fish, &send_step1_layer, MOCK_TOKENS, &mut y, move |sg_root, symbol, name, _balance| {
        {
            let mut data = send_tx_data2.lock().unwrap();
            data.token_symbol = Some(symbol.to_string());
            data.token_name = Some(name.to_string());
        }

        let mut atom = renderer.make_guard(gfxtag!("token selection"));
        let selected_token_symbol = sg_root.lookup_node("/window/content/wallet_send_step2_layer/send_selected_token_symbol").unwrap();
        selected_token_symbol.set_property_str(&mut atom, Role::App, "text", symbol).unwrap();
        let selected_token_name = sg_root.lookup_node("/window/content/wallet_send_step2_layer/send_selected_token_name").unwrap();
        selected_token_name.set_property_str(&mut atom, Role::App, "text", name).unwrap();

        step1_is_visible2.set(&mut atom, false);
        step2_is_visible2.set(&mut atom, true);
    }).await;

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

    y = 0.;

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

    let node = create_text("send_selected_token_name");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X + TOKEN_NAME_OFFSET).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "DRK").unwrap();
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
    let selected_token_text = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step2_layer.link(selected_token_text.clone());

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(app, atom, &send_step2_layer, "send_token_separator", &mut y).await;

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

    // Recipient placeholder
    let recipient_placeholder_node = create_text("send_recipient_input_placeholder");
    let prop = recipient_placeholder_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, RECIPIENT_INPUT_MARGIN + RECIPIENT_INPUT_PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + (RECIPIENT_INPUT_HEIGHT - RECIPIENT_INPUT_FONTSIZE) / 2.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 200.).unwrap();
    recipient_placeholder_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    recipient_placeholder_node.set_property_f32(atom, Role::App, "font_size", RECIPIENT_INPUT_FONTSIZE).unwrap();
    recipient_placeholder_node.set_property_str(atom, Role::App, "text", "recipient address...").unwrap();
    let prop = recipient_placeholder_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.45).unwrap();
    recipient_placeholder_node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let recipient_placeholder_node = recipient_placeholder_node
        .setup(|me| {
            Text::new(
                me,
                window_scale.clone(),
                app.renderer.clone(),
                i18n_fish.clone(),
            )
        })
        .await;
    send_step2_layer.link(recipient_placeholder_node.clone());

    // Recipient input
    let recipient_input = create_singleline_edit("send_recipient_input");
    recipient_input.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    recipient_input.set_property_bool(atom, Role::App, "is_focused", false).unwrap();
    let prop = recipient_input.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, RECIPIENT_INPUT_MARGIN + RECIPIENT_INPUT_PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    let code = cc.compile("parent_w - RECIPIENT_INPUT_MARGIN * 2 - RECIPIENT_INPUT_PADDING_X").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, RECIPIENT_INPUT_HEIGHT).unwrap();
    recipient_input.set_property_f32(atom, Role::App, "font_size", RECIPIENT_INPUT_FONTSIZE).unwrap();
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
            let text_color = recipient_placeholder_node.get_property("text_color").unwrap();
            let label_text_color = add_recipient_label_node.get_property("text_color").unwrap();
            let btn_bg_valid_visible = btn_bg_valid_clone.get_property("is_visible").unwrap();
            let btn_bg_invalid_visible = btn_bg_invalid_clone.get_property("is_visible").unwrap();
            let addr = recipient_input2.get_property_str("text").unwrap();
            if addr.is_empty() {
                text_color.set_f32(atom, Role::App, 3, 0.45).unwrap();
                // Grey color for invalid
                label_text_color.set_f32(atom, Role::App, 0, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 1, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 2, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 3, 1.).unwrap();
                btn_bg_valid_visible.set_bool(atom, Role::App, 0, false).unwrap();
                btn_bg_invalid_visible.set_bool(atom, Role::App, 0, true).unwrap();
            } else {
                text_color.set_f32(atom, Role::App, 3, 0.).unwrap();
                // Cyan color for valid
                if is_valid_address(&addr) {
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
    let step3_is_visible2 = step3_is_visible.clone();
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
            if !is_valid_address(&text) {
                continue;
            }

            let atom = &mut renderer.make_guard(gfxtag!("add recipient button"));
            {
                let mut data = send_tx_data3.lock().unwrap();
                data.recipient = Some(text.clone());
            }
            // Update step3 display with token and recipient
            let data = send_tx_data3.lock().unwrap().clone();

            if let Some(token_symbol) = &data.token_symbol {
                let token_symbol_node = sg_root.lookup_node("/window/content/wallet_send_step3_layer/send_selected_token_symbol3").unwrap();
                token_symbol_node.set_property_str(atom, Role::App, "text", token_symbol).unwrap();

                // Update amount token symbol
                if let Some(amount_token_node) = sg_root.lookup_node("/window/content/wallet_send_step3_layer/send_amount_token_symbol") {
                    amount_token_node.set_property_str(atom, Role::App, "text", token_symbol).unwrap();

                    // Update available balance
                    let available_balance = get_balance(token_symbol);
                    if let Some(available_balance_node) = sg_root.lookup_node("/window/content/wallet_send_step3_layer/send_available_balance") {
                        available_balance_node.set_property_str(atom, Role::App, "text", format!("{available_balance} available")).unwrap();
                    }
                }
            }
            if let Some(token_name) = &data.token_name {
                let token_name_node = sg_root.lookup_node("/window/content/wallet_send_step3_layer/send_selected_token_name3").unwrap();
                token_name_node.set_property_str(atom, Role::App, "text", token_name).unwrap();
            }
            if let Some(recipient) = &data.recipient {
                let recipient_node = sg_root.lookup_node("/window/content/wallet_send_step3_layer/send_recipient_value3").unwrap();
                recipient_node.set_property_str(atom, Role::App, "text", recipient).unwrap();
            }

            step2_is_visible2.set(atom, false);
            step3_is_visible2.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // ============================================
    // Step 3: Amount layer content
    // ============================================
    create_bg_mesh(app, atom, &send_step3_layer, "send_bg3").await;
    create_header_bg(app, atom, &send_step3_layer, "send_header_bg3").await;

    // Back button
    let node = create_vector_art("send_back_btn_bg3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step3_layer.link(node);

    let mut y = 0.;

    let node = create_button("send_back_btn3");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let step2_is_visible3 = step2_is_visible.clone();
    let step3_is_visible1 = step3_is_visible.clone();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("send_back_clicked3");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send step3 back button"));
            step3_is_visible1.set(atom, false);
            step2_is_visible3.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step3_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step3_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Selected token display (same layout as step2)
    let node = create_text("send_selected_token_symbol3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    // Set initial token value from send_tx_data
    let token_symbol_text = send_tx_data.lock().unwrap().token_symbol.clone().unwrap_or_else(|| "".to_string());
    node.set_property_str(atom, Role::App, "text", &token_symbol_text).unwrap();
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
    send_step3_layer.link(node);

    let node = create_text("send_selected_token_name3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X + TOKEN_NAME_OFFSET).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    // Set initial token value from send_tx_data
    let token_name_text = send_tx_data.lock().unwrap().token_name.clone().unwrap_or_else(|| "---".to_string());
    node.set_property_str(atom, Role::App, "text", &token_name_text).unwrap();
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
    let selected_token_text3 = node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step3_layer.link(selected_token_text3.clone());

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(app, atom, &send_step3_layer, "send_token_separator3", &mut y).await;

    // Recipient display
    let node = create_text("send_recipient_label3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
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
    send_step3_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    // Recipient address value
    let node = create_text("send_recipient_value3");
    let addr_h_prop = node.get_property("height").unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "").unwrap();
    node.set_property_enum(atom, Role::App, "overflow_wrap", "anywhere").unwrap();
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
    send_step3_layer.link(node);

    // Separator line
    let mut y_ = y.clone();
    let y2 = format!("{y} + (PADDING_Y * 2. + addr_height) + 1");
    let node = create_separator(app, atom, &send_step3_layer, "send_amount_label_separator", &mut y_).await;
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile(&y2).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");

    // Available balance text
    let available_balance_node = create_text("send_available_balance");
    let prop = available_balance_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - BASE_FONTSIZE - 1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    available_balance_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    available_balance_node.set_property_str(atom, Role::App, "text", "").unwrap();
    let prop = available_balance_node.get_property("text_color").unwrap();
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
    available_balance_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let available_balance_node = available_balance_node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    send_step3_layer.link(available_balance_node.clone());

    // Separator line below available balance
    create_separator_expr(
        app,
        atom,
        &send_step3_layer,
        "available_balance_separator",
        &mut cc,
        "h - PADDING_X * 2 - BUTTON_HEIGHT",
    ).await;

    // Amount wrapper layer
    let amount_y = format!("({y2}) + (h - ({y2}) - BUTTON_HEIGHT - PADDING_X * 2 - PADDING_Y - BASE_FONTSIZE - AMOUNT_FONTSIZE - 1) / 2");
    let amount_wrapper = create_layer("send_amount_wrapper");
    amount_wrapper.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    let prop = amount_wrapper.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(&amount_y).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    amount_wrapper.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let amount_wrapper = amount_wrapper.setup(|me| Layer::new(me, app.renderer.clone())).await;
    send_step3_layer.link(amount_wrapper.clone());

    // Amount input
    let input_node = create_singleline_edit("send_amount_input");
    input_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = input_node.get_property("rect").unwrap();
    // Position at (0, 0) within wrapper - wrapper controls the position
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    cc.add_const_f32("AMOUNT_CHAR_WIDTH", AMOUNT_CHAR_WIDTH+6.); // Initial width for "0"
    let code = cc.compile("AMOUNT_CHAR_WIDTH").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    input_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    let prop = input_node.get_property("text_color").unwrap();
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
    let prop = input_node.get_property("hi_bg_color").unwrap();
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
    let prop = input_node.get_property("text_hi_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.44).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.96).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = input_node.get_property("cursor_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.816).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.627).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    input_node.set_property_f32(atom, Role::App, "cursor_ascent", 0.).unwrap();
    input_node.set_property_f32(atom, Role::App, "cursor_descent", AMOUNT_FONTSIZE*1.3).unwrap();
    input_node.set_property_f32(atom, Role::App, "select_ascent", AMOUNT_FONTSIZE*1.3).unwrap();
    input_node.set_property_f32(atom, Role::App, "select_descent", AMOUNT_FONTSIZE/3.).unwrap();
    input_node.set_property_f32(atom, Role::App, "handle_descent", AMOUNT_FONTSIZE/2.5).unwrap();
    input_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let input_node = input_node
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
    amount_wrapper.link(input_node.clone());

    // Set initial text to "0" for the amount input
    let input_text_prop = input_node.get_property("text").unwrap();
    input_text_prop.set_str(atom, Role::App, 0, "0").unwrap();
    // Update editor's internal state
    if let Pimpl::Edit(edit) = input_node.pimpl() {
        edit.on_text_prop_changed();
    }

    // Token symbol text node (displayed next to amount)
    let token_symbol_node = create_text("send_amount_token_symbol");
    let prop = token_symbol_node.get_property("rect").unwrap();
    // Position will be dynamically updated based on amount text
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(format!("{amount_y} - AMOUNT_FONTSIZE * 0.1")).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    token_symbol_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    // Initial text - will be set from send_tx_data.token_symbol
    let token_symbol_initial = send_tx_data.lock().unwrap().token_symbol.clone().unwrap_or_else(|| "".to_string());
    token_symbol_node.set_property_str(atom, Role::App, "text", &token_symbol_initial).unwrap();
    let prop = token_symbol_node.get_property("text_color").unwrap();
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
    token_symbol_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let token_symbol_node = token_symbol_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step3_layer.link(token_symbol_node.clone());

    let token_symbol_initial = send_tx_data.lock().unwrap().token_symbol.clone().unwrap_or_else(|| "".to_string());
    update_amount_screen(
        atom,
        "0",
        &token_symbol_initial,
        &amount_wrapper,
        &input_node,
        &token_symbol_node,
        Some(&available_balance_node),
    );

    y += PADDING_Y * 2. + BUTTON_HEIGHT + 10.;

    // Add amount button with states
    let (node, btn_bg_valid, btn_bg_invalid, add_amount_label_node) = create_bottom_button_with_states(
        app,
        atom,
        &send_step3_layer,
        "send_add_amount_btn",
        &mut cc,
        "add amount",
        &window_scale,
        i18n_fish,
        false, // Start with invalid state (grey)
    ).await;

    // Amount input validation listener
    let amount_input2 = input_node.clone();
    let amount_wrapper2 = amount_wrapper.clone();
    let available_balance_node2 = available_balance_node.clone();
    let token_symbol_node2 = token_symbol_node.clone();
    let send_tx_data5 = send_tx_data.clone();
    let amount_text = amount_input2.get_property("text").unwrap();
    let amount_text_sub = amount_text.subscribe_modify();
    let renderer = app.renderer.clone();
    let btn_bg_valid_clone = btn_bg_valid.clone();
    let btn_bg_invalid_clone = btn_bg_invalid.clone();
    let listen_amount_text = app.ex.spawn(async move {
        let mut old_amount_text = "0".to_string();
        while let Ok(_) = amount_text_sub.receive().await {
            let atom = &mut renderer.make_guard(gfxtag!("wallet amount input recv"));
            let label_text_color = add_amount_label_node.get_property("text_color").unwrap();
            let btn_bg_valid_visible = btn_bg_valid_clone.get_property("is_visible").unwrap();
            let btn_bg_invalid_visible = btn_bg_invalid_clone.get_property("is_visible").unwrap();
            let amount_input_text_color = amount_input2.get_property("text_color").unwrap();
            let amount = amount_input2.get_property_str("text").unwrap();

            let sanitized_amount = if amount.is_empty() {
                "0".to_string()
            } else {
                let mut result = String::new();
                let mut dot_seen = false;
                for c in amount.chars() {
                    if c.is_ascii_digit() {
                        result.push(c);
                    } else if c == '.' && !dot_seen {
                        dot_seen = true;
                        result.push(c);
                    }
                }

                // Strip leading zeros
                let parts: Vec<&str> = result.split('.').collect();
                let integer_part = if !parts.is_empty() && !parts[0].is_empty() {
                    let trimmed = parts[0].trim_start_matches('0');
                    let trimmed = if trimmed.is_empty() {
                        "0".to_string()
                    } else {
                        trimmed.to_string()
                    };

                    // Forces removing "0" chars when the amount was previously "0".
                    // This is so that when the input cursor is BEFORE the initial
                    // "0" and you insert a digit, it removes the initial "0".
                    if old_amount_text == "0" && trimmed != "0" && parts.len() == 1 {
                        trimmed.chars().filter(|&c| c != '0').collect()
                    } else {
                        trimmed
                    }
                } else {
                    "0".to_string()
                };

                if parts.len() > 1 {
                    format!("{}.{}", integer_part, parts[1])
                } else {
                    integer_part
                }
            };

            if sanitized_amount != amount {
                let input_text_prop = amount_input2.get_property("text").unwrap();
                input_text_prop.set_str(atom, Role::Ignored, 0, &sanitized_amount).unwrap();
                if let Pimpl::Edit(edit) = amount_input2.pimpl() {
                    edit.on_text_prop_changed();
                }
            }

            old_amount_text = sanitized_amount.clone();

            // Update positions to center amount and token symbol
            let token_symbol = send_tx_data5.lock().unwrap().token_symbol.clone().unwrap_or_else(|| "".to_string());
            update_amount_screen(
                atom,
                &sanitized_amount,
                &token_symbol,
                &amount_wrapper2,
                &amount_input2,
                &token_symbol_node2,
                Some(&available_balance_node2),
            );
            let available_balance = get_balance(&token_symbol);
            let is_valid = if sanitized_amount == "0" {
                false
            } else {
                match sanitized_amount.parse::<f32>() {
                    Ok(v) if v > 0. && v <= available_balance => true,
                    _ => false,
                }
            };

            // Set amount input text color: grey if "0", white otherwise
            if sanitized_amount == "0" {
                amount_input_text_color.set_f32(atom, Role::App, 0, 0.5).unwrap();
                amount_input_text_color.set_f32(atom, Role::App, 1, 0.5).unwrap();
                amount_input_text_color.set_f32(atom, Role::App, 2, 0.5).unwrap();
                amount_input_text_color.set_f32(atom, Role::App, 3, 1.).unwrap();
            } else {
                amount_input_text_color.set_f32(atom, Role::App, 0, 1.).unwrap();
                amount_input_text_color.set_f32(atom, Role::App, 1, 1.).unwrap();
                amount_input_text_color.set_f32(atom, Role::App, 2, 1.).unwrap();
                amount_input_text_color.set_f32(atom, Role::App, 3, 1.).unwrap();
            }

            if is_valid {
                label_text_color.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
                label_text_color.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
                label_text_color.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
                label_text_color.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
                btn_bg_valid_visible.set_bool(atom, Role::App, 0, true).unwrap();
                btn_bg_invalid_visible.set_bool(atom, Role::App, 0, false).unwrap();
            } else {
                label_text_color.set_f32(atom, Role::App, 0, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 1, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 2, 0.5).unwrap();
                label_text_color.set_f32(atom, Role::App, 3, 1.).unwrap();
                btn_bg_valid_visible.set_bool(atom, Role::App, 0, false).unwrap();
                btn_bg_invalid_visible.set_bool(atom, Role::App, 0, true).unwrap();
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_amount_text);

    let renderer = app.renderer.clone();
    let amount_input2 = input_node.clone();
    let send_tx_data4 = send_tx_data.clone();
    let step3_is_visible2 = step3_is_visible.clone();
    let step4_is_visible2 = step4_is_visible.clone();
    let sg_root = app.sg_root.clone();
    let (slot, recvr) = Slot::new("send_add_amount_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let text = amount_input2.get_property_str("text").unwrap();
            let available_balance = 100.24_f32; // TODO
            let is_valid = if text.is_empty() {
                false
            } else {
                match text.parse::<f32>() {
                    Ok(v) if v > 0. && v <= available_balance => true,
                    _ => false,
                }
            };
            if is_valid {
                let atom = &mut renderer.make_guard(gfxtag!("add amount button"));
                {
                    let mut data = send_tx_data4.lock().unwrap();
                    data.amount = Some(text.clone());
                }
                // Update step4 display with transaction data
                let data = send_tx_data4.lock().unwrap().clone();
                if let Some(token_symbol) = &data.token_symbol {
                    let token_symbol_node = sg_root.lookup_node("/window/content/wallet_send_step4_layer/send_selected_token_symbol4").unwrap();
                    token_symbol_node.set_property_str(atom, Role::App, "text", token_symbol).unwrap();

                    if let Some(amount_token_node) = sg_root.lookup_node("/window/content/wallet_send_step4_layer/send_amount_token_symbol4") {
                        amount_token_node.set_property_str(atom, Role::App, "text", token_symbol).unwrap();
                    }
                }
                if let Some(token_name) = &data.token_name {
                    let token_name_node = sg_root.lookup_node("/window/content/wallet_send_step4_layer/send_selected_token_name4").unwrap();
                    token_name_node.set_property_str(atom, Role::App, "text", token_name).unwrap();
                }
                if let Some(recipient) = &data.recipient {
                    let recipient_node = sg_root.lookup_node("/window/content/wallet_send_step4_layer/send_recipient_value4").unwrap();
                    recipient_node.set_property_str(atom, Role::App, "text", recipient).unwrap();
                }
                if let Some(amount) = &data.amount {
                    let amount_node = sg_root.lookup_node("/window/content/wallet_send_step4_layer/send_amount_wrapper4/send_amount_text4").unwrap();
                    amount_node.set_property_str(atom, Role::App, "text", amount).unwrap();
                }
                step3_is_visible2.set(atom, false);
                step4_is_visible2.set(atom, true);
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Add listener for step3 visibility to focus/unfocus amount input
    let step3_is_visible_clone = step3_is_visible.clone();
    let renderer_clone = app.renderer.clone();
    let amount_wrapper_clone = amount_wrapper.clone();
    let input_node_clone = input_node.clone();
    let token_symbol_node_clone = token_symbol_node.clone();
    let send_tx_data_clone = send_tx_data.clone();
    let step3_is_visible_sub = step3_is_visible.prop().subscribe_modify();
    let listen_step3_visible = app.ex.spawn(async move {
        while let Ok(_) = step3_is_visible_sub.receive().await {
            if step3_is_visible_clone.get() {
                loop { // TODO: this waits for draw, there's probably a better way
                    msleep(50).await;
                    let input_rect = PropertyRect::wrap(&input_node_clone, Role::App, "rect").unwrap();
                    if input_rect.has_cached() {
                        break;
                    }
                }

                // Focus the amount input
                input_node_clone.call_method("focus", vec![]).await.unwrap();

                // Get the current token symbol and update positions
                let token_symbol = send_tx_data_clone.lock().unwrap().token_symbol.clone().unwrap_or_default();
                if !token_symbol.is_empty() {
                    let atom = &mut renderer_clone.make_guard(gfxtag!("update amount positions on visible"));
                    let current_amount = input_node_clone.get_property_str("text").unwrap();
                    update_amount_screen(
                        atom,
                        &current_amount,
                        &token_symbol,
                        &amount_wrapper_clone,
                        &input_node_clone,
                        &token_symbol_node_clone,
                        Some(&available_balance_node),
                    );
                }
            } else {
                // Unfocus when becoming hidden
                input_node_clone.call_method("unfocus", vec![]).await.unwrap();
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_step3_visible);

    // ============================================
    // Step 4: Confirmation layer content
    // ============================================
    create_bg_mesh(app, atom, &send_step4_layer, "send_bg4").await;
    create_header_bg(app, atom, &send_step4_layer, "send_header_bg4").await;

    // Back button
    let node = create_vector_art("send_back_btn_bg4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, BACKARROW_X).unwrap();
    prop.set_f32(atom, Role::App, 1, BACKARROW_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 500.).unwrap();
    prop.set_f32(atom, Role::App, 3, 500.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let shape = shape::create_back_arrow().scaled(BACKARROW_SCALE);
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    send_step4_layer.link(node);

    let mut y = 0.;

    let node = create_button("send_back_btn4");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, y).unwrap();
    prop.set_f32(atom, Role::App, 2, WALLET_BTN_SIZE * 2.).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();

    let step3_is_visible2 = step3_is_visible.clone();
    let step4_is_visible1 = step4_is_visible.clone();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("send_back_clicked4");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send step4 back button"));
            step4_is_visible1.set(atom, false);
            step3_is_visible2.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    send_step4_layer.link(node);

    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step4_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Selected token display
    let node = create_text("send_selected_token_symbol4");
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
    send_step4_layer.link(node);

    let node = create_text("send_selected_token_name4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X + TOKEN_NAME_OFFSET).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "DRK").unwrap();
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
    send_step4_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    create_separator(app, atom, &send_step4_layer, "send_token_separator4", &mut y).await;

    // Recipient label
    let node = create_text("send_recipient_label4");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
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
    send_step4_layer.link(node);

    y += PADDING_Y * 2. + BASE_FONTSIZE + 1.;

    // Recipient address value
    let recipient_addr_node = create_text("send_recipient_value4");
    let addr_h_prop = recipient_addr_node.get_property("height").unwrap();
    let prop = recipient_addr_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    recipient_addr_node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
    recipient_addr_node.set_property_str(atom, Role::App, "text", "").unwrap();
    recipient_addr_node.set_property_enum(atom, Role::App, "overflow_wrap", "anywhere").unwrap();
    let prop = recipient_addr_node.get_property("text_color").unwrap();
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
    recipient_addr_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let recipient_addr_node = recipient_addr_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step4_layer.link(recipient_addr_node.clone());

    // Separator line
    let mut y_ = y.clone();
    let y2 = format!("{y} + (PADDING_Y * 2. + addr_height) + 1");
    let node = create_separator(app, atom, &send_step4_layer, "send_amount_label_separator4", &mut y_).await;
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile(&y2).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");

    // Amount display - same layout as step3's amount input
    let amount_y = format!("({y2}) + (h - ({y2}) - BUTTON_HEIGHT - PADDING_X * 2 - PADDING_Y - BASE_FONTSIZE - AMOUNT_FONTSIZE - 1) / 2 - AMOUNT_FONTSIZE * 0.1");
    let amount_wrapper = create_layer("send_amount_wrapper4");
    amount_wrapper.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    let prop = amount_wrapper.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(&amount_y).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    amount_wrapper.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let amount_wrapper = amount_wrapper.setup(|me| Layer::new(me, app.renderer.clone())).await;
    send_step4_layer.link(amount_wrapper.clone());

    // Amount text
    let amount_text_node = create_text("send_amount_text4");
    let prop = amount_text_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    amount_text_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    let prop = amount_text_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    amount_text_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    amount_text_node.set_property_str(atom, Role::App, "text", "0").unwrap();
    let amount_text_node = amount_text_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    amount_wrapper.link(amount_text_node.clone());

    // Token symbol text node (displayed next to amount)
    let token_symbol_node = create_text("send_amount_token_symbol4");
    let prop = token_symbol_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(amount_y).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, AMOUNT_FONTSIZE).unwrap();
    prop.add_depend(&addr_h_prop, 0, "addr_height");
    token_symbol_node.set_property_f32(atom, Role::App, "font_size", AMOUNT_FONTSIZE).unwrap();
    token_symbol_node.set_property_str(atom, Role::App, "text", "").unwrap();
    let prop = token_symbol_node.get_property("text_color").unwrap();
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
    token_symbol_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let token_symbol_node = token_symbol_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step4_layer.link(token_symbol_node.clone());

    // Transaction fee label
    let tx_fee_label_node = create_text("send_fee_label");
    let prop = tx_fee_label_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - BASE_FONTSIZE - 1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    tx_fee_label_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    tx_fee_label_node.set_property_str(atom, Role::App, "text", "Transaction fee").unwrap();
    let prop = tx_fee_label_node.get_property("text_color").unwrap();
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
    tx_fee_label_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let tx_fee_label_node = tx_fee_label_node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    send_step4_layer.link(tx_fee_label_node.clone());

    // Transaction fee value
    let tx_fee_value_node = create_text("send_fee_value");
    let prop = tx_fee_value_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - BASE_FONTSIZE - 1").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    tx_fee_value_node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    tx_fee_value_node.set_property_str(atom, Role::App, "text", MOCK_TX_FEE).unwrap();
    tx_fee_value_node.set_property_enum(atom, Role::App, "text_align", "end").unwrap();
    let prop = tx_fee_value_node.get_property("text_color").unwrap();
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
    tx_fee_value_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let tx_fee_value_node = tx_fee_value_node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    send_step4_layer.link(tx_fee_value_node.clone());

    // Separator line
    create_separator_expr(
        app,
        atom,
        &send_step4_layer,
        "tx_fee_separator",
        &mut cc,
        "h - PADDING_X * 2 - BUTTON_HEIGHT",
    ).await;

    y += PADDING_Y * 2. + BUTTON_HEIGHT + 10.;

    // Send button (bottom button)
    let node = create_bottom_button(
        app,
        atom,
        &send_step4_layer,
        "send_send_btn",
        &mut cc,
        Some("send"),
        &window_scale,
        i18n_fish,
    ).await;

    let renderer = app.renderer.clone();
    let sg_root = app.sg_root.clone();
    let step4_is_visible1 = step4_is_visible.clone();
    let step5_is_visible1 = step5_is_visible.clone();
    let send_tx_data_clone = send_tx_data.clone();
    let (slot, recvr) = Slot::new("send_send_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("send button"));

            // Update step5 with transaction info
            let data = send_tx_data_clone.lock().unwrap().clone();

            if let Some(tx_info_node) = sg_root.lookup_node("/window/content/wallet_send_step5_layer/send_tx_info5") {
                let amount = data.amount.as_deref().unwrap_or("0");
                let token_symbol = data.token_symbol.as_deref().unwrap_or("DRK");
                let recipient = data.recipient.as_deref().unwrap_or("recipient");
                let tx_text = format!("Sending {} {} to {}", amount, token_symbol, recipient);
                tx_info_node.set_property_str(atom, Role::App, "text", tx_text).unwrap();
            }

            step4_is_visible1.set(atom, false);
            step5_is_visible1.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    // Add listener for step4 visibility to update amount positions
    let step4_is_visible_clone = step4_is_visible.clone();
    let renderer_clone = app.renderer.clone();
    let amount_wrapper_clone = amount_wrapper.clone();
    let amount_text_node_clone = amount_text_node.clone();
    let token_symbol_node_clone = token_symbol_node.clone();
    let send_tx_data_clone2 = send_tx_data.clone();
    let step4_is_visible_sub = step4_is_visible.prop().subscribe_modify();
    let listen_step4_visible = app.ex.spawn(async move {
        while let Ok(_) = step4_is_visible_sub.receive().await {
            if step4_is_visible_clone.get() {
                loop {
                    msleep(50).await;
                    let text_rect = PropertyRect::wrap(&amount_text_node_clone, Role::App, "rect").unwrap();
                    if text_rect.has_cached() {
                        break;
                    }
                }

                let atom = &mut renderer_clone.make_guard(gfxtag!("update step4 amount positions"));
                let data = send_tx_data_clone2.lock().unwrap().clone();

                let amount_text = data.amount.unwrap_or_else(|| "0".to_string());
                let token_symbol = data.token_symbol.unwrap_or_else(|| "".to_string());

                // Update positions to center amount and token symbol
                update_amount_screen(
                    atom,
                    &amount_text,
                    &token_symbol,
                    &amount_wrapper_clone,
                    &amount_text_node_clone,
                    &token_symbol_node_clone,
                    None,
                );
            }
        }
    });
    app.tasks.lock().unwrap().push(listen_step4_visible);

    // ============================================
    // Step 5: Transaction in progress layer content
    // ============================================
    create_bg_mesh(app, atom, &send_step5_layer, "send_bg5").await;
    create_header_bg(app, atom, &send_step5_layer, "send_header_bg5").await;

    y = 0.;
    y += HEADER_HEIGHT;

    create_title(app, atom, &send_step5_layer, &window_scale, i18n_fish, "SEND", &mut y).await;

    // Subtitle: "Transaction in progress..."
    create_subtitle(app, atom, &send_step5_layer, &window_scale, i18n_fish, "Transaction in progress...", &mut y).await;

    // Transaction info text: "Sending {amount} {token_symbol} to {recipient_address}"
    let node = create_text("send_tx_info5");
    let info_h_prop = node.get_property("height").unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, y + PADDING_Y).unwrap();
    let code = cc.compile("w - PADDING_X * 2").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Sending 0 DRK to recipient").unwrap();
    node.set_property_enum(atom, Role::App, "overflow_wrap", "anywhere").unwrap();
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
    send_step5_layer.link(node);

    let sep = create_separator(app, atom, &send_step5_layer, "send_info_separator5", &mut 0.).await;
    let prop = sep.get_property("rect").unwrap();
    let code = cc.compile(format!("{y} + PADDING_Y * 2 + info_height + 1")).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.add_depend(&info_h_prop, 0, "info_height");

    // Hint text
    let hint_text_node = create_text("send_close_hint5");
    let prop = hint_text_node.get_property("rect").unwrap();
    let code = cc.compile("w / 2 - (HINT_FONTSIZE * 0.7 * 31) / 2").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - PADDING_X * 2 - BUTTON_HEIGHT - PADDING_Y - HINT_FONTSIZE * 2").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("HINT_FONTSIZE * 0.7 * 31").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, HINT_FONTSIZE/2.).unwrap();
    hint_text_node.set_property_f32(atom, Role::App, "font_size", HINT_FONTSIZE).unwrap();
    hint_text_node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
    hint_text_node.set_property_str(atom, Role::App, "text", "You can close this screen while the transaction is confirming.").unwrap();
    let prop = hint_text_node.get_property("text_color").unwrap();
    if COLOR_SCHEME == ColorScheme::DarkMode {
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.7).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.).unwrap();
        prop.set_f32(atom, Role::App, 3, 0.7).unwrap();
    }
    hint_text_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();

    let node = hint_text_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    send_step5_layer.link(node);

    // Close label (bottom button)
    let close_label_node = create_text("send_close_label");
    let prop = close_label_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT + BUTTON_HEIGHT / 2 - BUTTON_FONTSIZE / 1.8").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    close_label_node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
    close_label_node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
    close_label_node.set_property_str(atom, Role::App, "text", "close").unwrap();
    let prop = close_label_node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
    prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
    prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
    prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
    close_label_node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let close_label_node = close_label_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;

    // Close button
    let node = create_bottom_button(
        app,
        atom,
        &send_step5_layer,
        "send_close_btn",
        &mut cc,
        Some("close"),
        &window_scale,
        i18n_fish,
    ).await;

    // Click handler
    let wallet_is_visible = wallet_is_visible.clone();
    let step5_is_visible = step5_is_visible.clone();
    let renderer = app.renderer.clone();
    let (slot, recvr) = Slot::new("send_close_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut renderer.make_guard(gfxtag!("close button"));
            step5_is_visible.set(atom, false);
            wallet_is_visible.set(atom, true);
        }
    });
    app.tasks.lock().unwrap().push(listen_click);

    send_step1_layer
}

/// Creates a title text node with separator line.
/// Returns the text node after setup, linked to the layer
async fn create_title(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
    name: &str,
    y: &mut f32,
) -> SceneNodePtr {
    let node = create_text(name);
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, *y + TITLE_PADDING).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", name).unwrap();
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
    let node = node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    layer.link(node.clone());

    *y += TITLE_PADDING * 2. + TITLE_FONTSIZE + 1.;

    create_separator(app, atom, layer, &format!("{}_separator", name), y).await;
    node
}

/// Creates a subtitle text node with separator line (e.g., "Pick a token to send", "Recipient", "Address")
/// Returns the text node after setup, linked to the layer
async fn create_subtitle(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
    text: &str,
    y: &mut f32,
) -> SceneNodePtr {
    let node = create_text(text);
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    prop.set_f32(atom, Role::App, 1, *y + PADDING_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, TITLE_FONTSIZE).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", text).unwrap();
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
    let node = node.setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())).await;
    layer.link(node.clone());

    *y += PADDING_Y * 2. + TITLE_FONTSIZE + 1.;

    create_separator(app, atom, layer, &format!("{}_separator", text), y).await;

    node
}

/// Creates a background mesh with gradient box.
async fn create_bg_mesh(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
) {
    let node = create_vector_art(name);
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = VectorShape::new();
    shape.add_gradient_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [[0., 0., 0., 0.5], [0., 0., 0., 0.5], [0., 0., 0., 0.5], [0., 0., 0., 0.8]],
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer.link(node);
}

/// Creates a header background with filled box and separator line at bottom.
async fn create_header_bg(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
) {
    let node = create_vector_art(name);
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, HEADER_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    let (bg_color, sep_color) = match COLOR_SCHEME {
        ColorScheme::DarkMode => ([0., 0.11, 0.11, 1.], [0.2, 0.2745, 0.2784, 1.]),
        ColorScheme::PaperLight => ([1., 1., 1., 1.], [0., 0.6, 0.65, 1.]),
    };

    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(HEADER_HEIGHT),
        bg_color,
    );
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(HEADER_HEIGHT - 1.),
        expr::load_var("w"),
        expr::const_f32(HEADER_HEIGHT),
        sep_color,
    );
    let node = node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer.link(node);
}

/// Creates a separator line at the given y expression.
/// Returns the separator node after setup, linked to the layer
async fn create_separator_expr(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    cc: &mut expr::Compiler,
    y_expr: &str,
) -> SceneNodePtr {
    let sep_node = create_vector_art(name);
    let prop = sep_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    let code = cc.compile(y_expr).unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    sep_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(1.),
        [0.2, 0.2745, 0.2784, 1.],
    );
    let sep_node = sep_node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer.link(sep_node.clone());
    sep_node
}

/// Creates a separator line at the current y position and increments y.
/// Returns the separator node after setup, linked to the layer
async fn create_separator(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    y: &mut f32,
) -> SceneNodePtr {
    let sep_node = create_vector_art(name);
    let prop = sep_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, *y).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    sep_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(1.),
        [0.2, 0.2745, 0.2784, 1.],
    );
    let sep_node = sep_node.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer.link(sep_node.clone());

    *y += 1.;
    sep_node
}

async fn create_tokens_table(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
    layer: &SceneNodePtr,
    tokens: &[(&str, &str, f32)],
    y: &mut f32,
    on_click: impl Fn(&SceneNodePtr, &str, &str, &f32) + Clone + Send + Sync + 'static,
) {
    use crate::prop::Role;
    use crate::scene::Slot;

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("PADDING_X", PADDING_X);

    for (i, (token, full_name, balance)) in tokens.iter().enumerate() {
        let row_y = *y;
        let row_height = PADDING_Y * 2. + BASE_FONTSIZE + 1.;

        // Token row button
        let node = create_button(&format!("token_row_btn_{}", i));
        node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, row_y).unwrap();
        let code = cc.compile("w").unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        prop.set_f32(atom, Role::App, 3, row_height).unwrap();

        let callback = on_click.clone();
        let token_str = token.to_string();
        let name_str = full_name.to_string();
        let balance2 = *balance;
        let sg_root = app.sg_root.clone();
        let (slot, recvr) = Slot::new(&format!("token_row_clicked_{}", i));
        node.register("click", slot).unwrap();
        let listen_click = app.ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                callback(&sg_root, &token_str, &name_str, &balance2);
            }
        });
        app.tasks.lock().unwrap().push(listen_click);

        let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
        layer.link(node);

        // Token symbol
        let node = create_text(&format!("token_{}", i));
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
        prop.set_f32(atom, Role::App, 1, *y + PADDING_Y).unwrap();
        prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
        prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
        node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
        node.set_property_str(atom, Role::App, "text", *token).unwrap();
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
        layer.link(node);

        // Token name
        let node = create_text(&format!("token_name_{}", i));
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, PADDING_X + TOKEN_NAME_OFFSET).unwrap();
        prop.set_f32(atom, Role::App, 1, *y + PADDING_Y).unwrap();
        prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
        prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
        node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
        node.set_property_str(atom, Role::App, "text", *full_name).unwrap();
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
        layer.link(node);

        // Balance
        let node = create_text(&format!("token_balance_{}", i));
        let prop = node.get_property("rect").unwrap();
        let code = cc.compile("PADDING_X").unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        prop.set_f32(atom, Role::App, 1, *y + PADDING_Y).unwrap();
        let code = cc.compile("w - PADDING_X * 2").unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        prop.set_f32(atom, Role::App, 3, BASE_FONTSIZE).unwrap();
        node.set_property_enum(atom, Role::App, "text_align", "end").unwrap();
        node.set_property_f32(atom, Role::App, "font_size", BASE_FONTSIZE).unwrap();
        node.set_property_str(atom, Role::App, "text", balance.to_string()).unwrap();
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
            .setup(|me| {
                Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone())
            })
            .await;
        layer.link(node);

        *y += PADDING_Y * 2. + BASE_FONTSIZE;

        create_separator(app, atom, layer, &format!("token_separator_{}", i), y).await;
    }
}

/// Creates a bottom button with teal outline and click handler.
/// Returns the button node after setup, linked to the layer.
/// The caller can use the returned button node to add custom click handlers.
async fn create_bottom_button(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    cc: &mut expr::Compiler,
    label_text: Option<&str>,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
) -> SceneNodePtr {
    // Button bg (teal outline)
    let node = create_vector_art(&format!("{}_bg", name));
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
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
    layer.link(node.clone());

    // Button label text (if provided)
    if let Some(text) = label_text {
        let label_node = create_text(&format!("{}_label", name));
        let prop = label_node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
        let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT + BUTTON_HEIGHT / 2 - BUTTON_FONTSIZE / 1.8").unwrap();
        prop.set_expr(atom, Role::App, 1, code).unwrap();
        let code = cc.compile("w - PADDING_X * 2.").unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
        label_node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
        label_node.set_property_str(atom, Role::App, "text", text).unwrap();
        label_node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();

        let prop = label_node.get_property("text_color").unwrap();
        prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
        prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
        prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
        prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
        label_node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
        let label_node = label_node
            .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
            .await;
        layer.link(label_node);
    }

    // Button
    let node = create_button(name);
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();

    let node = node.setup(|me| Button::new(me, app.renderer.clone())).await;
    layer.link(node.clone());
    node
}

/// Creates a bottom button with two states: valid (teal) and invalid (grey).
/// Returns a tuple of (button_node, bg_valid_node, bg_invalid_node, label_node) for visibility control.
async fn create_bottom_button_with_states(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    cc: &mut expr::Compiler,
    label_text: &str,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
    initial_valid: bool,
) -> (SceneNodePtr, SceneNodePtr, SceneNodePtr, SceneNodePtr) {
    // Button bg (teal outline - valid state)
    let bg_valid = create_vector_art(&format!("{}_bg", name));
    let prop = bg_valid.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    bg_valid.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    bg_valid.set_property_bool(atom, Role::App, "is_visible", initial_valid).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        COLOR_TEAL,
    );
    let bg_valid = bg_valid.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer.link(bg_valid.clone());

    // Button bg (grey outline - invalid state)
    let bg_invalid = create_vector_art(&format!("{}_bg_grey", name));
    let prop = bg_invalid.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    bg_invalid.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    bg_invalid.set_property_bool(atom, Role::App, "is_visible", !initial_valid).unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        [0.5, 0.5, 0.5, 1.],
    );
    let bg_invalid = bg_invalid.setup(|me| VectorArt::new(me, shape, app.renderer.clone())).await;
    layer.link(bg_invalid.clone());

    // Button label text
    let label_node = create_text(&format!("{}_label", name));
    let prop = label_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT + BUTTON_HEIGHT / 2 - BUTTON_FONTSIZE / 1.8").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();
    label_node.set_property_f32(atom, Role::App, "font_size", BUTTON_FONTSIZE).unwrap();
    label_node.set_property_str(atom, Role::App, "text", label_text).unwrap();
    label_node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();

    let prop = label_node.get_property("text_color").unwrap();
    if initial_valid {
        prop.set_f32(atom, Role::App, 0, COLOR_CYAN[0]).unwrap();
        prop.set_f32(atom, Role::App, 1, COLOR_CYAN[1]).unwrap();
        prop.set_f32(atom, Role::App, 2, COLOR_CYAN[2]).unwrap();
        prop.set_f32(atom, Role::App, 3, COLOR_CYAN[3]).unwrap();
    } else {
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    }
    label_node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    let label_node = label_node
        .setup(|me| Text::new(me, window_scale.clone(), app.renderer.clone(), i18n_fish.clone()))
        .await;
    layer.link(label_node.clone());

    // Button
    let btn = create_button(name);
    btn.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = btn.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, PADDING_X).unwrap();
    let code = cc.compile("h - PADDING_X - BUTTON_HEIGHT").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    let code = cc.compile("w - PADDING_X * 2.").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    prop.set_f32(atom, Role::App, 3, BUTTON_HEIGHT).unwrap();

    let btn = btn.setup(|me| Button::new(me, app.renderer.clone())).await;
    layer.link(btn.clone());

    (btn, bg_valid, bg_invalid, label_node)
}
