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

//! The `scifi` theme: the cyan-token look, king video background,
//! first-run scramble splash, and the netstatus overlay fade — the
//! shipped dark.fi app look, extracted from the schema (task 7.1).
//!
//! Everything here goes through the `ThemeCtx`, so unload restores the
//! minimal baseline for every touched property, removes journaled
//! dependency edges, cancels the fade watcher and the splash hide task,
//! and unlinks the video/splash/token nodes — atomically, as one batch.

use darkfi::system::msleep;
use indoc::indoc;

use crate::{
    app::{
        node::create_video,
        schema::{VID_ASPECT_RATIO, VID_PATH},
    },
    error::{Error, Result},
    expr::{self, Compiler},
    gfx::gfxtag,
    prop::{
        Property, PropertyAtomicGuard, PropertyFloat32, PropertyPermission, PropertySubType,
        PropertyType, PropertyValue, Role,
    },
    scene::{SceneNode, SceneNodeType},
    theme::{Theme, ThemeCtx},
    ui::Video,
};

pub struct ScifiTheme;

#[async_trait::async_trait]
impl Theme for ScifiTheme {
    fn name(&self) -> &'static str {
        "scifi"
    }

    async fn apply(&self, ctx: &ThemeCtx) -> Result<()> {
        scifi_apply(ctx).await
    }
}

/// scifi's private token vocabulary (design D6): exists only while the
/// theme is applied, referenced from theme-installed vals expressions
/// and journaled overrides — never from schema defaults.
fn private_token_props() -> Vec<Property> {
    let mut label_glow = Property::new(
        "label_glow",
        PropertyType::Float32,
        PropertySubType::Color,
        PropertyPermission { read: Role::ALL, write: Role::Theme },
    );
    label_glow.set_array_len(4);
    label_glow.set_defaults_f32(vec![0.47, 1., 0.75, 1.]).unwrap();

    vec![label_glow]
}

async fn scifi_apply(ctx: &ThemeCtx) -> Result<()> {
    let Some(app) = ctx.app() else { return Err(Error::ThemeNotFound) };
    let atom = &mut PropertyAtomicGuard::none();

    // ------------------------------------------------------------------
    // 1. Shared token values: the cyan palette over the minimal baseline
    // ------------------------------------------------------------------
    let tokens: &[(&str, [f32; 4])] = &[
        ("text_color", [1., 1., 1., 1.]),
        ("text_dim_color", [0.47, 1., 0.75, 1.]),
        ("bg_color", [0., 0.11, 0.11, 1.]),
        ("bg_dim_color", [0., 0.04, 0.04, 1.]),
        ("bg_overlay_color", [0., 0.1, 0.1, 0.7]),
        ("accent_color", [0., 0.94, 1., 1.]),
        ("sep_color", [0.41, 0.6, 0.65, 1.]),
        ("edit.text_color", [1., 1., 1., 1.]),
        ("edit.bg_color", [0., 0.13, 0.08, 1.]),
        ("edit.hi_bg_color", [0., 0.27, 0.22, 1.]),
        ("edit.text_hi_color", [0.44, 0.96, 1., 1.]),
        ("edit.cursor_color", [0.816, 0.627, 1., 1.]),
        ("edit.placeholder_color", [1., 1., 1., 0.45]),
        ("edit.action_fg_color", [0., 0.94, 1., 1.]),
        ("edit.action_bg_color", [0.1, 0.1, 0.1, 0.9]),
        ("menu.bg_color", [0., 0., 0., 0.5]),
        ("menu.sep_color", [0.41, 0.6, 0.65, 1.]),
        ("menu.role1_color", [0.36, 1., 0.51, 1.]),
        ("menu.role2_color", [0.56, 0.61, 1., 1.]),
        ("chatview.bg_color", [0., 0., 0., 0.]),
        ("chatview.timestamp_color", [0.407, 0.604, 0.647, 1.]),
        ("chatview.text_color", [1., 1., 1., 1.]),
        ("chatview.hi_bg_color", [0., 0.2, 0.2, 1.]),
        ("chatview.action_text_color", [0.5, 0.25, 0.75, 1.]),
        ("chatview.url_text_color", [0., 0.94, 1., 1.]),
    ];
    for (name, val) in tokens {
        ctx.set_shared_token_color(atom, name, *val)?;
    }

    // ------------------------------------------------------------------
    // 2. Private tokens under /theme/scifi
    // ------------------------------------------------------------------
    let token_child = ctx.create_token_child("scifi", private_token_props())?;
    let label_glow = token_child.get_property("label_glow").unwrap();

    // Private-token proof: the netstatus overlay's P2P/OUTBOUND labels
    // follow `label_glow` through journaled edges (removed on unload).
    for label in ["p2p_label", "outbound_label"] {
        let Some(node) =
            ctx.sg_root().lookup_node(&format!("/window/content/chat/netstatus_overlay/{label}"))
        else {
            continue;
        };
        let Some(prop) = node.get_property("text_color") else { continue };
        for i in 0..4 {
            let local = format!("th_label_glow_{i}");
            ctx.depend_journaled(&prop, &label_glow, i, local.clone());
            // The vals expression is installed AND journaled in one
            // operation: unload unsets it, restoring the schema default.
            ctx.set_touched(
                atom,
                &prop,
                i,
                PropertyValue::SExpr(std::sync::Arc::new(expr::load_var(&local))),
            )?;
        }
    }

    // ------------------------------------------------------------------
    // 3. Factory-scrubbed one-offs restored as touched vals: the
    //    "Copied link" overlay foreground (cyan in the shipped look).
    //    Baked vector-art shapes (netstatus icons, overlay panel, menu
    //    header, editbox bg, wallet logos, emoji icon) keep their
    //    original colors in the schema by design (non-goal: shape
    //    internals are not tokenizable), so minimal renders them too.
    // ------------------------------------------------------------------
    if let Some(privmsg_node) =
        ctx.sg_root().lookup_node("/window/content/chat/main_chat_layer/content/chatty/privmsg")
    {
        if let Some(prop) = privmsg_node.get_property("url_copy_fg_color") {
            ctx.set_touched_color(atom, &prop, [0., 0.94, 1., 1.])?;
        }
    }

    // ------------------------------------------------------------------
    // 4. King video background (tracked structural node, reserved low
    //    z band under /window/content)
    // ------------------------------------------------------------------
    king_video_node(ctx).await?;

    // ------------------------------------------------------------------
    // 5. First-run scramble splash (theme content, design D12)
    // ------------------------------------------------------------------

    // ------------------------------------------------------------------
    // 6. Netstatus overlay fade: watch is_visible, animate alpha via
    //    the ctx (touched-set dedups the per-step journal entries).
    //    Replaces the in-handler fade from the reconnect click handler.
    // ------------------------------------------------------------------
    let overlay = ctx
        .sg_root()
        .lookup_node("/window/content/chat/netstatus_overlay")
        .ok_or(Error::NodeNotFound)?;
    let is_visible = overlay.get_property("is_visible").ok_or(Error::PropertyNotFound)?;
    let alpha = overlay.get_property("alpha").ok_or(Error::PropertyNotFound)?;
    let sub = is_visible.subscribe_modify();
    let redraw = ctx.redraw().clone();
    let ctx2 = ThemeCtx::clone_shallow(ctx);
    let fade_task = app.ex.spawn(async move {
        loop {
            let Ok((_, _, guard)) = sub.receive().await else { break };
            drop(guard);
            if !is_visible.get_bool(0).unwrap_or(false) {
                continue;
            }
            let steps = 50;
            // Start from fully transparent so the fade begins hidden
            {
                let atom0 = &mut redraw.make_guard(gfxtag!("netstatus overlay fade"));
                let _ = ctx2.set_touched(atom0, &alpha, 0, PropertyValue::Float32(0.));
            }
            for i in 1..=steps {
                msleep(1000 / steps as u64).await;
                let atom = &mut redraw.make_guard(gfxtag!("netstatus overlay fade"));
                if let Err(e) = ctx2.set_touched(
                    atom,
                    &alpha,
                    0,
                    PropertyValue::Float32(i as f32 / steps as f32),
                ) {
                    tracing::error!(target: "theme::scifi", "fade write failed: {e}");
                    break;
                }
            }
        }
    });
    ctx.push_task(&token_child, fade_task);

    Ok(())
}

/// The king video background, as a tracked structural node.
async fn king_video_node(ctx: &ThemeCtx) -> Result<()> {
    let Some(app) = ctx.app() else { return Err(Error::ThemeNotFound) };
    let content = ctx.sg_root().lookup_node("/window/content").ok_or(Error::NodeNotFound)?;

    let mut cc = Compiler::new();
    let node = create_video("king");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();

    cc.add_const_f32("R", VID_ASPECT_RATIO);
    let prop = node.get_property("uv").unwrap();
    #[rustfmt::skip]
    let code = cc.compile(indoc! {"
        r = w / h;
        if r < R { 0.5 - (r / (2 * R)) } else { 0 }
    "}).unwrap();
    prop.set_default_expr(0, code).unwrap();
    #[rustfmt::skip]
    let code = cc.compile(indoc! {"
        r = w / h;
        if r < R { 0 } else { 0.5 - (R / (2 * r)) }
    "}).unwrap();
    prop.set_default_expr(1, code).unwrap();
    #[rustfmt::skip]
    let code = cc.compile(indoc! {"
        r = w / h;
        if r < R { r / R } else { 1 }
    "}).unwrap();
    prop.set_default_expr(2, code).unwrap();
    #[rustfmt::skip]
    let code = cc.compile(indoc! {"
        r = w / h;
        if r < R { 1 } else { R / r }
    "}).unwrap();
    prop.set_default_expr(3, code).unwrap();

    node.set_property_str(&mut PropertyAtomicGuard::none(), Role::App, "path", VID_PATH).unwrap();
    node.set_property_u32(&mut PropertyAtomicGuard::none(), Role::App, "z_index", 0).unwrap();
    let node = node
        .setup(|me| {
            Video::new(me, app.renderer.clone(), app.redraw_trigger.clone(), app.ex.clone())
        })
        .await;
    ctx.link_tracked(&content, node);
    Ok(())
}
