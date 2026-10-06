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

//! The `light` theme: the `minimal` palette with every color token
//! inverted (each RGB component mapped to `1 - v`, alpha preserved),
//! plus a full-screen white `king` vector-art background at the lowest
//! z band — the light analog of scifi's king video.
//!
//! Unload restores the minimal defaults by the standard journal replay
//! and unlinks the king node.

use std::sync::Arc;

use crate::{
    app::{
        node::create_vector_art,
        schema::{
            chat::ui_consts::{BACKARROW_SCALE, EMOJI_CLOSE_SCALE, EMOJI_SCALE},
            menu::ui_consts::{MENU_ICON_SCALE, SETTINGS_ICON_SCALE},
        },
    },
    error::{Error, Result},
    expr,
    mesh::{rgba, COLOR_BLACK, COLOR_WHITE},
    prop::{PropertyAtomicGuard, PropertyValue, Role},
    shape,
    theme::{Theme, ThemeCtx},
    ui::{VectorArt, VectorShape},
};

pub struct LightTheme;

#[async_trait::async_trait]
impl Theme for LightTheme {
    fn name(&self) -> &'static str {
        "light"
    }

    async fn apply(&self, ctx: &ThemeCtx) -> Result<()> {
        light_apply(ctx).await
    }
}

async fn light_apply(ctx: &ThemeCtx) -> Result<()> {
    let atom = &mut PropertyAtomicGuard::none();
    let cc = expr::Compiler::new();

    let tokens: &[(&str, [f32; 4])] = &[
        ("text_color", [0.08, 0.08, 0.08, 1.]),
        ("edit.text_color", [0.08, 0.08, 0.08, 1.]),
        ("edit.hi_bg_color", [0.65, 0.65, 0.65, 1.]),
        ("edit.text_hi_color", [1., 1., 1., 1.]),
        ("edit.cursor_color", rgba!(0x3300ffff)),
        ("edit.action_fg_color", [0.10, 0.10, 0.10, 1.]),
        ("edit.action_bg_color", [0.85, 0.85, 0.85, 1.]),
        ("menu.bg_color", [0.95, 0.95, 0.95, 0.5]),
        ("menu.role1_color", [0.40, 0.40, 0.40, 1.]),
        ("menu.role2_color", [0.25, 0.25, 0.25, 1.]),
        ("chatview.timestamp_color", [0.45, 0.45, 0.45, 1.]),
        ("chatview.text_color", [0.08, 0.08, 0.08, 1.]),
        ("chatview.hi_bg_color", [0.75, 0.75, 0.75, 1.]),
        ("chatview.action_text_color", [0.20, 0.20, 0.20, 1.]),
        ("chatview.url_text_color", [0.30, 0.20, 0.10, 1.]),
    ];
    for (name, val) in tokens {
        ctx.set_shared_token_color(atom, name, *val)?;
    }

    king_vectorart_node(ctx).await;

    let sg_root = ctx.sg_root();

    let node = sg_root.lookup_node("/window/content/wallet/main_layer/chat_btn_shape").unwrap();
    let color = rgba!(0xF41740ff);
    let mut shape = shape::create_netlogo1(color);
    shape.join(shape::create_netlogo2(color));
    shape.join(shape::create_netlogo3(color));
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let node =
        sg_root.lookup_node("/window/content/chat/menu_layer/mainbtn_layer/wallet_icon").unwrap();
    let mut shape = shape::create_blockchain_netlogo1(color);
    shape.join(shape::create_blockchain_netlogo2(color));
    shape.join(shape::create_blockchain_netlogo3(color));
    shape.join(shape::create_blockchain_netlogo4(color));
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let node =
        sg_root.lookup_node("/window/content/chat/menu_layer/mainbtn_layer/settings_icon").unwrap();
    let shape = shape::create_settings(color).scaled(SETTINGS_ICON_SCALE);
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let node =
        sg_root.lookup_node("/window/content/chat/menu_layer/mainbtn_layer/write_icon").unwrap();
    let shape = shape::create_menu_icon(color).scaled(MENU_ICON_SCALE);
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let color = rgba!(0x3300ffff);
    let node = sg_root.lookup_node("/window/content/wallet/main_layer/receive_label").unwrap();
    let prop = node.get_property("text_color").unwrap();
    ctx.set_touched_color(atom, &prop, color)?;
    let node = sg_root.lookup_node("/window/content/wallet/main_layer/receive_btn_bg").unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        color,
    );
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;
    let node = sg_root.lookup_node("/window/content/wallet/main_layer/wallet_send_btn_bg").unwrap();
    let mut shape = VectorShape::new();
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        1.,
        color,
    );
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;
    let node = sg_root.lookup_node("/window/content/wallet/main_layer/wallet_send_label").unwrap();
    let prop = node.get_property("text_color").unwrap();
    ctx.set_touched_color(atom, &prop, color)?;
    let node = sg_root.lookup_node("/window/content/wallet/main_layer/tokens_label").unwrap();
    let prop = node.get_property("text_color").unwrap();
    ctx.set_touched_color(atom, &prop, COLOR_BLACK)?;
    let node = sg_root.lookup_node("/window/content/wallet/main_layer/tokens_table").unwrap();
    let prop = node.get_property("text_color").unwrap();
    ctx.set_touched_color(atom, &prop, COLOR_BLACK)?;

    let node = sg_root.lookup_node("/window/content/chat/main_chat_layer/channel_label").unwrap();
    let prop = node.get_property("text_color").unwrap();
    ctx.set_touched_color(atom, &prop, COLOR_BLACK)?;
    let node = sg_root.lookup_node("/window/content/chat/menu_layer/channels_label").unwrap();
    let prop = node.get_property("text_color").unwrap();
    ctx.set_touched_color(atom, &prop, COLOR_BLACK)?;

    let node = sg_root.lookup_node("/window/content/header_bg").unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::load_var("h"),
        expr::load_var("w"),
        cc.compile("h + 1").unwrap(),
        COLOR_BLACK,
    );
    let color1 = rgba!(0x0000ffff);
    let color2 = rgba!(0x35ffffff);
    shape.add_smooth_vertical_gradient(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        color1,
        color2,
        8,
        0.2,
    );
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let node =
        sg_root.lookup_node("/window/content/chat/main_chat_layer/content/editbox_bg").unwrap();
    let (bg_color, line_color) = (COLOR_WHITE, rgba!(0x263B3Cff));
    let mut shape = VectorShape::new();
    // Main green background
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        bg_color,
    );
    // Top line
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::const_f32(1.),
        line_color,
    );
    shape.add_radial_glow(
        // Center
        cc.compile("w / 2").unwrap(),
        expr::load_var("h"),
        // Size
        expr::load_var("w"),
        cc.compile("h / 4").unwrap(),
        // Segments
        8,
        // Angles
        std::f32::consts::PI,
        2. * std::f32::consts::PI,
        // Color
        [0., 0.28, 0.2, 1.],
    );
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    // Chat editor icons

    let color = rgba!(0x3300ffff);

    let node =
        sg_root.lookup_node("/window/content/chat/main_chat_layer/content/send_btn_bg").unwrap();
    let shape = shape::create_send_arrow(color).scaled(EMOJI_SCALE);
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let node =
        sg_root.lookup_node("/window/content/chat/main_chat_layer/content/emoji_btn_bg").unwrap();
    let shape = shape::create_emoji_selector(color).scaled(EMOJI_SCALE);
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    let node = sg_root
        .lookup_node("/window/content/chat/main_chat_layer/content/emoji_close_btn_bg")
        .unwrap();
    let shape = shape::create_close_icon(color).scaled(EMOJI_CLOSE_SCALE);
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(Arc::new(shape)))?;

    // Back buttons

    let shape = Arc::new(shape::create_back_arrow(color).scaled(BACKARROW_SCALE));

    let node = sg_root.lookup_node("/window/content/chat/main_chat_layer/back_btn_bg").unwrap();
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(shape.clone()))?;

    let node =
        sg_root.lookup_node("/window/content/chat/contact_screen_layer/back_btn_bg").unwrap();
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(shape.clone()))?;

    let node =
        sg_root.lookup_node("/window/content/chat/channel_screen_layer/back_btn_bg").unwrap();
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(shape.clone()))?;

    let node = sg_root.lookup_node("/window/content/wallet/wallet_back_btn_bg").unwrap();
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(shape.clone()))?;

    let node = sg_root.lookup_node("/window/content/settings_layer/back_btn_bg").unwrap();
    let prop = node.get_property("shape").unwrap();
    ctx.set_touched(atom, &prop, 0, PropertyValue::VectorShape(shape.clone()))?;

    Ok(())
}

/// The full-screen white king background, as a tracked structural node
/// in the reserved low z band under `/window/content`.
async fn king_vectorart_node(ctx: &ThemeCtx) {
    let app = ctx.app().unwrap();
    let content = ctx.sg_root().lookup_node("/window/content").unwrap();

    let node = create_vector_art("king");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();

    let color_white = rgba!(0xF6FAFFff);
    let color_blue = rgba!(0xC3D8FFff);

    let mut shape = VectorShape::new();
    shape.add_gradient_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [color_white, color_white, color_blue, color_blue],
    );
    node.set_property_shape(&mut PropertyAtomicGuard::none(), Role::App, "shape", shape).unwrap();
    node.set_property_u32(&mut PropertyAtomicGuard::none(), Role::App, "z_index", 0).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    ctx.link_tracked(&content, node);
}
