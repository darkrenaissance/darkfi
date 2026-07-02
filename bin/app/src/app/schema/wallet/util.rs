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
use darkfi_money_contract::model::TokenId;
use darkfi_serial::Decodable;

use crate::{
    app::App,
    app::node::{create_button, create_text, create_vector_art},
    app::schema::COLOR_SCHEME,
    expr::{self, Compiler},
    gfx::Renderer,
    mesh::{COLOR_TEAL, COLOR_CYAN},
    prop::{PropertyAtomicGuard, PropertyFloat32, Role},
    scene::SceneNodePtr,
    scene::Pimpl,
    ui::{Button, Text, VectorArt, VectorShape},
    util::i18n::I18nBabelFish,
};

use super::{super::ColorScheme, data::*};

pub async fn get_balance(sg_root: &SceneNodePtr, token_id: &TokenId) -> u64 {
    let Some(drk_node) = sg_root.lookup_node("/plugin/drk") else {
        return 0
    };
    let Ok(Some(response_data)) = drk_node.call_method("get_balances", vec![]).await else {
        return 0
    };
    let mut cur = std::io::Cursor::new(response_data);
    let Ok(balances) = Vec::<(String, TokenId, u64)>::decode(&mut cur) else {
        return 0
    };

    balances
        .iter()
        .find(|(_, tid, _)| *tid == *token_id)
        .map(|(_, _, balance)| *balance)
        .unwrap_or(0)
}

/// Update positions for amount input wrapper and token symbol to center them together.
pub async fn update_amount_screen(
    atom: &mut PropertyAtomicGuard,
    sg_root: &SceneNodePtr,
    amount_text: &str,
    token_id: &TokenId,
    token_symbol: &str,
    amount_wrapper_node: &SceneNodePtr,
    amount_input_node: &SceneNodePtr,
    token_node: &SceneNodePtr,
    available_balance_node: Option<&SceneNodePtr>,
) {
    let mut cc = expr::Compiler::new();

    let amount_input_text = amount_input_node.get_property_str("text").unwrap();
    let display_text = if amount_input_text.is_empty() { "0" } else { &amount_input_text };
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
    if let Pimpl::Edit(edit) = token_node.pimpl() {
        edit.reset_scroll();
    }

    // Update token symbol position
    let token_rect = token_node.get_property("rect").unwrap();
    token_rect.set_expr(atom, Role::App, 0, cc.compile("AMOUNT_TOKEN_SPACING + AMOUNT_WIDTH").unwrap()).unwrap();

    // Set available balance
    if let Some(available_balance_node) = available_balance_node {
        let available_balance = encode_base10(get_balance(sg_root, token_id).await, BALANCE_BASE10_DECIMALS);
        available_balance_node.set_property_str(atom, Role::App, "text", format!("{available_balance} available")).unwrap();
    }
}

/// Creates a title text node with separator line.
/// Returns the text node after setup, linked to the layer
pub async fn create_title(
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

    create_separator(&app.renderer, atom, layer, &format!("{}_separator", name), y).await;
    node
}

/// Creates a subtitle text node with separator line (e.g., "Pick a token to send", "Recipient", "Address")
/// Returns the text node after setup, linked to the layer
pub async fn create_subtitle(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
    name: &str,
    text: &str,
    y: &mut f32,
) -> SceneNodePtr {
    let node = create_text(name);
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

    create_separator(&app.renderer, atom, layer, &format!("{}_separator", text), y).await;

    node
}

/// Creates a background mesh with gradient box.
pub async fn create_bg_mesh(
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
pub async fn create_header_bg(
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
pub async fn create_separator_expr(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    cc: &mut Compiler,
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
pub async fn create_separator(
    renderer: &Renderer,
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
    let sep_node = sep_node.setup(|me| VectorArt::new(me, shape, renderer.clone())).await;
    layer.link(sep_node.clone());

    *y += 1.;
    sep_node
}

/// Creates a bottom button with teal outline and click handler.
/// Returns the button node after setup, linked to the layer.
/// The caller can use the returned button node to add custom click handlers.
pub async fn create_bottom_button(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    cc: &mut Compiler,
    label_text: Option<&str>,
    window_scale: &PropertyFloat32,
    i18n_fish: &I18nBabelFish,
) -> SceneNodePtr {
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
pub async fn create_bottom_button_with_states(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    layer: &SceneNodePtr,
    name: &str,
    cc: &mut Compiler,
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
