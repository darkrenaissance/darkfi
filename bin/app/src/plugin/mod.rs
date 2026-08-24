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

use kvdb_overlay::Tree;
use std::{array::TryFromSliceError, string::FromUtf8Error, sync::Arc};

#[cfg(feature = "enable-plugin-darkirc")]
pub mod darkirc;
#[cfg(feature = "enable-plugin-darkirc")]
pub use darkirc::DarkIrcPtr;

#[cfg(feature = "enable-plugin-fud")]
pub mod fud;
#[cfg(feature = "enable-plugin-fud")]
pub use fud::FudPluginPtr as FudPtr;

#[cfg(feature = "enable-plugin-drk")]
pub mod drk;
#[cfg(feature = "enable-plugin-drk")]
pub use drk::DrkPluginPtr as DrkPtr;

#[cfg(feature = "enable-plugin-darkirc")]
pub use darkirc::DarkIrc;
#[cfg(feature = "enable-plugin-drk")]
pub use drk::DrkPlugin;
#[cfg(feature = "enable-plugin-fud")]
pub use fud::FudPlugin;

#[cfg(any(feature = "enable-plugin-darkirc", feature = "enable-plugin-fud"))]
use darkfi::net::Settings as NetSettings;

#[cfg(any(feature = "enable-plugin-darkirc", feature = "enable-plugin-fud"))]
use crate::{
    prop::{Property, PropertyAtomicGuard, PropertySubType, PropertyType, PropertyValue, Role},
    scene::{SceneNode, SceneNodePtr, SceneNodeType},
};

#[cfg(any(feature = "enable-plugin-darkirc", feature = "enable-plugin-fud"))]
pub struct PluginSettings {
    pub setting_root: SceneNodePtr,
    pub kvdb_tree: Tree,
}

#[cfg(any(feature = "enable-plugin-darkirc", feature = "enable-plugin-fud"))]
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

    // For all settings, copy the value from kvdb into the setting node's value property
    pub fn load_settings(&self) {
        let atom = &mut PropertyAtomicGuard::none();
        for setting_node in self.setting_root.get_children().iter() {
            if setting_node.typ != SceneNodeType::Setting {
                continue
            }

            let value = setting_node.get_property("value").clone().unwrap();
            match value.typ {
                PropertyType::Bool => {
                    let kvdb_result = self.kvdb_tree.get(setting_node.name.as_bytes());
                    if let Ok(Some(kvdb_value)) = kvdb_result {
                        setting_node
                            .set_property_bool(atom, Role::User, "value", kvdb_value[0] != 0)
                            .unwrap();
                    }
                }
                PropertyType::Uint32 => {
                    let kvdb_result = self.kvdb_tree.get(setting_node.name.as_bytes());
                    if let Ok(Some(kvdb_value)) = kvdb_result {
                        if kvdb_value.len() == 4 {
                            if let Ok(b) = kvdb_value.try_into() {
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
                    let kvdb_result = self.kvdb_tree.get(setting_node.name.as_bytes());
                    if let Ok(Some(kvdb_value)) = kvdb_result {
                        if kvdb_value.len() == 4 {
                            if let Ok(b) = kvdb_value.try_into() {
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
                    let kvdb_result = self.kvdb_tree.get(setting_node.name.as_bytes());
                    if let Ok(Some(kvdb_value)) = kvdb_result {
                        let string: Result<String, FromUtf8Error> =
                            String::from_utf8(kvdb_value.to_vec());
                        if let Ok(s) = string {
                            setting_node.set_property_str(atom, Role::User, "value", s).unwrap();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Save all settings to kvdb
    pub fn save_settings(&self) {
        for setting_node in self.setting_root.get_children().iter() {
            if setting_node.typ != SceneNodeType::Setting {
                continue
            }

            let value = setting_node.get_property("value").clone().unwrap();
            match value.typ {
                PropertyType::Bool => {
                    let value_bytes = if value.get_bool(0).unwrap() { 1u8 } else { 0u8 };
                    self.kvdb_tree
                        .insert(setting_node.name.as_bytes(), &vec![value_bytes])
                        .unwrap();
                }
                PropertyType::Uint32 => {
                    self.kvdb_tree
                        .insert(
                            setting_node.name.as_bytes(),
                            value.get_u32(0).unwrap().to_le_bytes().as_ref(),
                        )
                        .unwrap();
                }
                PropertyType::Float32 => {
                    self.kvdb_tree
                        .insert(
                            setting_node.name.as_bytes(),
                            value.get_f32(0).unwrap().to_le_bytes().as_ref(),
                        )
                        .unwrap();
                }
                PropertyType::Str => {
                    self.kvdb_tree
                        .insert(setting_node.name.as_bytes(), value.get_str(0).unwrap().as_bytes())
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
        //TODO: Update this when multiple active_profiles at a time is supported
        self.add_setting(
            "net.outbound_connect_timeout",
            PropertyValue::Uint32(
                p2p_settings
                    .outbound_connect_timeout(&p2p_settings.active_profiles.first().unwrap())
                    as u32,
            ),
        );
        self.add_setting(
            "net.channel_handshake_timeout",
            PropertyValue::Uint32(
                p2p_settings
                    .channel_handshake_timeout(&p2p_settings.active_profiles.first().unwrap())
                    as u32,
            ),
        );
        self.add_setting(
            "net.channel_heartbeat_interval",
            PropertyValue::Uint32(
                p2p_settings
                    .channel_heartbeat_interval(&p2p_settings.active_profiles.first().unwrap())
                    as u32,
            ),
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
        //TODO: Update this when multiple active_profiles at a time is supported
        if let Some(profile) =
            p2p_settings.profiles.get_mut(p2p_settings.active_profiles.first().unwrap())
        {
            profile.outbound_connect_timeout = self
                .get_setting("net.outbound_connect_timeout")
                .unwrap()
                .get_property_u32("value")
                .unwrap() as u64;
            profile.channel_handshake_timeout = self
                .get_setting("net.channel_handshake_timeout")
                .unwrap()
                .get_property_u32("value")
                .unwrap() as u64;
            profile.channel_heartbeat_interval = self
                .get_setting("net.channel_heartbeat_interval")
                .unwrap()
                .get_property_u32("value")
                .unwrap() as u64;
        }
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
        p2p_settings.time_with_no_connections = self
            .get_setting("net.time_with_no_connections")
            .unwrap()
            .get_property_u32("value")
            .unwrap() as u64;
    }
}
