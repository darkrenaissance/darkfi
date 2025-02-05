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

use async_trait::async_trait;
use std::sync::Arc;

use crate::ExecutorPtr;

pub mod darkirc;
pub use darkirc::{DarkIrc, DarkIrcPtr};

use sled_overlay::sled;
use crate::{
    scene::{MethodCallSub, Pimpl, SceneNode, SceneNodeType, SceneNodePtr, SceneNodeWeak},
    prop::{Property, PropertyAtomicGuard, PropertyStr, PropertyPtr, PropertyType, PropertySubType, PropertyValue, Role},
};
use std::array::TryFromSliceError;
use std::string::FromUtf8Error;

#[async_trait]
pub trait PluginObject {
    async fn start(self: Arc<Self>, ex: ExecutorPtr) {}
}

pub struct PluginSettings {
    setting_root: SceneNodePtr,
    sled_tree: sled::Tree,
}
impl PluginSettings {
    fn add_setting(&self, name: &str, default: PropertyValue) -> Option<SceneNodePtr> {
        let atom = &mut PropertyAtomicGuard::new();
        let node = match default {
            PropertyValue::Bool(b) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Bool, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Bool, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_bool(atom, Role::User, "value", b.clone());
                node.set_property_bool(atom, Role::App, "default", b.clone());
                Some(node)
            }
            PropertyValue::Uint32(u) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Uint32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Uint32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_u32(atom, Role::User, "value", u.clone());
                node.set_property_u32(atom, Role::App, "default", u.clone());
                Some(node)
            }
            PropertyValue::Float32(f) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Float32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Float32, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_f32(atom, Role::User, "value", f.clone());
                node.set_property_f32(atom, Role::App, "default", f.clone());
                Some(node)
            }
            PropertyValue::Str(s) => {
                let mut node = SceneNode::new(name, SceneNodeType::Setting);
                let prop = Property::new("value", PropertyType::Str, PropertySubType::Null);
                node.add_property(prop).unwrap();
                let prop = Property::new("default", PropertyType::Str, PropertySubType::Null);
                node.add_property(prop).unwrap();
                node.set_property_str(atom, Role::User, "value", s.clone());
                node.set_property_str(atom, Role::App, "default", s.clone());
                Some(node)
            }
            _ => { None }
        };

        match node {
            Some(n) => {
                let node_ptr = Arc::new(n);
                self.setting_root.clone().link(node_ptr.clone().into());
                Some(node_ptr)
            }
            None => None
        }
    }

    // For all settings, copy the value from sled into the setting node's value property
    fn load_settings(&self) {
        let atom = &mut PropertyAtomicGuard::new();
        for setting_node in self.setting_root.get_children().iter() {
            if setting_node.typ != SceneNodeType::Setting {
                continue;
            }

            let value = setting_node.get_property("value").clone().unwrap();
            match value.typ {
                PropertyType::Bool => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        setting_node.set_property_bool(atom, Role::User, "value", sled_value[0] != 0);
                    }
                }
                PropertyType::Uint32 => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        if sled_value.len() == 4 {
                            let bytes: Result<[u8; 4], TryFromSliceError> = sled_value.as_ref().try_into();
                            if let Ok(b) = bytes {
                                setting_node.set_property_u32(atom, Role::User, "value", u32::from_le_bytes(b));
                            }
                        }
                    }
                }
                PropertyType::Float32 => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        if sled_value.len() == 4 {
                            let bytes: Result<[u8; 4], TryFromSliceError> = sled_value.as_ref().try_into();
                            if let Ok(b) = bytes {
                                setting_node.set_property_f32(atom, Role::User, "value", f32::from_le_bytes(b));
                            }
                        }
                    }
                }
                PropertyType::Str => {
                    let sled_result = self.sled_tree.get(setting_node.name.as_str());
                    if let Ok(Some(sled_value)) = sled_result {
                        let string: Result<String, FromUtf8Error> = String::from_utf8(sled_value.to_vec());
                        if let Ok(s) = string {
                            setting_node.set_property_str(atom, Role::User, "value", s);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Save all settings to sled
    fn save_settings(&self) {
        for setting_node in self.setting_root.get_children().iter() {
            if setting_node.typ != SceneNodeType::Setting {
                continue;
            }

            let value = setting_node.get_property("value").clone().unwrap();
            match value.typ {
                PropertyType::Bool => {
                    let value_bytes = if value.get_bool(0).unwrap() { 1u8 } else { 0u8 };
                    self.sled_tree.insert(setting_node.name.as_str(), sled::IVec::from(vec![value_bytes]));
                }
                PropertyType::Uint32 => {
                    self.sled_tree.insert(setting_node.name.as_str(), sled::IVec::from(value.get_u32(0).unwrap().to_le_bytes().as_ref()));
                }
                PropertyType::Float32 => {
                    self.sled_tree.insert(setting_node.name.as_str(), sled::IVec::from(value.get_f32(0).unwrap().to_le_bytes().as_ref()));
                }
                PropertyType::Str => {
                    self.sled_tree.insert(setting_node.name.as_str(), sled::IVec::from(value.get_str(0).unwrap().as_bytes()));
                }
                _ => {}
            }
        }
    }
}
