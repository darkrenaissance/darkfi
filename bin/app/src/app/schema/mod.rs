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

use std::{fs::File, io::Write, sync::atomic::Ordering};

use darkfi::system::msleep;

use indoc::indoc;
use kvdb_overlay::Database as KvDb;

use crate::{
    app::{
        node::{
            create_button, create_layer, create_text, create_text_scramble, create_vector_art,
            create_video,
        },
        App,
    },
    db::AppDbPtr,
    expr::{self, Compiler},
    gfx::gfxtag,
    prop::{PropertyAtomicGuard, PropertyEnum, PropertyFloat32, PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
    sfx, shape,
    ui::{emoji_picker, Button, Layer, Text, TextScramble, VectorArt, VectorShape, Video},
    util::{clipboard, i18n::I18nBabelFish},
};

mod chat;
pub mod menu;
use menu::channel::Channel;
pub mod test;
pub mod test_edit;
pub mod test_scroll_layer;
mod wallet;

macro_rules! i { ($($arg:tt)*) => { info!(target: "app::schema", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "app::schema", $($arg)*); } }

const COLOR_SCHEME: ColorScheme = ColorScheme::DarkMode;
//const COLOR_SCHEME: ColorScheme = ColorScheme::PaperLight;

#[cfg(any(target_os = "android", feature = "emulate-android"))]
mod android_ui_consts {
    pub const NETSTATUS_ICON_SIZE: f32 = 140.;
    pub const SETTINGS_ICON_SIZE: f32 = 140.;
    pub const NETLOGO_SCALE: f32 = 50.;
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 120.;
    pub const EMOJI_PICKER_ICON_MARGIN_X: f32 = 20.;
    pub const EMOJI_PICKER_ICON_MARGIN_Y: f32 = 20.;

    pub const NETSTAT_OVERLAY_MARGIN: f32 = 20.;
    pub const NETSTAT_OVERLAY_BTN_W: f32 = 200.;
    pub const NETSTAT_OVERLAY_BTN_H: f32 = 90.;
    pub const NETSTAT_OVERLAY_BTN_FONTSIZE: f32 = 40.;

    pub const SPLASH_FONTSIZE: f32 = 52.;
    pub const SPLASH_MARGIN: f32 = 40.;

    pub const NETSTAT_OVERLAY_HEIGHT: f32 = 960.;
    pub const NETSTAT_OVERLAY_SEP_X: f32 = 2.;
    pub const NETSTAT_OVERLAY_SEP_Y: f32 = 440.;
    pub const NETSTAT_OVERLAY_SEP_H: f32 = 2.;
    pub const NETSTAT_OVERLAY_OUTLINE_W: f32 = 4.;
    pub const NETSTAT_OVERLAY_TEXT_X: f32 = 100.;
    pub const NETSTAT_OVERLAY_TEXT_MAX: f32 = 4000.;
    pub const NETSTAT_OVERLAY_P2P_LABEL_Y: f32 = 100.;
    pub const NETSTAT_OVERLAY_OUTBOUND_LABEL_Y: f32 = 540.;
    pub const NETSTAT_OVERLAY_CONN_INFO_Y: f32 = 660.;
    pub const NETSTAT_OVERLAY_TOGGLE_NEG_X: f32 = 240.;
    pub const NETSTAT_OVERLAY_TOGGLE_R_PAD: f32 = 40.;
    pub const NETSTAT_OVERLAY_TOGGLE_Y: f32 = 40.;
    pub const NETSTAT_OVERLAY_TOGGLE_W: f32 = 200.;
    pub const NETSTAT_OVERLAY_TOGGLE_H: f32 = 160.;
    pub const NETSTAT_OVERLAY_TOGGLE_OUTLINE_W: f32 = 2.;
    pub const NETSTAT_OVERLAY_TOGGLE_LABEL_Y: f32 = 90.;
    pub const NETSTAT_OVERLAY_TRANSPORT_Y: f32 = 240.;
    pub const NETSTAT_OVERLAY_TRANSPORT_LABEL_Y: f32 = 300.;
    pub const NETSTAT_OVERLAY_TRANSPORT_OPT_LABEL_Y: f32 = 290.;
}

#[cfg(target_os = "android")]
mod ui_consts {
    use crate::android::{get_appdata_path, get_external_storage_path};
    use std::path::PathBuf;

    pub const VID_PATH: &str = "forest_720x1280.mp4";
    pub const VID_ASPECT_RATIO: f32 = 9. / 16.;
    pub use super::android_ui_consts::*;

    pub fn get_chatdb_path() -> PathBuf {
        get_external_storage_path().join("chatdb")
    }

    pub fn get_main_db_path() -> PathBuf {
        get_appdata_path().join("db")
    }

    pub fn get_joined_channels_filename() -> PathBuf {
        get_appdata_path().join("joined.txt")
    }
}

#[cfg(not(target_os = "android"))]
mod desktop_paths {
    use std::path::PathBuf;

    pub const VID_PATH: &str = "assets/forest_1920x1080.ivf";
    pub const VID_ASPECT_RATIO: f32 = 16. / 9.;

    pub fn get_chatdb_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/chatdb")
    }

    pub fn get_main_db_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/db")
    }

    pub fn get_joined_channels_filename() -> PathBuf {
        dirs::cache_dir().unwrap().join("darkfi/app/joined.txt")
    }
}

#[cfg(feature = "emulate-android")]
mod ui_consts {
    pub use super::{android_ui_consts::*, desktop_paths::*};
}

#[cfg(all(
    any(target_os = "linux", target_os = "macos", target_os = "windows"),
    not(feature = "emulate-android")
))]
mod ui_consts {
    pub const NETSTATUS_ICON_SIZE: f32 = 60.;
    pub const SETTINGS_ICON_SIZE: f32 = 60.;
    pub const NETLOGO_SCALE: f32 = 25.;
    pub const EMOJI_PICKER_ICON_SIZE: f32 = 40.;
    pub const EMOJI_PICKER_ICON_MARGIN_X: f32 = 8.;
    pub const EMOJI_PICKER_ICON_MARGIN_Y: f32 = 8.;

    pub const NETSTAT_OVERLAY_MARGIN: f32 = 10.;
    pub const NETSTAT_OVERLAY_BTN_W: f32 = 100.;
    pub const NETSTAT_OVERLAY_BTN_H: f32 = 45.;
    pub const NETSTAT_OVERLAY_BTN_FONTSIZE: f32 = 20.;

    pub const SPLASH_FONTSIZE: f32 = 26.;
    pub const SPLASH_MARGIN: f32 = 20.;

    pub const NETSTAT_OVERLAY_HEIGHT: f32 = 480.;
    pub const NETSTAT_OVERLAY_SEP_X: f32 = 1.;
    pub const NETSTAT_OVERLAY_SEP_Y: f32 = 220.;
    pub const NETSTAT_OVERLAY_SEP_H: f32 = 1.;
    pub const NETSTAT_OVERLAY_OUTLINE_W: f32 = 2.;
    pub const NETSTAT_OVERLAY_TEXT_X: f32 = 50.;
    pub const NETSTAT_OVERLAY_TEXT_MAX: f32 = 2000.;
    pub const NETSTAT_OVERLAY_P2P_LABEL_Y: f32 = 50.;
    pub const NETSTAT_OVERLAY_OUTBOUND_LABEL_Y: f32 = 270.;
    pub const NETSTAT_OVERLAY_CONN_INFO_Y: f32 = 330.;
    pub const NETSTAT_OVERLAY_TOGGLE_NEG_X: f32 = 120.;
    pub const NETSTAT_OVERLAY_TOGGLE_R_PAD: f32 = 20.;
    pub const NETSTAT_OVERLAY_TOGGLE_Y: f32 = 20.;
    pub const NETSTAT_OVERLAY_TOGGLE_W: f32 = 100.;
    pub const NETSTAT_OVERLAY_TOGGLE_H: f32 = 80.;
    pub const NETSTAT_OVERLAY_TOGGLE_OUTLINE_W: f32 = 1.;
    pub const NETSTAT_OVERLAY_TOGGLE_LABEL_Y: f32 = 45.;
    pub const NETSTAT_OVERLAY_TRANSPORT_Y: f32 = 120.;
    pub const NETSTAT_OVERLAY_TRANSPORT_LABEL_Y: f32 = 150.;
    pub const NETSTAT_OVERLAY_TRANSPORT_OPT_LABEL_Y: f32 = 145.;

    pub use super::desktop_paths::*;
}

pub use ui_consts::*;

pub static DEFAULT_CHANNELS: &'static [&str] =
    &["dev", "media", "hackers", "memes", "philosophy", "markets", "math", "random"];

#[derive(PartialEq)]
enum ColorScheme {
    DarkMode,
    PaperLight,
}

/// Read the ordered list of joined channels/contacts (prefixed names like "#dev", "@alice").
pub fn read_joined_channels() -> Vec<String> {
    let Ok(contents) = std::fs::read_to_string(get_joined_channels_filename()) else {
        return vec![]
    };

    let mut joined = vec![];
    for line in contents.lines() {
        let line = line.trim();
        assert!(!line.is_empty());
        joined.push(line.to_string());
    }
    joined
}

/// First-run seed: write DEFAULT_CHANNELS (as "#name") if no joined file exists.
/// Idempotent and deterministic, so it is safe to call from both schema startup and the plugin.
pub fn ensure_joined_channels_seeded() {
    let path = get_joined_channels_filename();
    if path.exists() {
        return
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let defaults: Vec<String> = DEFAULT_CHANNELS.iter().map(|c| format!("#{}", c)).collect();
    let _ = std::fs::write(&path, defaults.join("\n"));
}

/// Append a single channel line to the joined file (no dedup). Ensures newline separation
/// when appending to a non-empty file that lacks a trailing newline.
pub fn write_joined_channel(name: &str) {
    let path = get_joined_channels_filename();
    let needs_newline = std::fs::read(&path)
        .map(|bytes| !bytes.is_empty() && !bytes.ends_with(b"\n"))
        .unwrap_or(false);
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
    if needs_newline {
        writeln!(f).unwrap();
    }
    writeln!(f, "{}", name).unwrap();
}

/// Join a channel: skip if already joined, otherwise append one line.
pub fn append_joined_channel(name: &str) {
    if read_joined_channels().iter().any(|x| x == name) {
        return
    }
    write_joined_channel(name);
}

/// Rewrite the whole joined list (used for first-run seed and main-menu edit_done sync).
pub fn write_joined_channels(items: &[String]) {
    let _ = std::fs::write(get_joined_channels_filename(), items.join("\n"));
}

pub async fn make(
    app: &App,
    window: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    kv_db: KvDb,
    app_db: AppDbPtr,
) {
    let mut cc = Compiler::new();
    cc.add_const_f32("NETSTATUS_ICON_SIZE", NETSTATUS_ICON_SIZE);
    cc.add_const_f32("SETTINGS_ICON_SIZE", SETTINGS_ICON_SIZE);
    cc.add_const_f32("NETSTAT_OVERLAY_MARGIN", NETSTAT_OVERLAY_MARGIN);
    cc.add_const_f32("NETSTAT_OVERLAY_BTN_W", NETSTAT_OVERLAY_BTN_W);
    cc.add_const_f32("NETSTAT_OVERLAY_BTN_H", NETSTAT_OVERLAY_BTN_H);
    cc.add_const_f32("NETSTAT_OVERLAY_TOGGLE_NEG_X", NETSTAT_OVERLAY_TOGGLE_NEG_X);
    cc.add_const_f32("NETSTAT_OVERLAY_TOGGLE_R_PAD", NETSTAT_OVERLAY_TOGGLE_R_PAD);
    cc.add_const_f32("NETSTAT_OVERLAY_TOGGLE_W", NETSTAT_OVERLAY_TOGGLE_W);

    let atom = &mut PropertyAtomicGuard::none();

    let window_scale =
        PropertyFloat32::wrap(&app.sg_root.lookup_node("/window").unwrap(), Role::App, "scale", 0)
            .unwrap();

    // Root content layer
    let content = create_layer("content");
    let prop = content.get_property("rect").unwrap();
    prop.set_expr(atom, Role::App, 0, expr::load_var("insets_left")).unwrap();
    prop.set_expr(atom, Role::App, 1, expr::load_var("insets_top")).unwrap();
    let code = cc.compile("w - insets_left - insets_right").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    let code = cc.compile("h - insets_top - insets_bottom").unwrap();
    prop.set_expr(atom, Role::App, 3, code).unwrap();
    let window_insets = window.get_property("insets").unwrap();
    prop.add_depend(&window_insets, 0, "insets_left");
    prop.add_depend(&window_insets, 1, "insets_top");
    prop.add_depend(&window_insets, 2, "insets_right");
    prop.add_depend(&window_insets, 3, "insets_bottom");
    content.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    content.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let content =
        content.setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    window.link(content.clone());

    // Splash layer with the scramble message, shown on the first run of
    // a new app version
    if app.is_first_time.load(Ordering::Relaxed) {
        cc.add_const_f32("SPLASH_FONTSIZE", SPLASH_FONTSIZE);
        cc.add_const_f32("SPLASH_MARGIN", SPLASH_MARGIN);
        let splash_layer = create_layer("splash_layer");
        let prop = splash_layer.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
        splash_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        splash_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
        let splash_layer = splash_layer
            .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
        content.link(splash_layer.clone());

        let node = create_text_scramble("splash_msg");
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, SPLASH_MARGIN).unwrap();
        let code = cc.compile("h * 0.4").unwrap();
        prop.set_expr(atom, Role::App, 1, code).unwrap();
        let code = cc.compile("w - 2 * SPLASH_MARGIN").unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        prop.set_f32(atom, Role::App, 3, SPLASH_FONTSIZE * 1.2).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        node.set_property_f32(atom, Role::App, "font_size", SPLASH_FONTSIZE).unwrap();
        node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
        let prop = node.get_property("text_color").unwrap();
        prop.set_f32(atom, Role::App, 0, 1.).unwrap();
        prop.set_f32(atom, Role::App, 1, 1.).unwrap();
        prop.set_f32(atom, Role::App, 2, 1.).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
        let prop = node.get_property("scramble_color").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
        node.set_property_str(atom, Role::App, "text", "welcome back commander").unwrap();
        node.set_property_f32(atom, Role::App, "solve_probability", 0.06).unwrap();
        let node = node
            .setup(|me| {
                TextScramble::new(
                    me,
                    window_scale.clone(),
                    app.renderer.clone(),
                    i18n_fish.clone(),
                    app.redraw_trigger.clone(),
                )
            })
            .await;
        splash_layer.link(node);

        // Hide the splash layer after 3s
        let redraw = app.redraw_trigger.clone();
        let hide_task = app.ex.spawn(async move {
            msleep(5000).await;
            let atom = &mut redraw.make_guard(gfxtag!("splash_layer hide"));
            splash_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
        });
        app.tasks.lock().unwrap().push(hide_task);
    }

    if COLOR_SCHEME == ColorScheme::DarkMode {
        let node = create_video("king");
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();

        cc.add_const_f32("R", VID_ASPECT_RATIO);

        let prop = node.get_property("uv").unwrap();
        #[rustfmt::skip]
        let code = cc.compile(indoc! {"
            r = w / h;
            # r < R means screen narrower than image
            if r < R {
                0.5 - (r / (2 * R))
            } else {
                0
            }
        "}).unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        #[rustfmt::skip]
        let code = cc.compile(indoc! {"
            r = w / h;
            if r < R {
                0
            } else {
                0.5 - (R / (2 * r))
            }
        "}).unwrap();
        prop.set_expr(atom, Role::App, 1, code).unwrap();
        #[rustfmt::skip]
        let code = cc.compile(indoc! {"
            r = w / h;
            if r < R {
                r / R
            } else {
                1
            }
        "}).unwrap();
        prop.set_expr(atom, Role::App, 2, code).unwrap();
        #[rustfmt::skip]
        let code = cc.compile(indoc! {"
            r = w / h;
            if r < R {
                1
            } else {
                R / r
            }
        "}).unwrap();
        prop.set_expr(atom, Role::App, 3, code).unwrap();

        //node.set_property_str(atom, Role::App, "path", BG_PATH).unwrap();
        node.set_property_str(atom, Role::App, "path", VID_PATH).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
        //let node = node.setup(|me| Image::new(me, app.renderer.clone())).await;
        //layer_node.link(node);
        let node = node
            .setup(|me| {
                Video::new(me, app.renderer.clone(), app.redraw_trigger.clone(), app.ex.clone())
            })
            .await;
        content.link(node);
    } else if COLOR_SCHEME == ColorScheme::PaperLight {
        let node = create_vector_art("bg");
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.).unwrap();
        prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
        prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

        let c = 1.;
        // Setup the pimpl
        let mut shape = VectorShape::new();
        shape.add_filled_box(
            expr::const_f32(0.),
            expr::const_f32(0.),
            expr::load_var("w"),
            expr::load_var("h"),
            [c, c, c, 0.3],
        );
        node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
        let node = node
            .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
        window.link(node);
    }

    let emoji_meshes = emoji_picker::EmojiMeshes::new(app.renderer.clone(), EMOJI_PICKER_ICON_SIZE);

    emoji_meshes.clone().start_make();

    // Initialize default channels if the table is empty
    if app_db.channels().await.expect("cannot read channels").is_empty() {
        for channel_name in DEFAULT_CHANNELS {
            let channel = Channel { name: channel_name.to_string(), secret: None };
            app_db.channel_insert(&channel).await.expect("cannot seed channel");
        }
    }

    // Seed the joined-channels file with defaults on first run.
    ensure_joined_channels_seeded();

    // Create chat container layer
    let chat_layer = create_layer("chat");
    let prop = chat_layer.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    chat_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    chat_layer.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let chat_layer = chat_layer
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    content.link(chat_layer.clone());

    let netlayer_node = create_layer("netstatus_layer");
    let prop = netlayer_node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTATUS_ICON_SIZE").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    //prop.set_f32(atom, Role::App, 2, NETSTATUS_ICON_SIZE).unwrap();
    //prop.set_f32(atom, Role::App, 3, NETSTATUS_ICON_SIZE).unwrap();
    prop.set_f32(atom, Role::App, 2, 1000.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1000.).unwrap();
    netlayer_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    netlayer_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    netlayer_node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let netlayer_node = netlayer_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    chat_layer.link(netlayer_node.clone());

    let node = create_vector_art("net0");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_netlogo1([1., 0., 0.25, 1.]);
    shape.join(shape::create_netlogo2([0.27, 0.4, 0.4, 1.]));
    shape.join(shape::create_netlogo3([0.27, 0.4, 0.4, 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net0_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
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
    let mut shape = shape::create_netlogo1([0.49, 0.57, 1., 1.]);
    shape.join(shape::create_netlogo2([0.49, 0.57, 1., 1.]));
    shape.join(shape::create_netlogo3([0.27, 0.4, 0.4, 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net1_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
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
    let mut shape = shape::create_netlogo1([0., 0.94, 1., 1.]);
    shape.join(shape::create_netlogo2([0., 0.94, 1., 1.]));
    shape.join(shape::create_netlogo3([0., 0.94, 1., 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net2_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(net2_node);

    let node = create_vector_art("net3");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE / 2.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    node.set_property_f32(atom, Role::App, "scale", NETLOGO_SCALE).unwrap();
    let mut shape = shape::create_netlogo1([0., 0.94, 1., 1.]);
    shape.join(shape::create_netlogo2([0., 0.94, 1., 1.]));
    shape.join(shape::create_netlogo3([0., 0.94, 1., 1.]));
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let net3_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(net3_node);

    // netstat-klik icon (visual feedback when reconnect button is clicked)
    let klik_color = [0., 0.5, 1., 1.]; // Blue
    let node = create_vector_art("netstat_klik");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTATUS_ICON_SIZE).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTATUS_ICON_SIZE).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    // Above other icons
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::const_f32(NETSTATUS_ICON_SIZE),
        expr::const_f32(NETSTATUS_ICON_SIZE),
        klik_color,
    );
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let netstat_klik_node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(netstat_klik_node.clone());

    // Reconnect Button (overlaid on netstatus icons)
    let node = create_button("reconnect_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTATUS_ICON_SIZE).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTATUS_ICON_SIZE).unwrap();

    let sg_root = app.sg_root.clone();
    let redraw = app.redraw_trigger.clone();
    let ex = app.ex.clone();
    let ex_fade = app.ex.clone();
    let (slot, recvr) = Slot::new("reconnect_clicked");
    node.register("click", slot).unwrap();
    let reconnect_task = ex.spawn(async move {
        let mut _fade_task = None;
        let mut _conn_info_task = None;
        while let Ok(_) = recvr.recv().await {
            i!("Reconnect button clicked");

            // Toggle the overlay layer
            let overlay = sg_root.lookup_node("/window/content/chat/netstatus_overlay").unwrap();
            let is_visible = overlay.get_property_bool("is_visible").unwrap();
            if !is_visible {
                sfx::play_cloak();
            }
            let atom = &mut redraw.make_guard(gfxtag!("netstatus overlay toggle"));
            overlay.set_property_bool(atom, Role::App, "is_visible", !is_visible).unwrap();

            if !is_visible {
                // Start from fully transparent so the fade begins hidden
                overlay.set_property_f32(atom, Role::App, "alpha", 0.).unwrap();

                // While the overlay is shown, keep the conn_info text in sync
                // with the darkirc outbound peers
                let sg_root2 = sg_root.clone();
                let redraw2 = redraw.clone();
                _conn_info_task = Some(ex_fade.spawn(async move {
                    let Some(darkirc) = sg_root2.lookup_node("/plugin/darkirc") else {
                        e!("DarkIrc plugin has not been loaded");
                        return
                    };
                    let conn_info = sg_root2
                        .lookup_node("/window/content/chat/netstatus_overlay/conn_info")
                        .unwrap();
                    let conn_info_text =
                        PropertyStr::wrap(&conn_info, Role::App, "text", 0).unwrap();
                    let outbound_peers = darkirc.get_property("outbound_peers").unwrap();
                    let outbound_peers_sub = outbound_peers.subscribe_modify();

                    loop {
                        let mut lines = vec![];
                        for idx in 0..outbound_peers.get_len() {
                            match outbound_peers.get_str_opt(idx) {
                                Ok(Some(url)) => lines.push(format!("{idx}  {url}")),
                                _ => lines.push(format!("{idx}  sleeping")),
                            }
                        }

                        let atom = &mut redraw2.make_guard(gfxtag!("conn_info update"));
                        conn_info_text.set(atom, lines.join("\n"));

                        let Ok(_) = outbound_peers_sub.receive().await else { break };
                    }
                }));

                // Fade the overlay alpha from 0 to 1 over 1s
                let overlay = overlay.clone();
                let redraw = redraw.clone();
                _fade_task = Some(ex_fade.spawn(async move {
                    let steps = 50;
                    for i in 1..=steps {
                        msleep(1000 / steps as u64).await;
                        let atom = &mut redraw.make_guard(gfxtag!("netstatus overlay fade"));
                        overlay
                            .set_property_f32(atom, Role::App, "alpha", i as f32 / steps as f32)
                            .unwrap();
                    }
                }));
            } else {
                // Hiding cancels any in-flight fade and the conn_info listener
                _fade_task = None;
                _conn_info_task = None;
            }
        }
    });
    app.tasks.lock().unwrap().push(reconnect_task);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    netlayer_node.link(node);

    // Overlay layer toggled by the netstatus logo. Sits on top of everything
    // except the header strip, so the logo stays visible and clickable.
    let overlay_node = create_layer("netstatus_overlay");
    let prop = overlay_node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTAT_OVERLAY_MARGIN).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTATUS_ICON_SIZE + NETSTAT_OVERLAY_MARGIN).unwrap();
    let code = cc.compile("w - 2 * NETSTAT_OVERLAY_MARGIN").unwrap();
    prop.set_expr(atom, Role::App, 2, code).unwrap();
    //let code = cc.compile("h - NETSTATUS_ICON_SIZE - 2 * NETSTAT_OVERLAY_MARGIN").unwrap();
    //prop.set_expr(atom, Role::App, 3, code).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_HEIGHT).unwrap();
    overlay_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    overlay_node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    overlay_node.set_property_u32(atom, Role::App, "priority", 2).unwrap();
    let overlay_node = overlay_node
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    chat_layer.link(overlay_node.clone());

    // Placeholder single-color background filling the whole overlay
    let node = create_vector_art("overlay_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0., 0.1, 0.1, 0.7],
    );
    shape.add_filled_box(
        expr::const_f32(NETSTAT_OVERLAY_SEP_X),
        expr::const_f32(NETSTAT_OVERLAY_SEP_Y),
        expr::load_var("w"),
        expr::const_f32(NETSTAT_OVERLAY_SEP_Y + NETSTAT_OVERLAY_SEP_H),
        [0., 0.94, 1., 1.],
    );
    shape.add_outline(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        NETSTAT_OVERLAY_OUTLINE_W,
        [0., 0.94, 1., 1.],
    );
    shape.add_filled_box(
        cc.compile("w - NETSTAT_OVERLAY_TOGGLE_NEG_X").unwrap(),
        expr::const_f32(NETSTAT_OVERLAY_TOGGLE_Y),
        cc.compile("w - NETSTAT_OVERLAY_TOGGLE_R_PAD").unwrap(),
        expr::const_f32(NETSTAT_OVERLAY_TOGGLE_Y + NETSTAT_OVERLAY_TOGGLE_H),
        [0., 0.12, 0.08, 1.],
    );
    shape.add_outline(
        cc.compile("w - NETSTAT_OVERLAY_TOGGLE_NEG_X").unwrap(),
        expr::const_f32(NETSTAT_OVERLAY_TOGGLE_Y),
        cc.compile("w - NETSTAT_OVERLAY_TOGGLE_R_PAD").unwrap(),
        expr::const_f32(NETSTAT_OVERLAY_TOGGLE_Y + NETSTAT_OVERLAY_TOGGLE_H),
        NETSTAT_OVERLAY_TOGGLE_OUTLINE_W,
        [0.08, 0.68, 0.72, 1.],
    );
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    overlay_node.link(node);

    let node = create_text("p2p_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTAT_OVERLAY_TEXT_X).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_P2P_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", NETSTAT_OVERLAY_BTN_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "P2P").unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "left").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.47).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.75).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
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
    overlay_node.link(node);

    let node = create_text("toggle_label");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTAT_OVERLAY_TOGGLE_NEG_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_TOGGLE_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TOGGLE_W).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", NETSTAT_OVERLAY_BTN_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "on").unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.08).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.68).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.72).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
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
    let toggle_text = PropertyStr::wrap(&node, Role::App, "text", 0).unwrap();
    let setting_node = app.sg_root.lookup_node("/setting").unwrap();
    let chat_is_enabled = setting_node.get_property("chat.is_enabled").unwrap();
    if !chat_is_enabled.get_bool(0).unwrap() {
        toggle_text.set(atom, "off");
    }
    overlay_node.link(node);

    // Create the p2p toggle button
    let node = create_button("p2p_toggle_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("w - NETSTAT_OVERLAY_TOGGLE_NEG_X").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_TOGGLE_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TOGGLE_W).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TOGGLE_H).unwrap();
    let (slot, recvr) = Slot::new("toggle_p2p");
    node.register("click", slot).unwrap();
    let redraw = app.redraw_trigger.clone();
    let listen_click = ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let is_enabled = chat_is_enabled.get_bool(0).unwrap();
            i!("toggle_p2p from {is_enabled} to {}", !is_enabled);
            let atom = &mut redraw.make_guard(gfxtag!("toggle_p2p"));
            if is_enabled {
                toggle_text.set(atom, "off");
            } else {
                toggle_text.set(atom, "on");
            }
            chat_is_enabled.set_bool(atom, Role::User, 0, !is_enabled).unwrap();
        }
    });
    overlay_node.push_task(listen_click);
    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    overlay_node.link(node);

    let node = create_text("transport_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTAT_OVERLAY_TEXT_X).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_TRANSPORT_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", NETSTAT_OVERLAY_BTN_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "Transport").unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "left").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.47).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.75).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
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
    overlay_node.link(node);

    // Transport selector: one segment per option, the filled toggle is moved
    // onto the selected one
    let transport_opts = ["tcp", "tor"];
    let net_transport = PropertyEnum::wrap(&setting_node, Role::User, "net.transport", 0).unwrap();
    let transport_selected = net_transport.get();
    let mut transport_sel_nodes = vec![];
    for (idx, opt) in transport_opts.iter().enumerate() {
        let node = create_vector_art(&format!("transport_sel_{opt}"));
        let prop = node.get_property("rect").unwrap();
        let code = cc
            .compile(&format!(
                "w - NETSTAT_OVERLAY_TOGGLE_R_PAD - {} * NETSTAT_OVERLAY_TOGGLE_W",
                transport_opts.len() - idx
            ))
            .unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_TRANSPORT_Y).unwrap();
        prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TOGGLE_W).unwrap();
        prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TOGGLE_H).unwrap();
        node.set_property_bool(atom, Role::App, "is_visible", *opt == transport_selected).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();
        let mut shape = VectorShape::new();
        shape.add_filled_box(
            expr::const_f32(0.),
            expr::const_f32(0.),
            expr::load_var("w"),
            expr::load_var("h"),
            [0., 0.12, 0.08, 1.],
        );
        shape.add_outline(
            expr::const_f32(0.),
            expr::const_f32(0.),
            expr::load_var("w"),
            expr::load_var("h"),
            NETSTAT_OVERLAY_TOGGLE_OUTLINE_W,
            [0.08, 0.68, 0.72, 1.],
        );
        node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
        let node = node
            .setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
        overlay_node.link(node.clone());
        transport_sel_nodes.push(node);
    }

    for (idx, opt) in transport_opts.iter().enumerate() {
        let node = create_text(&format!("transport_opt_{opt}"));
        let prop = node.get_property("rect").unwrap();
        let code = cc
            .compile(&format!(
                "w - NETSTAT_OVERLAY_TOGGLE_R_PAD - {} * NETSTAT_OVERLAY_TOGGLE_W",
                transport_opts.len() - idx
            ))
            .unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_TRANSPORT_OPT_LABEL_Y).unwrap();
        prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TOGGLE_W).unwrap();
        prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
        node.set_property_f32(atom, Role::App, "font_size", NETSTAT_OVERLAY_BTN_FONTSIZE).unwrap();
        node.set_property_str(atom, Role::App, "text", *opt).unwrap();
        node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
        let prop = node.get_property("text_color").unwrap();
        prop.set_f32(atom, Role::App, 0, 0.08).unwrap();
        prop.set_f32(atom, Role::App, 1, 0.68).unwrap();
        prop.set_f32(atom, Role::App, 2, 0.72).unwrap();
        prop.set_f32(atom, Role::App, 3, 1.).unwrap();
        node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
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
        overlay_node.link(node);
    }

    for (idx, opt) in transport_opts.iter().enumerate() {
        let node = create_button(&format!("transport_btn_{opt}"));
        node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
        let prop = node.get_property("rect").unwrap();
        let code = cc
            .compile(&format!(
                "w - NETSTAT_OVERLAY_TOGGLE_R_PAD - {} * NETSTAT_OVERLAY_TOGGLE_W",
                transport_opts.len() - idx
            ))
            .unwrap();
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_TRANSPORT_Y).unwrap();
        prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TOGGLE_W).unwrap();
        prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TOGGLE_H).unwrap();
        let (slot, recvr) = Slot::new(&format!("transport_select_{opt}"));
        node.register("click", slot).unwrap();
        let redraw = app.redraw_trigger.clone();
        let sel_nodes = transport_sel_nodes.clone();
        let net_transport = net_transport.clone();
        let opt = *opt;
        let listen_click = ex.spawn(async move {
            while let Ok(_) = recvr.recv().await {
                i!("transport_select_{opt}");
                let atom = &mut redraw.make_guard(gfxtag!("transport_select"));
                for (j, sel_node) in sel_nodes.iter().enumerate() {
                    sel_node.set_property_bool(atom, Role::App, "is_visible", j == idx).unwrap();
                }
                net_transport.set(atom, opt);
            }
        });
        overlay_node.push_task(listen_click);
        let node = node
            .setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
            .await;
        overlay_node.link(node);
    }

    let node = create_text("outbound_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTAT_OVERLAY_TEXT_X).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_OUTBOUND_LABEL_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", NETSTAT_OVERLAY_BTN_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "OUTBOUND").unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "left").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.47).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.75).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
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
    overlay_node.link(node);

    let node = create_text("conn_info");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, NETSTAT_OVERLAY_TEXT_X).unwrap();
    prop.set_f32(atom, Role::App, 1, NETSTAT_OVERLAY_CONN_INFO_Y).unwrap();
    prop.set_f32(atom, Role::App, 2, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    prop.set_f32(atom, Role::App, 3, NETSTAT_OVERLAY_TEXT_MAX).unwrap();
    node.set_property_f32(atom, Role::App, "font_size", NETSTAT_OVERLAY_BTN_FONTSIZE).unwrap();
    #[cfg(not(feature = "enable-plugin-darkirc"))]
    node.set_property_str(
        atom,
        Role::App,
        "text",
        indoc! {"
            0  tcp+tls://dasman.xyz:9600
            1  tcp+tls://dasman.xyz:9600
            2  tcp+tls://dasman.xyz:9600
        "},
    )
    .unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "left").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
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
    overlay_node.link(node);

    menu::make(app, chat_layer.clone(), i18n_fish, app_db.clone(), &kv_db, emoji_meshes.clone())
        .await;

    // Create chat layers only for joined channels/contacts, in joined order.
    for name in read_joined_channels() {
        let bare = if name.starts_with('#') || name.starts_with('@') { &name[1..] } else { &name };
        let in_db = match name.chars().next() {
            Some('#') => app_db.channel_get(bare).await.ok().flatten().is_some(),
            Some('@') => app_db.contact_get(bare).await.ok().flatten().is_some(),
            _ => false,
        };
        if !in_db {
            warn!(target: "app::schema", "Joined entry '{name}' not found in kv_db; skipping");
            continue
        }

        chat::make(
            &app.sg_root,
            &app.renderer,
            &app.ex,
            chat_layer.clone(),
            &name,
            &kv_db,
            i18n_fish,
            emoji_meshes.clone(),
            app.redraw_trigger.clone(),
        )
        .await;
    }

    wallet::make(app, content.clone(), i18n_fish).await;

    // Setup wallet button after wallet layer is created
    menu::setup_wallet_button(app, chat_layer, i18n_fish).await;

    // @@@ Debug stuff @@@
    //let chatview_node = app.sg_root.lookup_node("/window/content/chat/dev_chat_layer").unwrap();
    //chatview_node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    //let menu_node = app.sg_root.lookup_node("/window/content/chat/menu_layer").unwrap();
    //menu_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
}
