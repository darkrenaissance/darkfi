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

//! Dev schema hosting one chatview node, selected by the
//! `schema-test-chatview` cargo feature. Development and live-testing
//! screen for the chatview rework until the cutover phases rework the
//! real chat screen.

use crate::{
    app::{
        node::{create_button, create_chatview, create_layer, create_vector_art},
        App,
    },
    expr::{self, Compiler},
    gfx::gfxtag,
    mesh::rgba,
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{Pimpl, SceneNodePtr, Slot},
    shape,
    ui::{Button, ChatView, Layer, VectorArt},
    util::i18n::I18nBabelFish,
};
use darkfi_serial::Encodable;
use kvdb_overlay::{Database as KvDb, Tree};

#[cfg(target_os = "android")]
mod ui_consts {
    use crate::android::get_appdata_path;
    use std::path::PathBuf;

    pub fn get_chatdb_path() -> PathBuf {
        get_appdata_path().join("chatdb2")
    }
}

#[cfg(not(target_os = "android"))]
mod ui_consts {
    use std::path::PathBuf;

    pub fn get_chatdb_path() -> PathBuf {
        PathBuf::from("chatdb2")
    }
}

use ui_consts::*;

/// The channel the dev screen binds.
const DEV_CHANNEL: &str = "dev";

pub async fn make(app: &App, window: SceneNodePtr, i18n_fish: &I18nBabelFish) {
    let atom = &mut PropertyAtomicGuard::none();
    let cc = Compiler::new();

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

    let node = create_chatview("chatview");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_expr(atom, Role::App, 2, expr::load_var("w")).unwrap();
    prop.set_expr(atom, Role::App, 3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    node.set_property_f32(atom, Role::App, "font_size", 20.).unwrap();
    node.set_property_f32(atom, Role::App, "timestamp_font_size", 10.).unwrap();
    node.set_property_f32(atom, Role::App, "timestamp_width", 80.).unwrap();
    node.set_property_f32(atom, Role::App, "line_height", 30.).unwrap();
    node.set_property_f32(atom, Role::App, "message_spacing", 6.).unwrap();
    node.set_property_f32(atom, Role::App, "baseline", 20.).unwrap();

    let prop = node.get_property("timestamp_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 1.).unwrap();
    prop.set_f32(atom, Role::App, 1, 1.).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = node.get_property("hi_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.2).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.2).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let kv_db = KvDb::open_default(&get_chatdb_path()).expect("cannot open chatdb2");

    // Seed the dev channel's tree on first run so there is history to load.
    let dev_tree = kv_db
        .open_tree_default(&ChatView::tree_name(DEV_CHANNEL))
        .expect("cannot open dev chat tree");
    if dev_tree.is_empty().expect("cannot read dev chat tree") {
        populate_tree(&dev_tree);
    }
    drop(dev_tree);

    let window_scale =
        PropertyFloat32::wrap(&app.sg_root.lookup_node("/window").unwrap(), Role::App, "scale", 0)
            .unwrap();
    let chatview_node = node
        .setup(|me| {
            ChatView::new(
                me,
                kv_db,
                window_scale.clone(),
                i18n_fish.clone(),
                app.renderer.clone(),
                app.redraw_trigger.clone(),
                app.ex.clone(),
            )
        })
        .await;
    layer_node.link(chatview_node.clone());
    let chatview_node_for_bind = chatview_node.clone();

    // Type-specific styling lives on the privmsg type sub-node.
    let privmsg_node = app
        .sg_root
        .lookup_node("/window/view/chatview/privmsg")
        .expect("privmsg type node not linked");
    let prop = privmsg_node.get_property("action_text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.25).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.75).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = privmsg_node.get_property("url_text_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.94).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    let prop = privmsg_node.get_property("url_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.13).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.08).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_bg_border_size", 1.).unwrap();
    let prop = privmsg_node.get_property("url_bg_border_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.11).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.6).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.63).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    // "Copied link" overlay styling (mirrors the edit action menu).
    let prop = privmsg_node.get_property("url_copy_fg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.94).unwrap();
    prop.set_f32(atom, Role::App, 2, 1.).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
    let prop = privmsg_node.get_property("url_copy_bg_color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.1).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.1).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.1).unwrap();
    prop.set_f32(atom, Role::App, 3, 0.9).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_copy_font_size", 20.).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_copy_padding", 8.).unwrap();
    privmsg_node.set_property_f32(atom, Role::App, "url_copy_offset", 8.).unwrap();

    privmsg_node.set_property_f32(atom, Role::App, "cap_max_height", 300.).unwrap();

    let prop = privmsg_node.get_property("nick_colors").unwrap();
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
        prop.push_f32(atom, Role::App, c).unwrap();
    }

    // Date-separator styling.
    let datemsg_node = app
        .sg_root
        .lookup_node("/window/view/chatview/datemsg")
        .expect("datemsg type node not linked");
    let prop = datemsg_node.get_property("color").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
    prop.set_f32(atom, Role::App, 3, 1.).unwrap();

    // Relay the filemsg node's signals to the fud plugin when loaded.
    let filemsg_node = app
        .sg_root
        .lookup_node("/window/view/chatview/filemsg")
        .expect("filemsg type node not linked");

    let (slot, recvr) = Slot::new("fileurl_detect");
    filemsg_node.register("fileurl_detected", slot).unwrap();
    let sg_root2 = app.sg_root.clone();
    let listen_fileurl = app.ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            if let Some(fud_node) = sg_root2.lookup_node("/plugin/fud") {
                let _ = fud_node.call_method("track_file", data).await;
            }
        }
    });
    filemsg_node.push_task(listen_fileurl);

    let (slot, recvr) = Slot::new("file_download");
    filemsg_node.register("download_request", slot).unwrap();
    let sg_root2 = app.sg_root.clone();
    let listen_download = app.ex.spawn(async move {
        use crate::ui::chatview::MessageId;
        use darkfi_serial::{Decodable, Encodable};
        while let Ok(data) = recvr.recv().await {
            // The payload carries (id, url); the fud plugin takes the url.
            let mut cur = std::io::Cursor::new(&data);
            let Ok(_id) = MessageId::decode(&mut cur) else { continue };
            let Ok(url) = url::Url::decode(&mut cur) else { continue };
            if let Some(fud_node) = sg_root2.lookup_node("/plugin/fud") {
                let mut fud_data = vec![];
                url.encode(&mut fud_data).unwrap();
                let _ = fud_node.call_method("get", fud_data).await;
            }
        }
    });
    filemsg_node.push_task(listen_download);

    let bind_task = app.ex.spawn(async move {
        // Over the method bus once start() has subscribed; the delay
        // covers the setup -> start gap so the call isn't dropped.
        darkfi::system::sleep(1).await;
        let mut data = vec![];
        DEV_CHANNEL.encode(&mut data).unwrap();
        let _ = chatview_node_for_bind.call_method("set_channel", data).await;
    });
    app.tasks.lock().unwrap().push(bind_task);

    // Scroll-to-bottom arrow: floats over the chatview, so it carries
    // priority above it for the gesture session's ordered targeting.
    let down_layer = create_layer("chat_down_arrow");
    let prop = down_layer.get_property("rect").unwrap();
    let code = cc.compile("w - 120").unwrap();
    prop.set_expr(atom, Role::App, 0, code).unwrap();
    let code = cc.compile("h - 160").unwrap();
    prop.set_expr(atom, Role::App, 1, code).unwrap();
    prop.set_f32(atom, Role::App, 2, 100.).unwrap();
    prop.set_f32(atom, Role::App, 3, 100.).unwrap();
    down_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    down_layer.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    down_layer.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let down_layer = down_layer
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    layer_node.link(down_layer.clone());

    let down_layer_is_visible =
        PropertyBool::wrap(&down_layer, Role::App, "is_visible", 0).unwrap();
    let chatview_at_bottom =
        PropertyBool::wrap(&chatview_node, Role::App, "is_at_bottom", 0).unwrap();
    let chatview_at_bottom_sub = chatview_at_bottom.prop().subscribe_modify();
    let redraw2 = app.redraw_trigger.clone();
    let monitor_task = app.ex.spawn(async move {
        while let Ok(_) = chatview_at_bottom_sub.receive().await {
            let atom = &mut redraw2.make_guard(gfxtag!("down arrow visibility change"));
            down_layer_is_visible.set(atom, !chatview_at_bottom.get());
        }
    });
    down_layer.push_task(monitor_task);

    let node = create_vector_art("downbg");
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 100.).unwrap();
    prop.set_f32(atom, Role::App, 3, 100.).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut arrow_shape =
        shape::create_down_bgtab(rgba!(0x003232ff), rgba!(0x294f60ff), 0.1).scaled(0.2);
    arrow_shape.join(shape::create_down_arrow(rgba!(0x00f0ffff), 1.));
    node.set_property_shape(atom, Role::App, "shape", arrow_shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    down_layer.link(node);

    let node = create_button("scroll_bottom_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_f32(atom, Role::App, 0, 0.).unwrap();
    prop.set_f32(atom, Role::App, 1, 0.).unwrap();
    prop.set_f32(atom, Role::App, 2, 100.).unwrap();
    prop.set_f32(atom, Role::App, 3, 100.).unwrap();
    let (slot, recvr) = Slot::new("scroll_bottom");
    node.register("click", slot).unwrap();
    let chatview_node2 = chatview_node.clone();
    let redraw2 = app.redraw_trigger.clone();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let _ = chatview_node2.call_method("scroll_to_bottom", vec![]).await;
            let _ = redraw2;
        }
    });
    down_layer.push_task(listen_click);
    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    down_layer.link(node);
}

/// Seed the dev channel's tree with test messages (in the v2 tagged
/// format), so the screen has something to load. Copied from the old
/// chat schema's fixture, re-encoded through the chatview codec.
fn populate_tree(tree: &Tree) {
    use crate::ui::chatview::{codec, MessageId, MsgType};
    use chrono::{NaiveDate, NaiveDateTime};

    let chat_txt = include_str!("../../../data/chat.txt");
    for (idx, line) in chat_txt.lines().enumerate() {
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        assert_eq!(parts.len(), 3);
        let time_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
        let (hour, min) = (time_parts[0], time_parts[1]);
        let hour = hour.parse::<u32>().unwrap();
        let min = min.parse::<u32>().unwrap();
        let dt: NaiveDateTime =
            NaiveDate::from_ymd_opt(2024, 8, 6).unwrap().and_hms_opt(hour, min, 0).unwrap();
        let timest = dt.and_utc().timestamp_millis() as u64;

        let nick = parts[1].to_string();
        let text = parts[2].to_string();

        // Unique id per line (the minute timestamp alone can repeat).
        let mut id_bytes = [0u8; 32];
        id_bytes[..8].copy_from_slice(&(idx as u64).to_be_bytes());
        let id = MessageId(id_bytes);

        let payload = codec::encode_privmsg_payload(&nick, &text, true);
        let val = codec::encode_value(MsgType::PrivMsg, &payload);
        let key = codec::encode_key(timest, &id);
        tree.insert(&key, &val).unwrap();
    }
    // O(n)
    debug!(target: "app::schema", "populated db with {} lines", tree.len().unwrap());
}
