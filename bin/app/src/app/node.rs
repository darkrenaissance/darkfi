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
    prop::{Property, PropertySubType, PropertyType},
    scene::{CallArgType, SceneNode, SceneNodeType},
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "app::node", $($arg)*); } }

pub fn create_layer(name: &str) -> SceneNode {
    t!("create_layer({name})");
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
    t!("create_vector_art({name})");
    let mut node = SceneNode::new(name, SceneNodeType::VectorArt);

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

    node
}

pub fn create_button(name: &str) -> SceneNode {
    t!("create_button({name})");
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
    t!("create_shortcut({name})");
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
    t!("create_gesture({name})");
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

pub fn create_image(name: &str) -> SceneNode {
    t!("create_image({name})");
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
    t!("create_video({name})");
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
    prop.set_ui_text("Path", "Path format string using {frame} in the name");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("length", PropertyType::Uint32, PropertySubType::Null);
    prop.set_ui_text("Frame Length", "Total frames to load (last frame + 1)");
    node.add_property(prop).unwrap();

    node
}

pub fn create_text(name: &str) -> SceneNode {
    t!("create_text({name})");
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

    node.add_signal("enter_pressed", "Enter key pressed", vec![]).unwrap();
    node.add_signal("focus_request", "Request to gain focus", vec![]).unwrap();
    node.add_signal("paste_request", "Request to show paste dialog", vec![]).unwrap();

    // Used by emoji_picker
    node.add_method("insert_text", vec![("text", "Text", CallArgType::Str)], None).unwrap();
    node.add_method("focus", vec![], None).unwrap();
    node.add_method("unfocus", vec![], None).unwrap();

    node
}

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
    t!("create_chatview({name})");
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

    node
}

pub fn create_emoji_picker(name: &str) -> SceneNode {
    t!("create_emoji_picker({name})");
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
