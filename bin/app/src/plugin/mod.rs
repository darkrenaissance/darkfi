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

use sled_overlay::sled;
use std::{array::TryFromSliceError, string::FromUtf8Error, sync::Arc};

pub mod darkirc;
#[cfg(feature = "enable-plugins")]
pub use darkirc::DarkIrc;
pub use darkirc::DarkIrcPtr;

pub mod fud;
pub use fud::{FudPlugin as Fud, FudPluginPtr as FudPtr};

use darkfi::net::Settings as NetSettings;

use crate::{
    prop::{Property, PropertyAtomicGuard, PropertySubType, PropertyType, PropertyValue, Role},
    scene::{SceneNode, SceneNodePtr, SceneNodeType},
};

pub struct PluginSettings {
    pub setting_root: SceneNodePtr,
    pub sled_tree: sled::Tree,
}
impl PluginSettings {
    pub fn add_setting(&self, name: &str, default: PropertyValue) -> Option<SceneNodePtr> {
        let atom = &mut PropertyAtomicGuard::none();
        let node = match default {
            PropertyValue::Bool(b) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Bool, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Bool, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_bool(atom, Role::User, "value", b.clone()).unwrap();
                node.set_property_bool(atom, Role::App, "default", b.clone()).unwrap();
                Some(node)
            }
            PropertyValue::Uint32(u) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Uint32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Uint32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_u32(atom, Role::User, "value", u.clone()).unwrap();
                node.set_property_u32(atom, Role::App, "default", u.clone()).unwrap();
                Some(node)
            }
            PropertyValue::Float32(f) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Float32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Float32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_f32(atom, Role::User, "value", f.clone()).unwrap();
                node.set_property_f32(atom, Role::App, "default", f.clone()).unwrap();
                Some(node)
            }
            PropertyValue::Str(s) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Str, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Str, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_str(atom, Role::User, "value", s.clone()).unwrap();
                node.set_property_str(atom, Role::App, "default", s.clone()).unwrap();
                Some(node)
            }
            _ => None,
        };

        match node {
            Some(n) => {
                let node_ptr = Arc::new(n);
                self.setting_root.link(node_ptr.clone().into());
                Some(node_ptr)
            }
            None => None,
        }
    }

    // For all settings, copy the value from sled into the setting node's value property
    pub fn load_settings(&self) {
        let atom = &mut PropertyAtomicGuard::none();
        for setting_node in self.setting_root.get_children().iter() {
            if setting_node.typ != SceneNodeType::Setting {
                continue
            }

            let value = setting_node.get_property("value").clone().unwrap();
            match value.typ {
                PropertyType::Bool => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        setting_node
                            .set_property_bool(atom, Role::User, "value", sled_value[0] != 0)
                            .unwrap();
                    }
                }
                PropertyType::Uint32 => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        if sled_value.len() == 4 {
                            let bytes: Result<[u8; 4], TryFromSliceError> =
                                sled_value.as_ref().try_into();
                            if let Ok(b) = bytes {
                                setting_node
                                    .set_property_u32(
                                        atom,
                                        Role::User,
                                        "value",
                                        u32::from_le_bytes(b),
                                    )
                                    .unwrap();
                            }
                        }
                    }
                }
                PropertyType::Float32 => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        if sled_value.len() == 4 {
                            let bytes: Result<[u8; 4], TryFromSliceError> =
                                sled_value.as_ref().try_into();
                            if let Ok(b) = bytes {
                                setting_node
                                    .set_property_f32(
                                        atom,
                                        Role::User,
                                        "value",
                                        f32::from_le_bytes(b),
                                    )
                                    .unwrap();
                            }
                        }
                    }
                }
                PropertyType::Str => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        let string: Result<String, FromUtf8Error> =
                            String::from_utf8(sled_value.to_vec());
                        if let Ok(s) = string {
                            setting_node.set_property_str(atom, Role::User, "value", s).unwrap();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Save all settings to sled
    pub fn save_settings(&self) {
        for setting_node in self.setting_root.get_children().iter() {
            if setting_node.typ != SceneNodeType::Setting {
                continue
            }

            let value = setting_node.get_property("value").clone().unwrap();
            match value.typ {
                PropertyType::Bool => {
                    let value_bytes = if value.get_bool(0).unwrap() { 1u8 } else { 0u8 };
                    self.sled_tree
                        .insert(setting_node.name.as_str(), sled::IVec::from(vec![value_bytes]))
                        .unwrap();
                }
                PropertyType::Uint32 => {
                    self.sled_tree
                        .insert(
                            setting_node.name.as_str(),
                            sled::IVec::from(value.get_u32(0).unwrap().to_le_bytes().as_ref()),
                        )
                        .unwrap();
                }
                PropertyType::Float32 => {
                    self.sled_tree
                        .insert(
                            setting_node.name.as_str(),
                            sled::IVec::from(value.get_f32(0).unwrap().to_le_bytes().as_ref()),
                        )
                        .unwrap();
                }
                PropertyType::Str => {
                    self.sled_tree
                        .insert(
                            setting_node.name.as_str(),
                            sled::IVec::from(value.get_str(0).unwrap().as_bytes()),
                        )
                        .unwrap();
                }
                _ => {}
            }
        }
    }

    pub fn get_setting(&self, name: &str) -> Option<SceneNodePtr> {
        self.setting_root
            .clone()
            .get_children()
            .iter()
            .find(|node| node.typ == SceneNodeType::Setting && node.name == name)
            .cloned()
    }

    pub fn add_p2p_settings(&self, p2p_settings: &NetSettings) {
        self.add_setting(
            "net.outbound_connections",
            PropertyValue::Uint32(p2p_settings.outbound_connections as u32),
        );
        self.add_setting(
            "net.inbound_connections",
            PropertyValue::Uint32(p2p_settings.inbound_connections as u32),
        );
        self.add_setting(
            "net.outbound_connect_timeout",
            PropertyValue::Uint32(p2p_settings.outbound_connect_timeout as u32),
        );
        self.add_setting(
            "net.channel_handshake_timeout",
            PropertyValue::Uint32(p2p_settings.channel_handshake_timeout as u32),
        );
        self.add_setting(
            "net.channel_heartbeat_interval",
            PropertyValue::Uint32(p2p_settings.channel_heartbeat_interval as u32),
        );
        self.add_setting(
            "net.outbound_peer_discovery_cooloff_time",
            PropertyValue::Uint32(p2p_settings.outbound_peer_discovery_cooloff_time as u32),
        );
        self.add_setting("net.localnet", PropertyValue::Bool(p2p_settings.localnet));
        self.add_setting(
            "net.greylist_refinery_interval",
            PropertyValue::Uint32(p2p_settings.greylist_refinery_interval as u32),
        );
        self.add_setting(
            "net.white_connect_percent",
            PropertyValue::Uint32(p2p_settings.white_connect_percent as u32),
        );
        self.add_setting(
            "net.gold_connect_count",
            PropertyValue::Uint32(p2p_settings.gold_connect_count as u32),
        );
        self.add_setting(
            "net.slot_preference_strict",
            PropertyValue::Bool(p2p_settings.slot_preference_strict),
        );
        self.add_setting(
            "net.time_with_no_connections",
            PropertyValue::Uint32(p2p_settings.time_with_no_connections as u32),
        );
    }

    // Update a NetSettings from settings in the node tree
    pub fn update_p2p_settings(&self, p2p_settings: &mut NetSettings) {
        p2p_settings.outbound_connections = self
            .get_setting("net.outbound_connections")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as usize;
        p2p_settings.inbound_connections =
            self.get_setting("net.inbound_connections").unwrap().get_property_u32("value").unwrap()
                as usize;
        p2p_settings.outbound_connect_timeout = self
            .get_setting("net.outbound_connect_timeout")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
        p2p_settings.channel_handshake_timeout = self
            .get_setting("net.channel_handshake_timeout")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
        p2p_settings.channel_heartbeat_interval = self
            .get_setting("net.channel_heartbeat_interval")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
        p2p_settings.outbound_peer_discovery_cooloff_time = self
            .get_setting("net.outbound_peer_discovery_cooloff_time")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
        p2p_settings.localnet =
            self.get_setting("net.localnet").unwrap().get_property_bool("value").unwrap();
        p2p_settings.greylist_refinery_interval = self
            .get_setting("net.greylist_refinery_interval")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
        p2p_settings.white_connect_percent = self
            .get_setting("net.white_connect_percent")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as usize;
        p2p_settings.gold_connect_count =
            self.get_setting("net.gold_connect_count").unwrap().get_property_u32("value").unwrap()
                as usize;
        p2p_settings.slot_preference_strict = self
            .get_setting("net.slot_preference_strict")
            .unwrap()
            .get_property_bool("value")
            .unwrap();
        p2p_settings.time_with_no_connections = self
            .get_setting("net.time_with_no_connections")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
    }
}
