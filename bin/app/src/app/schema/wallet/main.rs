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

use darkfi_serial::{Decodable, Encodable};

use crate::{
    app::{
        node::{
            create_button, create_chatview, create_layer, create_text, create_tokentable,
            create_vector_art,
        },
        App,
    },
    expr,
    gfx::gfxtag,
    mesh::{rgba, COLOR_CYAN, COLOR_TEAL},
    prop::{PropertyAtomicGuard, PropertyBool, PropertyFloat32, Role},
    scene::{Pimpl, SceneNodePtr, Slot},
    shape,
    theme::wire_color,
    ui::{
        chatview::{MessageId, FEED_CHANNEL},
        Button, ChatView, Layer, Text, TokenTable, VectorArt, VectorShape,
    },
    util::i18n::I18nBabelFish,
};

use super::{data::*, util::*};

pub async fn make(
    app: &App,
    wallet_layer: SceneNodePtr,
    i18n_fish: &I18nBabelFish,
    window_scale: PropertyFloat32,
) -> SceneNodePtr {
    let atom = &mut PropertyAtomicGuard::none();

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("HEADER_HEIGHT", HEADER_HEIGHT);
    cc.add_const_f32("TOKEN_ROW_HEIGHT", TOKEN_ROW_HEIGHT);
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("PADDING_Y", PADDING_Y);
    cc.add_const_f32("TITLE_FONTSIZE", TITLE_FONTSIZE);
    cc.add_const_f32("BUTTON_HEIGHT", BUTTON_HEIGHT);
    cc.add_const_f32("BUTTON_FONTSIZE", BUTTON_FONTSIZE);
    cc.add_const_f32("ROW_HEIGHT", ROW_HEIGHT);

    // ============================================
    // Main Wallet Layer
    // ============================================
    let main_layer = create_layer("main_layer");
    let prop = main_layer.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();
    main_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    main_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let main_layer = main_layer
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    wallet_layer.link(main_layer.clone());

    let main_is_visible = PropertyBool::wrap(&main_layer, Role::App, "is_visible", 0).unwrap();

    // ============================================
    // Feed Layer (bottom half of the screen)
    // ============================================
    cc.add_const_f32("FEED_LAYER_PADDING", FEED_LAYER_PADDING);
    cc.add_const_f32("FEED_HEADER_HEIGHT", FEED_HEADER_HEIGHT);
    let feed_layer = create_layer("feed_layer");
    let prop = feed_layer.get_property("rect").unwrap();
    prop.set_default_f32(0, FEED_LAYER_PADDING).unwrap();
    let code = cc.compile("h / 2 + FEED_LAYER_PADDING").unwrap();
    prop.set_default_expr(1, code).unwrap();
    let code = cc.compile("w - 2 * FEED_LAYER_PADDING").unwrap();
    prop.set_default_expr(2, code).unwrap();
    let code = cc.compile("h / 2 - 2 * FEED_LAYER_PADDING").unwrap();
    prop.set_default_expr(3, code).unwrap();
    feed_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    feed_layer.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    let feed_layer = feed_layer
        .setup(|me| Layer::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    // Child of the main wallet layer: shows with the main screen and
    // hides with it when navigating to send/receive screens.
    main_layer.link(feed_layer.clone());

    // Feed layer background: covers the entire layer. Transparent grey
    // by default; the active theme may restyle it (scifi recolors it).
    let node = create_vector_art("bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 0).unwrap();
    let mut shape = VectorShape::new();
    shape.add_filled_box(
        expr::const_f32(0.),
        expr::const_f32(0.),
        expr::load_var("w"),
        expr::load_var("h"),
        [0.5, 0.5, 0.5, 0.5],
    );
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    feed_layer.link(node);

    // The global feed chatview, bound to the reserved `*` pseudo-channel.
    // Starts below the feed header band and fills the rest of the layer.
    // Channel labels are forced on (the feed is unread without them).
    let node = create_chatview("feed_chatty");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, FEED_HEADER_HEIGHT).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    let code = cc.compile("h - FEED_HEADER_HEIGHT").unwrap();
    prop.set_default_expr(3, code).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, FEED_FONTSIZE).unwrap();
    node.get_property("timestamp_font_size")
        .unwrap()
        .set_default_f32(0, FEED_TIMESTAMP_FONTSIZE)
        .unwrap();
    node.get_property("timestamp_width").unwrap().set_default_f32(0, FEED_TIMESTAMP_WIDTH).unwrap();
    node.get_property("line_height").unwrap().set_default_f32(0, FEED_LINE_HEIGHT).unwrap();
    node.get_property("message_spacing").unwrap().set_default_f32(0, FEED_MESSAGE_SPACING).unwrap();
    node.get_property("baseline").unwrap().set_default_f32(0, FEED_BASELINE).unwrap();
    node.set_property_f32(atom, Role::App, "wheel_page_frac", 0.2).unwrap();
    node.set_property_bool(atom, Role::App, "show_channel", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 1).unwrap();

    // Class wiring onto the shared /theme tokens.
    let theme = app.sg_root.lookup_node("/theme").unwrap();
    wire_color(&node, "timestamp_color", &theme, "chatview.timestamp_color").unwrap();
    wire_color(&node, "text_color", &theme, "chatview.text_color").unwrap();
    wire_color(&node, "hi_bg_color", &theme, "chatview.hi_bg_color").unwrap();

    let kv_db = app.kv_db.get().expect("app kvdb set in setup()").clone();
    let feed_chatty = node
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
    feed_layer.link(feed_chatty.clone());

    // Type-node styling: privmsg + date separator.
    let privmsg_node = feed_chatty.lookup_node("/privmsg").expect("privmsg type node");
    privmsg_node.set_property_f32(atom, Role::App, "cap_max_height", 1000.).unwrap();
    wire_color(&privmsg_node, "action_text_color", &theme, "chatview.action_text_color").unwrap();
    wire_color(&privmsg_node, "url_text_color", &theme, "chatview.url_text_color").unwrap();
    wire_color(&privmsg_node, "channel_color", &theme, "chatview.channel_color").unwrap();
    let prop = privmsg_node.get_property("url_bg_color").unwrap();
    prop.set_default_f32_multi(&[0., 0.13, 0.08, 1.]).unwrap();
    privmsg_node.get_property("url_bg_border_size").unwrap().set_default_f32(0, 1.).unwrap();
    let prop = privmsg_node.get_property("url_bg_border_color").unwrap();
    prop.set_default_f32_multi(&[0.11, 0.6, 0.63, 1.]).unwrap();
    // Channel label chip: same values as the chat screen.
    let prop = privmsg_node.get_property("channel_bg_color").unwrap();
    prop.set_default_f32_multi(&rgba!(0x263B3Cff)).unwrap();
    let prop = privmsg_node.get_property("channel_border_color").unwrap();
    prop.set_default_f32_multi(&rgba!(0x689AA5ff)).unwrap();
    privmsg_node.get_property("channel_border_size").unwrap().set_default_f32(0, 1.).unwrap();
    privmsg_node
        .get_property("channel_font_size")
        .unwrap()
        .set_default_f32(0, FEED_TIMESTAMP_FONTSIZE)
        .unwrap();
    privmsg_node
        .get_property("channel_spacing")
        .unwrap()
        .set_default_f32(0, FEED_CHANNEL_SPACING)
        .unwrap();
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
        1.00, 0.30, 0.00, 1.,
    ];
    prop.set_default_f32_multi(&nick_colors).unwrap();

    let datemsg_node = feed_chatty.lookup_node("/datemsg").expect("datemsg type node");
    datemsg_node.get_property("font_size").unwrap().set_default_f32(0, FEED_DATE_FONTSIZE).unwrap();
    let prop = datemsg_node.get_property("color").unwrap();
    prop.set_default_f32_multi(&[0.5, 0.5, 0.5, 1.]).unwrap();

    // Clicking a channel label navigates to the chat screen and opens
    // that channel (mirrors the channels-menu click path).
    {
        let (slot, recvr) = Slot::new("feed_channel_nav");
        privmsg_node.register("channel_clicked", slot).unwrap();
        let sg_root = app.sg_root.clone();
        let redraw = app.redraw_trigger.clone();
        let listen_channel = app.ex.spawn(async move {
            while let Ok(data) = recvr.recv().await {
                let mut cur = std::io::Cursor::new(&data);
                let Ok(_id) = MessageId::decode(&mut cur) else { continue };
                let Ok(channel) = String::decode(&mut cur) else { continue };
                if channel == FEED_CHANNEL {
                    continue
                }
                let atom = &mut redraw.make_guard(gfxtag!("feed channel nav"));
                let Some(chat_screen) = sg_root.lookup_node("/window/content/chat") else {
                    continue
                };
                chat_screen.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                let wallet_screen = sg_root.lookup_node("/window/content/wallet").unwrap();
                wallet_screen.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                let Some(node) = sg_root.lookup_node("/window/content/chat/main_chat_layer") else {
                    continue
                };
                node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                let menu_node = sg_root.lookup_node("/window/content/chat/menu_layer").unwrap();
                menu_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                let chatty = node.lookup_node("/content/chatty").unwrap();
                let mut chan_data = vec![];
                channel.encode(&mut chan_data).unwrap();
                let _ = chatty.call_method("set_channel", chan_data).await;
            }
        });
        app.tasks.lock().push(listen_channel);
    }

    // Bind to the feed. Direct pimpl call (not the method bus): the
    // bus has no buffering, so a call before the node's start()
    // subscribes would be silently dropped. The loader's wake flag
    // persists, so binding before start() spawns the pump is safe.
    let Pimpl::ChatView(feed_view) = feed_chatty.pimpl() else { unreachable!() };
    feed_view.handle_set_channel(FEED_CHANNEL.to_string()).await;

    // Back arrow
    let node = create_vector_art("wallet_back_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, BACKARROW_X).unwrap();
    prop.set_default_f32(1, BACKARROW_Y).unwrap();
    prop.set_default_f32(2, BACKARROW_SCALE).unwrap();
    prop.set_default_f32(3, BACKARROW_SCALE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
    let shape = shape::create_back_arrow([0., 1., 1., 1.]).scaled(BACKARROW_SCALE);
    let prop = node.get_property("shape").unwrap();
    prop.set_default_shape(0, shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    wallet_layer.link(node.clone());
    let back_bg_is_visible = PropertyBool::wrap(&node, Role::App, "is_visible", 0).unwrap();

    // Back button
    let node = create_button("wallet_back_btn");
    node.set_property_bool(atom, Role::App, "is_active", false).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 10).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 10).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_f32(2, BACKARROW_BG_W).unwrap();
    prop.set_default_f32(3, HEADER_HEIGHT).unwrap();

    let redraw = app.redraw_trigger.clone();
    let sg_root = app.sg_root.clone();
    let main_is_visible1 = main_is_visible.clone();
    let (slot, recvr) = Slot::new("wallet_back_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while recvr.recv().await.is_ok() {
            let atom = &mut redraw.make_guard(gfxtag!("wallet back button"));
            let tx_status_is_visible = PropertyBool::wrap(
                &sg_root.lookup_node("/window/content/wallet/tx_status_layer").unwrap(),
                Role::App,
                "is_visible",
                0,
            )
            .unwrap();
            let step4_is_visible = PropertyBool::wrap(
                &sg_root.lookup_node("/window/content/wallet/send_step4_layer").unwrap(),
                Role::App,
                "is_visible",
                0,
            )
            .unwrap();
            let step3_is_visible = PropertyBool::wrap(
                &sg_root.lookup_node("/window/content/wallet/send_step3_layer").unwrap(),
                Role::App,
                "is_visible",
                0,
            )
            .unwrap();
            let step2_is_visible = PropertyBool::wrap(
                &sg_root.lookup_node("/window/content/wallet/send_step2_layer").unwrap(),
                Role::App,
                "is_visible",
                0,
            )
            .unwrap();
            let step1_is_visible = PropertyBool::wrap(
                &sg_root.lookup_node("/window/content/wallet/send_step1_layer").unwrap(),
                Role::App,
                "is_visible",
                0,
            )
            .unwrap();
            let receive_is_visible = PropertyBool::wrap(
                &sg_root.lookup_node("/window/content/wallet/receive_layer").unwrap(),
                Role::App,
                "is_visible",
                0,
            )
            .unwrap();

            if tx_status_is_visible.get() {
                tx_status_is_visible.set(atom, false);
                main_is_visible1.set(atom, true);
                continue
            }

            if step4_is_visible.get() {
                step4_is_visible.set(atom, false);
                step3_is_visible.set(atom, true);
                continue
            }

            if step3_is_visible.get() {
                if let Some(error_node) =
                    sg_root.lookup_node("/window/content/wallet/send_step3_layer/error")
                {
                    error_node.set_property_str(atom, Role::App, "text", "").unwrap();
                }
                step3_is_visible.set(atom, false);
                step2_is_visible.set(atom, true);
                continue
            }

            if step2_is_visible.get() {
                step2_is_visible.set(atom, false);
                step1_is_visible.set(atom, true);
                continue
            }

            if step1_is_visible.get() {
                step1_is_visible.set(atom, false);
                main_is_visible1.set(atom, true);
                continue
            }

            if receive_is_visible.get() {
                receive_is_visible.set(atom, false);
                main_is_visible1.set(atom, true);
            }
        }
    });
    app.tasks.lock().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    wallet_layer.link(node.clone());
    let back_btn_is_active = PropertyBool::wrap(&node, Role::App, "is_active", 0).unwrap();

    // Main menu button bg (blue box placeholder). Same spot the back
    // button takes on the wallet subscreens.
    let node = create_vector_art("main_menu_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, BACKARROW_BG_W / 2.).unwrap();
    prop.set_default_f32(1, HEADER_HEIGHT / 2.).unwrap();
    prop.set_default_f32(2, BACKARROW_BG_W).unwrap();
    prop.set_default_f32(3, HEADER_HEIGHT).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
    let shape = shape::create_darkfi_logo([1., 1., 1., 1.]).scaled(DARKFI_LOGO_SCALE);
    node.set_property_shape(atom, Role::App, "shape", shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    wallet_layer.link(node.clone());
    let main_menu_bg_is_visible = PropertyBool::wrap(&node, Role::App, "is_visible", 0).unwrap();

    // Main menu button, only active on the main wallet screen
    let node = create_button("main_menu_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 10).unwrap();
    node.set_property_u32(atom, Role::App, "priority", 10).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    prop.set_default_f32(1, 0.).unwrap();
    prop.set_default_f32(2, BACKARROW_BG_W).unwrap();
    prop.set_default_f32(3, HEADER_HEIGHT).unwrap();

    let sg_root = app.sg_root.clone();
    let redraw = app.redraw_trigger.clone();
    let (slot, recvr) = Slot::new("main_menu_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while recvr.recv().await.is_ok() {
            info!(target: "app::wallet", "clicked main menu");
            let main_menu_layer = sg_root.lookup_node("/window/content/main_menu_layer").unwrap();
            let main_menu_is_visible =
                PropertyBool::wrap(&main_menu_layer, Role::App, "is_visible", 0).unwrap();
            let atom = &mut redraw.make_guard(gfxtag!("main menu toggle"));
            main_menu_is_visible.set(atom, !main_menu_is_visible.get());
        }
    });
    app.tasks.lock().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    wallet_layer.link(node.clone());
    let main_menu_btn_is_active = PropertyBool::wrap(&node, Role::App, "is_active", 0).unwrap();

    // Show the back button whenever the main wallet screen is hidden,
    // and the main menu button whenever it is shown
    let redraw = app.redraw_trigger.clone();
    let main_is_visible2 = main_is_visible.clone();
    let main_is_visible_sub = main_is_visible.prop().subscribe_modify();
    let listen_main_visible = app.ex.spawn(async move {
        while let Ok(_) = main_is_visible_sub.receive().await {
            let atom = &mut redraw.make_guard(gfxtag!("wallet back button visibility"));
            let visible = !main_is_visible2.get();
            back_bg_is_visible.set(atom, visible);
            back_btn_is_active.set(atom, visible);
            main_menu_bg_is_visible.set(atom, !visible);
            main_menu_btn_is_active.set(atom, !visible);
        }
    });
    app.tasks.lock().push(listen_main_visible);

    create_chat_btn(app, atom, &cc, &main_layer).await;

    let mut y = HEADER_HEIGHT;

    // Balance display
    let node = create_text("wallet_balance");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, PADDING_X).unwrap();
    prop.set_default_f32(1, y + TITLE_PADDING).unwrap();
    prop.set_default_f32(2, 1000.).unwrap();
    prop.set_default_f32(3, TITLE_FONTSIZE).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, TITLE_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "DRK 0").unwrap();
    // Class wiring: the wallet title follows the shared text token.
    let theme = app.sg_root.lookup_node("/theme").unwrap();
    wire_color(&node, "text_color", &theme, "text_color").unwrap();
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
    main_layer.link(node);

    y += TITLE_PADDING * 2. + TITLE_FONTSIZE + 1.;

    create_separator(
        &app.renderer,
        &app.redraw_trigger,
        atom,
        &main_layer,
        "wallet_balance_separator",
        &mut y,
    )
    .await;

    // Receive button bg
    let node = create_vector_art("receive_btn_bg");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, PADDING_X).unwrap();
    prop.set_default_f32(1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_default_expr(2, code).unwrap();
    prop.set_default_f32(3, BUTTON_HEIGHT).unwrap();
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
    node.get_property("shape").unwrap().set_default_shape(0, shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    main_layer.link(node);

    // Receive button click handler
    let node = create_button("receive_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, PADDING_X).unwrap();
    prop.set_default_f32(1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_default_expr(2, code).unwrap();
    prop.set_default_f32(3, BUTTON_HEIGHT).unwrap();

    let redraw = app.redraw_trigger.clone();
    let main_is_visible2 = main_is_visible.clone();
    let sg_root = app.sg_root.clone();
    let (slot, recvr) = Slot::new("receive_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while recvr.recv().await.is_ok() {
            let atom = &mut redraw.make_guard(gfxtag!("receive button click"));
            main_is_visible2.set(atom, false);

            let receive_layer =
                sg_root.lookup_node("/window/content/wallet/receive_layer").unwrap();
            receive_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();

            // Get the default address from drk plugin and update the UI
            if let Some(drk_node) = sg_root.lookup_node("/plugin/drk") {
                if let Ok(Some(response_data)) =
                    drk_node.call_method("get_default_address", vec![]).await
                {
                    let mut cur = std::io::Cursor::new(response_data);
                    if let Ok(address) = String::decode(&mut cur) {
                        d!("Got default address from drk: {address}");
                        if let Some(receive_address_node) =
                            receive_layer.lookup_node("/receive_address")
                        {
                            receive_address_node
                                .set_property_str(atom, Role::App, "text", address)
                                .unwrap();
                        }
                    } else {
                        e!("Failed to decode default address response");
                    }
                } else {
                    e!("Failed to call get_default_address method");
                }
            } else {
                e!("Failed to lookup drk plugin node");
            }
        }
    });
    app.tasks.lock().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    main_layer.link(node);

    // Receive label
    let node = create_text("receive_label");
    let prop = node.get_property("rect").unwrap();
    prop.set_default_f32(0, PADDING_X).unwrap();
    prop.set_default_f32(1, y + PADDING_X + BUTTON_HEIGHT / 2. - BUTTON_FONTSIZE / 1.8).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_default_expr(2, code).unwrap();
    prop.set_default_f32(3, BUTTON_HEIGHT).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, BUTTON_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "receive").unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_default_f32_multi(&[COLOR_CYAN[0], COLOR_CYAN[1], COLOR_CYAN[2], COLOR_CYAN[3]])
        .unwrap();
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
    main_layer.link(node);

    // Send button bg
    let node = create_vector_art("wallet_send_btn_bg");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X / 2 + w / 2").unwrap();
    prop.set_default_expr(0, code).unwrap();
    prop.set_default_f32(1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_default_expr(2, code).unwrap();
    prop.set_default_f32(3, BUTTON_HEIGHT).unwrap();
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
    node.get_property("shape").unwrap().set_default_shape(0, shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    main_layer.link(node);

    // Send button click handler
    let node = create_button("wallet_send_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X / 2 + w / 2").unwrap();
    prop.set_default_expr(0, code).unwrap();
    prop.set_default_f32(1, y + PADDING_X).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_default_expr(2, code).unwrap();
    prop.set_default_f32(3, BUTTON_HEIGHT).unwrap();

    let redraw = app.redraw_trigger.clone();
    let sg_root = app.sg_root.clone();
    let main_is_visible3 = main_is_visible.clone();
    let (slot, recvr) = Slot::new("send_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            let atom = &mut redraw.make_guard(gfxtag!("send button click"));
            main_is_visible3.set(atom, false);
            let send_layer =
                sg_root.lookup_node("/window/content/wallet/send_step1_layer").unwrap();
            send_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
        }
    });
    app.tasks.lock().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    main_layer.link(node);

    // Send label
    let node = create_text("wallet_send_label");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile("PADDING_X / 2 + w / 2").unwrap();
    prop.set_default_expr(0, code).unwrap();
    prop.set_default_f32(1, y + PADDING_X + BUTTON_HEIGHT / 2. - BUTTON_FONTSIZE / 1.8).unwrap();
    let code = cc.compile("w / 2 - PADDING_X * 1.5").unwrap();
    prop.set_default_expr(2, code).unwrap();
    prop.set_default_f32(3, BUTTON_HEIGHT).unwrap();
    node.get_property("font_size").unwrap().set_default_f32(0, BUTTON_FONTSIZE).unwrap();
    node.set_property_str(atom, Role::App, "text", "send").unwrap();
    node.set_property_enum(atom, Role::App, "text_align", "center").unwrap();
    let prop = node.get_property("text_color").unwrap();
    prop.set_default_f32_multi(&[COLOR_CYAN[0], COLOR_CYAN[1], COLOR_CYAN[2], COLOR_CYAN[3]])
        .unwrap();
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
    main_layer.link(node);

    y += PADDING_X * 2. + BUTTON_HEIGHT + 1.;

    create_separator(
        &app.renderer,
        &app.redraw_trigger,
        atom,
        &main_layer,
        "wallet_buttons_separator",
        &mut y,
    )
    .await;

    create_title(
        app,
        atom,
        &main_layer,
        &window_scale,
        i18n_fish,
        "tokens_label",
        "TOKENS",
        &mut y,
    )
    .await;

    let mut cc = expr::Compiler::new();
    cc.add_const_f32("PADDING_X", PADDING_X);
    cc.add_const_f32("TOKENS_Y", y);

    let tokens_table = create_tokentable("tokens_table");
    let prop = tokens_table.get_property("rect").unwrap();
    prop.set_default_f32(0, 0.).unwrap();
    let code = cc.compile("TOKENS_Y").unwrap();
    prop.set_default_expr(1, code).unwrap();
    prop.set_default_expr(2, expr::load_var("w")).unwrap();
    prop.set_default_expr(3, expr::load_var("h")).unwrap();
    tokens_table.get_property("font_size").unwrap().set_default_f32(0, BASE_FONTSIZE).unwrap();
    tokens_table.get_property("padding_x").unwrap().set_default_f32(0, PADDING_X).unwrap();
    tokens_table.get_property("padding_y").unwrap().set_default_f32(0, PADDING_Y).unwrap();
    tokens_table.set_property_u32(atom, Role::App, "z_index", 2).unwrap();
    tokens_table.set_property_u32(atom, Role::App, "priority", 0).unwrap();

    let prop = tokens_table.get_property("text_color").unwrap();
    prop.set_default_f32_multi(&[1., 1., 1., 1.]).unwrap();

    let prop = tokens_table.get_property("separator_color").unwrap();
    prop.set_default_f32_multi(&[0.2, 0.2745, 0.2784, 1.]).unwrap();

    let tokens_table = tokens_table
        .setup(|me| TokenTable::new(me, app.renderer.clone(), app.redraw_trigger.clone()))
        .await;
    main_layer.link(tokens_table.clone());

    main_layer
}

async fn create_chat_btn(
    app: &App,
    atom: &mut PropertyAtomicGuard,
    cc: &expr::Compiler,
    parent: &SceneNodePtr,
) {
    let node = create_vector_art("chat_btn_shape");
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile(format!("w - {CHAT_BTN_SIZE}/2 - {CHAT_BTN_MARGIN}")).unwrap();
    prop.set_default_expr(0, code).unwrap();
    let code = cc.compile(format!("h - {CHAT_BTN_SIZE}/2 - {CHAT_BTN_MARGIN}")).unwrap();
    prop.set_default_expr(1, code).unwrap();
    prop.set_default_f32(2, CHAT_BTN_SIZE).unwrap();
    prop.set_default_f32(3, CHAT_BTN_SIZE).unwrap();
    node.set_property_u32(atom, Role::App, "z_index", 3).unwrap();
    node.get_property("scale").unwrap().set_default_f32(0, CHAT_BTN_LOGO_SCALE).unwrap();
    let mut shape = shape::create_netlogo1([0., 0.94, 1., 1.]);
    shape.join(shape::create_netlogo2([0., 0.94, 1., 1.]));
    shape.join(shape::create_netlogo3([0., 0.94, 1., 1.]));
    let prop = node.get_property("shape").unwrap();
    prop.set_default_shape(0, shape).unwrap();
    let node =
        node.setup(|me| VectorArt::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    parent.link(node);

    let node = create_button("chat_btn");
    node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
    // The button floats above the tokens table (priority 0), whose
    // rect spans the rest of the layer — it must be hit-tested first.
    node.set_property_u32(atom, Role::App, "priority", 1).unwrap();
    let prop = node.get_property("rect").unwrap();
    let code = cc.compile(format!("w - {CHAT_BTN_SIZE} - {CHAT_BTN_MARGIN}")).unwrap();
    prop.set_default_expr(0, code).unwrap();
    let code = cc.compile(format!("h - {CHAT_BTN_SIZE} - {CHAT_BTN_MARGIN}")).unwrap();
    prop.set_default_expr(1, code).unwrap();
    prop.set_default_f32(2, CHAT_BTN_SIZE).unwrap();
    prop.set_default_f32(3, CHAT_BTN_SIZE).unwrap();

    let sg_root = app.sg_root.clone();
    let redraw = app.redraw_trigger.clone();
    let menu_is_visible = PropertyBool::wrap(
        &sg_root.lookup_node("/window/content/chat").unwrap(),
        Role::App,
        "is_visible",
        0,
    )
    .unwrap();
    let wallet_is_visible = PropertyBool::wrap(
        &sg_root.lookup_node("/window/content/wallet").unwrap(),
        Role::App,
        "is_visible",
        0,
    )
    .unwrap();
    let (slot, recvr) = Slot::new("chat_btn_clicked");
    node.register("click", slot).unwrap();
    let listen_click = app.ex.spawn(async move {
        while let Ok(_) = recvr.recv().await {
            info!(target: "app::wallet", "clicked back from wallet");
            let atom = &mut redraw.make_guard(gfxtag!("wallet goback action"));
            wallet_is_visible.set(atom, false);
            menu_is_visible.set(atom, true);
        }
    });
    app.tasks.lock().push(listen_click);

    let node =
        node.setup(|me| Button::new(me, app.renderer.clone(), app.redraw_trigger.clone())).await;
    parent.link(node);
}
