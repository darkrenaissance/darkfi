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

use std::{
    io::Cursor,
    sync::{Arc, Mutex as SyncMutex},
};

use darkfi_serial::{Decodable, Encodable, VarInt};
use kvdb_overlay::Tree;

use crate::{
    error::{Error, Result},
    prop::{
        Property, PropertyAtomicGuard, PropertyPtr, PropertySubType, PropertyType, PropertyValue,
        Role,
    },
    scene::{CallArgType, Pimpl, SceneNode, SceneNodeType, SceneNodeWeak},
    ExecutorPtr,
};

/// Settings when modified are persisted otherwise they use their default
/// as expected by properties.
///
/// Settings can be used directly by other nodes or as a dependency which
/// can be used in expressions.
///
/// Settings are set by the user with `Role::User`.
///
/// For example `net.enable_tor` might be implicitly used by `/plugin/darkirc`
/// while some other setting might be used in the schema itself with the node
/// not being aware its depending on an external property.
///
/// In both cases modifying the setting should propagate the changes to that node.
///
/// Although the `/setting2` root has no knowledge of property paths underneath there
/// is a convention of using `foo.bar.baz` to namespace the settings.
pub fn create_setting(name: &str) -> SceneNode {
    let mut node = SceneNode::new(name, SceneNodeType::Setting);

    // Example
    let prop = Property::new("net.enable_tor", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("win.scale", PropertyType::Float32, PropertySubType::Null);
    prop.set_defaults_f32(vec![1.]).unwrap();
    prop.set_range_f32(0., f32::MAX);
    node.add_property(prop).unwrap();

    node.add_method(
        "search",
        vec![("filter", "Filter string to search keys", CallArgType::Str)],
        None,
    )
    .unwrap();

    node
}

pub type SettingPtr = Arc<Setting>;

pub struct Setting {
    tasks: SyncMutex<Vec<smol::Task<()>>>,
}

impl Setting {
    pub async fn new(node: SceneNodeWeak, db_tree: Tree, ex: ExecutorPtr) -> Pimpl {
        let node_ref = node.upgrade().unwrap();

        // Load any persisted properties from the db.
        for entry in db_tree.iter() {
            let (key, data) = entry.unwrap();
            let key = String::from_utf8(key).unwrap();
            let prop = node_ref.get_property(&key).unwrap();
            Self::load_prop(&prop, &data).unwrap();
        }

        // Spawn tasks persisting our properties to the db when they change.
        let mut tasks = vec![];
        for prop in &node_ref.props {
            let db_tree2 = db_tree.clone();
            let prop2 = prop.clone();
            let on_modify_sub = prop.subscribe_modify();
            let task = ex.spawn(async move {
                while let Ok((_role, _action, _guard)) = on_modify_sub.receive().await {
                    Self::save_prop(&prop2, &db_tree2).unwrap();
                }
            });
            tasks.push(task);
        }

        Pimpl::Setting(Arc::new(Self { tasks: SyncMutex::new(tasks) }))
    }

    /// Persist the state of a property under its name as the db key.
    /// The db value is a list of (prop_idx, prop_value) pairs exactly
    /// specifying which idxs inside the property are set. If none are
    /// set the key is dropped.
    fn save_prop(prop: &PropertyPtr, db_tree: &Tree) -> Result<()> {
        assert!(prop.is_bounded());

        let mut data = vec![];
        let mut is_set = false;

        for i in 0..prop.get_len() {
            if prop.is_unset(i)? {
                continue
            }

            is_set = true;
            let val = prop.get_value(i)?;
            VarInt(i as u64).encode(&mut data)?;
            Self::encode_value(prop, &val, &mut data)?;
        }

        if is_set {
            db_tree.insert(prop.name.as_bytes(), &data)?;
        } else {
            db_tree.remove(prop.name.as_bytes())?;
        }

        Ok(())
    }

    /// Serialize a single value. The property type determines the binary
    /// format of the value itself. If the property allows null values then
    /// it is written as an option (`Option<X>`): a single tag byte followed
    /// by the value only when it is not null.
    fn encode_value(prop: &Property, val: &PropertyValue, data: &mut Vec<u8>) -> Result<()> {
        if prop.is_null_allowed {
            match val {
                PropertyValue::Null => {
                    false.encode(data)?;
                }
                val => {
                    true.encode(data)?;
                    val.encode(data)?;
                }
            }
        } else {
            val.encode(data)?;
        }

        Ok(())
    }

    /// Restore a property state previously written by `Self::save_prop()`.
    fn load_prop(prop: &PropertyPtr, data: &[u8]) -> Result<()> {
        assert!(prop.is_bounded());

        let mut cur = Cursor::new(data);
        let atom = &mut PropertyAtomicGuard::none();

        while (cur.position() as usize) < data.len() {
            let i = VarInt::decode(&mut cur)?.0 as usize;
            let val = Self::decode_value(prop, &mut cur)?;
            Self::apply_value(prop, atom, i, val)?;
        }

        Ok(())
    }

    /// Decode a value serialized by `Self::encode_value()`.
    fn decode_value(prop: &Property, cur: &mut Cursor<&[u8]>) -> Result<PropertyValue> {
        macro_rules! decode_ty {
            ($typ:ty, $variant:ident) => {{
                if prop.is_null_allowed {
                    match Option::<$typ>::decode(cur)? {
                        Some(v) => PropertyValue::$variant(v),
                        None => PropertyValue::Null,
                    }
                } else {
                    PropertyValue::$variant(<$typ>::decode(cur)?)
                }
            }};
        }

        let val = match prop.typ {
            PropertyType::Bool => decode_ty!(bool, Bool),
            PropertyType::Uint32 => decode_ty!(u32, Uint32),
            PropertyType::Float32 => decode_ty!(f32, Float32),
            PropertyType::Str => decode_ty!(String, Str),
            PropertyType::Enum => decode_ty!(String, Enum),
            PropertyType::SceneNodeId => decode_ty!(u32, SceneNodeId),
            PropertyType::Null | PropertyType::SExpr => return Err(Error::PropertyWrongType),
        };

        Ok(val)
    }

    /// Set the value at index `i` as `Role::User`.
    fn apply_value(
        prop: &PropertyPtr,
        atom: &mut PropertyAtomicGuard,
        i: usize,
        val: PropertyValue,
    ) -> Result<()> {
        match val {
            PropertyValue::Bool(v) => prop.set_bool(atom, Role::User, i, v),
            PropertyValue::Uint32(v) => prop.set_u32(atom, Role::User, i, v),
            PropertyValue::Float32(v) => prop.set_f32(atom, Role::User, i, v),
            PropertyValue::Str(v) => prop.set_str(atom, Role::User, i, v),
            PropertyValue::Enum(v) => prop.set_enum(atom, Role::User, i, v),
            PropertyValue::SceneNodeId(v) => prop.set_node_id(atom, Role::User, i, v),
            PropertyValue::Null => prop.set_null(atom, Role::User, i),
            PropertyValue::Unset | PropertyValue::SExpr(_) => Err(Error::PropertyWrongType),
        }
    }
}

impl Drop for Setting {
    fn drop(&mut self) {
        self.tasks.lock().unwrap().clear();
    }
}
