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
    prop::{Property, PropertySubType, PropertyType},
    scene::{CallArgType, SceneNode, SceneNodeType},
};

pub fn create_window(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Window);

    let mut prop = Property::new("locale", PropertyType::Str, PropertySubType::Locale);
    prop.set_defaults_str(vec!["en-US".to_string()]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(2);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("insets", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text(
        "Window Insets",
        "Window insets applied by the system (left, top, right, bottom)",
    );
    prop.set_array_len(4);
    prop.set_defaults_f32(vec![0., 0., 0., 0.]).unwrap();
    node.add_property(prop).unwrap();

    node.add_signal("start", "App UI started", vec![]).unwrap();
    node.add_signal("stop", "App UI stopped", vec![]).unwrap();

    node
}

pub fn create_layer(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Layer);
    let prop = Property::new("is_visible", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node
}

pub fn create_vector_art(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::VectorArt);

    let mut prop = Property::new("is_visible", PropertyType::Bool, PropertySubType::Null);
    prop.set_defaults_bool(vec![true]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scale", PropertyType::Float32, PropertySubType::Null);
    prop.set_ui_text("Scale", "Scale factor for the vector art");
    prop.set_defaults_f32(vec![1.0]).unwrap();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node
}

pub fn create_button(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Button);

    let mut prop = Property::new("is_active", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Active", "An active Button can be clicked");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.add_signal("click", "Button clicked event", vec![]).unwrap();

    node
}

pub fn create_shortcut(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Shortcut);

    let mut prop = Property::new("key", PropertyType::Str, PropertySubType::Null);
    prop.allow_null_values();
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.add_signal("shortcut", "Shortcut triggered", vec![]).unwrap();

    node
}

#[allow(dead_code)]
pub fn create_gesture(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Gesture);

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.add_signal(
        "gesture",
        "Gesture triggered",
        vec![("distance", "Distance", CallArgType::Float32)],
    )
    .unwrap();

    node
}

#[allow(dead_code)]
pub fn create_image(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Image);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("uv", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0., 0., 1., 1.]).unwrap();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("path", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node
}

pub fn create_video(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Image);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("uv", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0., 0., 1., 1.]).unwrap();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("path", PropertyType::Str, PropertySubType::Null);
    #[cfg(target_os = "android")]
    prop.set_ui_text("Path", "Path to .mp4 video file (H.264 format)");
    #[cfg(not(target_os = "android"))]
    prop.set_ui_text("Path", "Path to .ivf video file (AV1 format)");
    node.add_property(prop).unwrap();

    node
}

pub fn create_text(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Text);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("lineheight", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Line Height", "Line height/lead (em)");
    prop.set_defaults_f32(vec![1.2]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("use_i18n", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node
}

pub fn create_baseedit(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Edit);

    let mut prop = Property::new("is_active", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Active", "An active EditBox can be focused");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("is_focused", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Focused", "A focused EditBox receives input");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("lineheight", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Line Height", "Line height/lead (em)");
    prop.set_defaults_f32(vec![1.2]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("padding", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Inner Padding", "Padding inside - top, right, bottom, left");
    prop.set_range_f32(0., f32::MAX);
    prop.set_array_len(4);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll_speed", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Scroll Speed", "Scrolling speed");
    prop.set_defaults_f32(vec![4.]).unwrap();
    node.add_property(prop).unwrap();

    let prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_hi_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_cmd_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_width", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_defaults_f32(vec![2.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_ascent", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_defaults_f32(vec![10.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_descent", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("select_ascent", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_defaults_f32(vec![10.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("select_descent", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("handle_descent", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("hi_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cmd_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("select_text", PropertyType::Str, PropertySubType::Null);
    prop.allow_null_values();
    prop.set_defaults_null().unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_blink_time", PropertyType::Uint32, PropertySubType::Null);
    prop.set_defaults_u32(vec![500]).unwrap();
    prop.set_range_u32(0, u32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_idle_time", PropertyType::Uint32, PropertySubType::Null);
    prop.set_defaults_u32(vec![150]).unwrap();
    prop.set_range_u32(0, u32::MAX);
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("action_fg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Action Menu FG Color", "Foreground color of action menu items");
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0., 0.94, 1., 1.]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("action_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Action Menu BG Color", "Background color of action menu items");
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0.1, 0.1, 0.1, 0.9]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("action_padding", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Action Menu Padding", "Padding inside action menu items");
    prop.set_defaults_f32(vec![8.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("action_spacing", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Action Menu Spacing", "Spacing between action menu items");
    prop.set_defaults_f32(vec![4.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    node.add_signal("enter_pressed", "Enter key pressed", vec![]).unwrap();
    node.add_signal("focus_request", "Request to gain focus", vec![]).unwrap();

    // Used by emoji_picker
    node.add_method("insert_text", vec![("text", "Text", CallArgType::Str)], None).unwrap();
    node.add_method("focus", vec![], None).unwrap();
    node.add_method("unfocus", vec![], None).unwrap();

    node
}

#[allow(dead_code)]
pub fn create_singleline_edit(name: &str) -> SceneNode {
    // No additional properties to add
    create_baseedit(name)
}

pub fn create_multiline_edit(name: &str) -> SceneNode {
    let mut node = create_baseedit(name);

    let mut prop = Property::new("height_range", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Min/Max Height", "Minimum and Maximum height");
    prop.set_range_f32(0., f32::MAX);
    prop.set_array_len(2);
    node.add_property(prop).unwrap();

    node
}

pub fn create_chatview(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::ChatView);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Null);
    prop.set_ui_text("Scroll", "Scroll up from the bottom");
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("timestamp_font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("timestamp_width", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("line_height", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("message_spacing", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("timestamp_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("nick_colors", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_unbounded();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("hi_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("scroll_start_accel", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Scroll Start Acceleration", "Initial acceperation when scrolling");
    prop.set_defaults_f32(vec![4.]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll_resist", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Scroll Resistance", "How quickly scrolling speed is dampened");
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0.9]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("select_hold_time", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Select Holding Time", "How long to hard press for selecting lines (ms)");
    prop.set_defaults_f32(vec![1000.]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("key_scroll_speed", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Page Up/Down Scroll Speed", "Scroll speed when pressing page up/down");
    prop.set_defaults_f32(vec![6.]).unwrap();
    node.add_property(prop).unwrap();

    node.add_signal(
        "fileurl_detected",
        "File URL detected in message",
        vec![("url", "File URL", CallArgType::Str)],
    )
    .unwrap();

    node.add_signal(
        "file_download_request",
        "User requested file download",
        vec![("url", "File URL", CallArgType::Str)],
    )
    .unwrap();

    node.add_method(
        "insert_line",
        vec![
            ("timestamp", "Timestamp", CallArgType::Uint64),
            ("id", "Message ID", CallArgType::Hash),
            ("nick", "Nickname", CallArgType::Str),
            ("text", "Text", CallArgType::Str),
        ],
        None,
    )
    .unwrap();

    node.add_method(
        "insert_unconf_line",
        vec![
            ("timestamp", "Timestamp", CallArgType::Uint64),
            ("id", "Message ID", CallArgType::Hash),
            ("nick", "Nickname", CallArgType::Str),
            ("text", "Text", CallArgType::Str),
        ],
        None,
    )
    .unwrap();

    node.add_method(
        "set_file_status",
        vec![("url", "File URL", CallArgType::Str), ("status", "File status", CallArgType::Str)],
        None,
    )
    .unwrap();

    node
}

pub fn create_emoji_picker(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::EmojiPicker);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Scroll", "Scroll down from the top");
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_speed", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Mouse Scroll Speed", "Mouse Scrolling speed");
    prop.set_defaults_f32(vec![4.]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("emoji_size", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Emoji Size", "The emoji's size");
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    node.add_signal("emoji_select", "Emoji selected", vec![("text", "Text", CallArgType::Str)])
        .unwrap();

    node
}

pub fn create_menu(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Menu);

    let mut prop = Property::new("is_visible", PropertyType::Bool, PropertySubType::Null);
    prop.set_defaults_bool(vec![true]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("priority", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Scroll", "Scroll position from the top");
    prop.set_defaults_f32(vec![0.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Font Size", "Text font size in pixels");
    prop.set_defaults_f32(vec![22.0]).unwrap();
    prop.set_range_f32(1., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("padding", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Padding", "Padding [left, top/bottom]");
    prop.set_array_len(2);
    prop.set_defaults_f32(vec![14.0, 14.0]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("handle_padding", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Handle Padding", "X handle padding from left edge");
    prop.set_defaults_f32(vec![14.0]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Text Color", "Text color (RGBA)");
    prop.set_array_len(4);
    prop.set_defaults_f32(vec![1., 1., 1., 1.]).unwrap();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Background Color", "Item background color");
    prop.set_array_len(4);
    prop.set_defaults_f32(vec![0.1, 0.1, 0.1, 1.]).unwrap();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("sep_size", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Separator Size", "Separator line thickness");
    prop.set_defaults_f32(vec![1.0]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("sep_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Separator Color", "Separator line color");
    prop.set_array_len(4);
    prop.set_defaults_f32(vec![0.5, 0.5, 0.5, 0.3]).unwrap();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("active_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Active Color", "Active item text color");
    prop.set_array_len(4);
    prop.set_defaults_f32(vec![1., 1., 1., 1.]).unwrap();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("alert_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_ui_text("Alert Color", "Alert item text color");
    prop.set_array_len(4);
    prop.set_defaults_f32(vec![1., 0.3, 0.3, 1.]).unwrap();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("scroll_start_accel", PropertyType::Float32, PropertySubType::Null);
    prop.set_ui_text("Scroll Start Acceleration", "Multiplier for initial scroll velocity");
    prop.set_defaults_f32(vec![1.0]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll_resist", PropertyType::Float32, PropertySubType::Null);
    prop.set_ui_text("Scroll Resistance", "Momentum decay factor (0-1, lower = faster stop)");
    prop.set_defaults_f32(vec![0.95]).unwrap();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("fade_zone", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Fade Zone", "Fade out items in the last X pixels");
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("items", PropertyType::Str, PropertySubType::Null);
    prop.set_ui_text("Items", "Menu items");
    prop.set_unbounded();
    node.add_property(prop).unwrap();

    node.add_signal(
        "select",
        "Item selected",
        vec![("item", "Selected item name", CallArgType::Str)],
    )
    .unwrap();

    node.add_signal("edit_active", "Edit mode activated", vec![]).unwrap();

    node.add_method("mark_active", vec![("item_name", "Item name", CallArgType::Str)], None)
        .unwrap();

    node.add_method("mark_alert", vec![("item_name", "Item name", CallArgType::Str)], None)
        .unwrap();

    node.add_method("cancel_edit", vec![], None).unwrap();

    node.add_method("done_edit", vec![], None).unwrap();

    node
}
