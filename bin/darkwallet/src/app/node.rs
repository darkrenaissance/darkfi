/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    darkirc::{DarkIrcBackendPtr, Privmsg},
    error::Error,
    expr::Op,
    gfx::{GraphicsEventPublisherPtr, RenderApiPtr, Vertex},
    prop::{Property, PropertyBool, PropertyStr, PropertySubType, PropertyType, Role},
    scene::{
        CallArgType, MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId,
        SceneNodeType, Slot,
    },
    text::TextShaperPtr,
    ui::{chatview, Button, ChatView, EditBox, Image, Mesh, RenderLayer, Stoppable, Text, Window},
    ExecutorPtr,
};

pub fn create_layer(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_layer({name})");
    let node = sg.add_node(name, SceneNodeType::RenderLayer);
    let prop = Property::new("is_visible", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_mesh(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_mesh({name})");
    let node = sg.add_node(name, SceneNodeType::RenderMesh);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_button(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_button({name})");
    let node = sg.add_node(name, SceneNodeType::Button);

    let mut prop = Property::new("is_active", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Active", "An active Button can be clicked");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    node.add_signal("click", "Button clicked event", vec![]).unwrap();

    node.id
}

pub fn create_image(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_image({name})");
    let node = sg.add_node(name, SceneNodeType::RenderMesh);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("path", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_text(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_text({name})");
    let node = sg.add_node(name, SceneNodeType::RenderText);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
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

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_editbox(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_editbox({name})");
    let node = sg.add_node(name, SceneNodeType::EditBox);

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

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_pos", PropertyType::Uint32, PropertySubType::Pixel);
    prop.set_range_u32(0, u32::MAX);
    node.add_property(prop).unwrap();

    let prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("hi_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("selected", PropertyType::Uint32, PropertySubType::Color);
    prop.set_array_len(2);
    prop.allow_null_values();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_chatview(
    sg: &mut SceneGraph,
    name: &str,
) -> (SceneNodeId, async_channel::Receiver<Vec<u8>>) {
    debug!(target: "app", "create_chatview({name})");
    let node = sg.add_node(name, SceneNodeType::ChatView);

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

    let prop = Property::new("line_height", PropertyType::Float32, PropertySubType::Pixel);
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

    let prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_start_accel", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Mouse Scroll Start Acceleration", "Initial acceperation when scrolling");
    prop.set_defaults_f32(vec![4.]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_decel", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text(
        "Mouse Scroll Deceleration",
        "Deceleration factor for mouse scroll acceleration",
    );
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0.5]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_resist", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Mouse Scroll Resistance", "How quickly scrolling speed is dampened");
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0.9]).unwrap();
    node.add_property(prop).unwrap();

    let (sender, recvr) = async_channel::unbounded::<Vec<u8>>();
    let method = move |data: Vec<u8>, response_fn: MethodResponseFn| {
        if sender.try_send(data).is_err() {
            response_fn(Err(Error::ChannelClosed));
        } else {
            response_fn(Ok(vec![]));
        }
    };
    node.add_method(
        "insert_line",
        vec![
            ("timestamp", "Timestamp", CallArgType::Uint64),
            ("id", "Message ID", CallArgType::Hash),
            ("nick", "Nickname", CallArgType::Str),
            ("text", "Text", CallArgType::Str),
        ],
        vec![],
        Box::new(method),
    )
    .unwrap();

    (node.id, recvr)
}

