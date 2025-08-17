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

use crate::error::{Error, Result};
use darkfi_serial::{async_trait, Encodable, FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use std::{
    io::Write,
    sync::{Arc, Mutex as SyncMutex, Weak},
};

use crate::{
    expr::SExprCode,
    pubsub::{Publisher, PublisherPtr, Subscription},
    scene::{SceneNodeId, SceneNodeWeak},
};

mod guard;
pub use guard::{BatchGuardId, BatchGuardPtr, PropertyAtomicGuard};
mod wrap;
pub use wrap::{
    PropertyBool, PropertyColor, PropertyDimension, PropertyFloat32, PropertyRect, PropertyStr,
    PropertyUint32,
};

#[derive(Debug, Copy, Clone, PartialEq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum PropertyType {
    Null = 0,
    Bool = 1,
    Uint32 = 2,
    Float32 = 3,
    Str = 4,
    Enum = 5,
    SceneNodeId = 7,
    SExpr = 8,
}

impl PropertyType {
    fn default_value(&self) -> PropertyValue {
        match self {
            Self::Null => PropertyValue::Null,
            Self::Bool => PropertyValue::Bool(false),
            Self::Uint32 => PropertyValue::Uint32(0),
            Self::Float32 => PropertyValue::Float32(0.),
            Self::Str => PropertyValue::Str(String::new()),
            Self::Enum => PropertyValue::Enum(String::new()),
            Self::SceneNodeId => PropertyValue::SceneNodeId(0),
            Self::SExpr => PropertyValue::SExpr(Arc::new(vec![])),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum PropertySubType {
    Null = 0,
    Color = 1,
    // Size of something in pixels
    Pixel = 2,
    ResourceId = 3,
    Locale = 4,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Role {
    User = 0,
    App = 1,
    Internal = 2,
    Ignored = 3,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Unset,
    Null,
    Bool(bool),
    Uint32(u32),
    Float32(f32),
    Str(String),
    Enum(String),
    SceneNodeId(SceneNodeId),
    SExpr(Arc<SExprCode>),
}

impl PropertyValue {
    fn as_type(&self) -> PropertyType {
        match self {
            Self::Unset => todo!("not sure"),
            Self::Null => PropertyType::Null,
            Self::Bool(_) => PropertyType::Bool,
            Self::Uint32(_) => PropertyType::Uint32,
            Self::Float32(_) => PropertyType::Float32,
            Self::Str(_) => PropertyType::Str,
            Self::Enum(_) => PropertyType::Enum,
            Self::SceneNodeId(_) => PropertyType::SceneNodeId,
            Self::SExpr(_) => PropertyType::SExpr,
        }
    }

    pub fn is_unset(&self) -> bool {
        match self {
            Self::Unset => true,
            _ => false,
        }
    }

    pub fn is_null(&self) -> bool {
        match self {
            Self::Null => true,
            _ => false,
        }
    }

    pub fn is_expr(&self) -> bool {
        match self {
            Self::SExpr(_) => true,
            _ => false,
        }
    }

    pub fn as_bool(&self) -> Result<bool> {
        match self {
            Self::Bool(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    pub fn as_u32(&self) -> Result<u32> {
        match self {
            Self::Uint32(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    pub fn as_f32(&self) -> Result<f32> {
        match self {
            Self::Float32(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    pub fn as_str(&self) -> Result<String> {
        match self {
            Self::Str(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
    pub fn as_enum(&self) -> Result<String> {
        match self {
            Self::Enum(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
    pub fn as_node_id(&self) -> Result<SceneNodeId> {
        match self {
            Self::SceneNodeId(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    pub fn as_sexpr(&self) -> Result<Arc<SExprCode>> {
        match self {
            Self::SExpr(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
}

impl Encodable for PropertyValue {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        match self {
            Self::Unset | Self::Null => {
                // do nothing
                Ok(0)
            }
            Self::Bool(v) => v.encode(s),
            Self::Uint32(v) => v.encode(s),
            Self::Float32(v) => v.encode(s),
            Self::Str(v) => v.encode(s),
            Self::Enum(v) => v.encode(s),
            Self::SceneNodeId(v) => v.encode(s),
            Self::SExpr(v) => v.encode(s),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModifyAction {
    Clear,
    Set(usize),
    SetCache(Vec<usize>),
    Push(usize),
}

type ModifyPublisher = PublisherPtr<(Role, ModifyAction, BatchGuardPtr)>;

pub type PropertyPtr = Arc<Property>;
pub type PropertyWeak = Weak<Property>;

#[derive(Debug, Clone)]
pub struct PropertyDepend {
    pub prop: PropertyWeak,
    pub i: usize,
    pub local_name: String,
}

pub struct Property {
    pub name: String,
    pub node: SyncMutex<Option<SceneNodeWeak>>,
    pub typ: PropertyType,
    pub subtype: PropertySubType,
    pub defaults: Vec<PropertyValue>,
    // either a value or an expr must be set
    pub vals: SyncMutex<Vec<PropertyValue>>,
    // only used valid when PropertyValue is an expr
    // caches the last calculated value
    pub cache: SyncMutex<Vec<PropertyValue>>,
    pub ui_name: String,
    pub desc: String,

    pub is_null_allowed: bool,
    pub is_expr_allowed: bool,

    // Use 0 for unbounded length
    pub array_len: usize,
    pub min_val: Option<PropertyValue>,
    pub max_val: Option<PropertyValue>,

    // PropertyType must be Enum
    pub enum_items: Option<Vec<String>>,

    on_modify: ModifyPublisher,
    depends: SyncMutex<Vec<PropertyDepend>>,
}

impl Property {
    pub fn new<S: Into<String>>(name: S, typ: PropertyType, subtype: PropertySubType) -> Self {
        Self {
            name: name.into(),
            node: SyncMutex::new(None),
            typ,
            subtype,

            defaults: vec![typ.default_value()],
            vals: SyncMutex::new(vec![PropertyValue::Unset]),
            cache: SyncMutex::new(vec![PropertyValue::Null]),

            ui_name: String::new(),
            desc: String::new(),

            is_null_allowed: false,
            is_expr_allowed: false,

            array_len: 1,
            min_val: None,
            max_val: None,
            enum_items: None,

            on_modify: Publisher::new(),
            depends: SyncMutex::new(vec![]),
        }
    }

    /// Just used for debugging
    pub fn set_parent(&self, node: SceneNodeWeak) {
        *self.node.lock().unwrap() = Some(node);
    }

    pub fn set_ui_text<S: Into<String>>(&mut self, ui_name: S, desc: S) {
        self.ui_name = ui_name.into();
        self.desc = desc.into();
    }

    pub fn set_array_len(&mut self, len: usize) {
        self.array_len = len;
        self.defaults.resize(len, self.typ.default_value());
        self.defaults.shrink_to_fit();

        let vals = &mut *self.vals.lock().unwrap();
        vals.resize(len, PropertyValue::Unset);
        vals.shrink_to_fit();

        let cache = &mut *self.cache.lock().unwrap();
        cache.resize(len, PropertyValue::Null);
        cache.shrink_to_fit();
    }
    pub fn set_unbounded(&mut self) {
        self.set_array_len(0);
    }

    pub fn set_range_u32(&mut self, min: u32, max: u32) {
        self.min_val = Some(PropertyValue::Uint32(min));
        self.max_val = Some(PropertyValue::Uint32(max));
    }
    pub fn set_range_f32(&mut self, min: f32, max: f32) {
        self.min_val = Some(PropertyValue::Float32(min));
        self.max_val = Some(PropertyValue::Float32(max));
    }

    pub fn set_enum_items<S: Into<String>>(&mut self, enum_items: Vec<S>) -> Result<()> {
        if self.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }
        self.enum_items = Some(enum_items.into_iter().map(|item| item.into()).collect());
        Ok(())
    }

    pub fn allow_null_values(&mut self) {
        self.is_null_allowed = true;
    }

    pub fn allow_exprs(&mut self) {
        self.is_expr_allowed = true;
    }

    fn check_defaults_len(&self, defaults_len: usize) -> Result<()> {
        if !self.is_bounded() || defaults_len != self.array_len {
            return Err(Error::PropertyWrongLen)
        }
        Ok(())
    }
    pub fn set_defaults_bool(&mut self, defaults: Vec<bool>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        self.defaults = defaults.into_iter().map(|v| PropertyValue::Bool(v)).collect();
        Ok(())
    }
    pub fn set_defaults_u32(&mut self, defaults: Vec<u32>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        self.defaults = defaults.into_iter().map(|v| PropertyValue::Uint32(v)).collect();
        Ok(())
    }
    pub fn set_defaults_f32(&mut self, defaults: Vec<f32>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        self.defaults = defaults.into_iter().map(|v| PropertyValue::Float32(v)).collect();
        Ok(())
    }
    pub fn set_defaults_str(&mut self, defaults: Vec<String>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        self.defaults = defaults.into_iter().map(|v| PropertyValue::Str(v)).collect();
        Ok(())
    }
    pub fn set_defaults_null(&mut self) -> Result<()> {
        if !self.is_null_allowed {
            return Err(Error::PropertyNullNotAllowed)
        }
        if !self.is_bounded() {
            return Err(Error::PropertyWrongLen)
        }
        self.defaults = (0..self.array_len).map(|_| PropertyValue::Null).collect();
        Ok(())
    }

    // Set

    /// This will clear all values, resetting them to the default
    pub fn clear_values(self: Arc<Self>, atom: &mut PropertyAtomicGuard, role: Role) {
        {
            let vals = &mut self.vals.lock().unwrap();
            vals.clear();
            vals.resize(self.array_len, PropertyValue::Unset);
        }
        atom.add(self, role, ModifyAction::Clear);
    }

    fn set_raw_value(&self, i: usize, val: PropertyValue) -> Result<()> {
        if self.typ != val.as_type() {
            return Err(Error::PropertyWrongType)
        }

        let vals = &mut self.vals.lock().unwrap();
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        vals[i] = val;
        Ok(())
    }

    pub fn unset(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
    ) -> Result<()> {
        {
            let vals = &mut self.vals.lock().unwrap();
            if i >= vals.len() {
                return Err(Error::PropertyWrongIndex)
            }
            vals[i] = PropertyValue::Unset;
        }
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }

    pub fn set_null(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
    ) -> Result<()> {
        if !self.is_null_allowed {
            return Err(Error::PropertyNullNotAllowed)
        }

        let mut vals = self.vals.lock().unwrap();
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        vals[i] = PropertyValue::Null;
        drop(vals);

        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }

    pub fn set_bool(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: bool,
    ) -> Result<()> {
        self.set_raw_value(i, PropertyValue::Bool(val))?;
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_u32(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: u32,
    ) -> Result<()> {
        if self.min_val.is_some() {
            let min = self.min_val.as_ref().unwrap().as_u32()?;
            if val < min {
                return Err(Error::PropertyOutOfRange)
            }
        }
        if self.max_val.is_some() {
            let max = self.max_val.as_ref().unwrap().as_u32()?;
            if val > max {
                return Err(Error::PropertyOutOfRange)
            }
        }
        self.set_raw_value(i, PropertyValue::Uint32(val))?;
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_f32(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: f32,
    ) -> Result<()> {
        if self.min_val.is_some() {
            let min = self.min_val.as_ref().unwrap().as_f32()?;
            if val < min {
                return Err(Error::PropertyOutOfRange)
            }
        }
        if self.max_val.is_some() {
            let max = self.max_val.as_ref().unwrap().as_f32()?;
            if val > max {
                return Err(Error::PropertyOutOfRange)
            }
        }
        self.set_raw_value(i, PropertyValue::Float32(val))?;
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_str<S: Into<String>>(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: S,
    ) -> Result<()> {
        self.set_raw_value(i, PropertyValue::Str(val.into()))?;
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_enum<S: Into<String>>(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: S,
    ) -> Result<()> {
        if self.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }
        let val = val.into();
        if !self.enum_items.as_ref().unwrap().contains(&val) {
            return Err(Error::PropertyWrongEnumItem)
        }
        self.set_raw_value(i, PropertyValue::Enum(val.into()))?;
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_node_id(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: SceneNodeId,
    ) -> Result<()> {
        self.set_raw_value(i, PropertyValue::SceneNodeId(val))?;
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_expr(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: SExprCode,
    ) -> Result<()> {
        {
            if !self.is_expr_allowed {
                return Err(Error::PropertySExprNotAllowed)
            }
            let vals = &mut self.vals.lock().unwrap();
            if i >= vals.len() {
                return Err(Error::PropertyWrongIndex)
            }
            vals[i] = PropertyValue::SExpr(Arc::new(val));
        }
        atom.add(self, role, ModifyAction::Set(i));
        Ok(())
    }

    pub fn set_f32_vec(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<f32>,
    ) -> Result<()> {
        for (i, &val) in val.iter().enumerate() {
            self.clone().set_f32(atom, role, i, val)?;
        }
        Ok(())
    }

    fn set_cache(&self, i: usize, val: PropertyValue) -> Result<()> {
        if self.typ != val.as_type() {
            return Err(Error::PropertyWrongType)
        }

        let cache = &mut self.cache.lock().unwrap();
        if i >= cache.len() {
            return Err(Error::PropertyWrongIndex)
        }
        cache[i] = val;
        Ok(())
    }
    pub fn set_cache_f32(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: f32,
    ) -> Result<()> {
        self.set_cache(i, PropertyValue::Float32(val))?;
        atom.add(self, role, ModifyAction::SetCache(vec![i]));
        Ok(())
    }
    pub fn set_cache_u32(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: u32,
    ) -> Result<()> {
        self.set_cache(i, PropertyValue::Uint32(val))?;
        atom.add(self, role, ModifyAction::SetCache(vec![i]));
        Ok(())
    }

    pub fn set_cache_f32_multi(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        changes: Vec<(usize, f32)>,
    ) -> Result<()> {
        let mut idxs = vec![];
        for (idx, val) in changes {
            self.set_cache(idx, PropertyValue::Float32(val))?;
            idxs.push(idx);
        }
        atom.add(self, role, ModifyAction::SetCache(idxs));
        Ok(())
    }
    pub fn set_cache_u32_range(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        changes: Vec<(usize, u32)>,
    ) -> Result<()> {
        let mut idxs = vec![];
        for (idx, val) in changes {
            self.set_cache(idx, PropertyValue::Uint32(val))?;
            idxs.push(idx);
        }
        atom.add(self, role, ModifyAction::SetCache(idxs));
        Ok(())
    }

    // Push

    fn push_value(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        value: PropertyValue,
    ) -> Result<usize> {
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }

        let mut vals = self.vals.lock().unwrap();
        let i = vals.len();
        vals.push(value);
        drop(vals);

        atom.add(self, role, ModifyAction::Push(i));
        Ok(i)
    }

    pub fn push_null(self: Arc<Self>, atom: &mut PropertyAtomicGuard, role: Role) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Null)
    }
    pub fn push_bool(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: bool,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Bool(val))
    }
    pub fn push_u32(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: u32,
    ) -> Result<usize> {
        // TODO: none of these push calls are enforcing constraints that are required
        // see the set_XX calls.
        self.push_value(atom, role, PropertyValue::Uint32(val))
    }
    pub fn push_f32(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: f32,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Float32(val))
    }
    pub fn push_str<S: Into<String>>(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: S,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Str(val.into()))
    }
    pub fn push_enum<S: Into<String>>(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: S,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Enum(val.into()))
    }
    pub fn push_node_id(
        self: Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: SceneNodeId,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::SceneNodeId(val))
    }

    // Get

    pub fn is_bounded(&self) -> bool {
        self.array_len != 0
    }

    pub fn get_len(&self) -> usize {
        // Avoid locking unless we need to
        // If array len is nonzero, then vals len should be the same.
        if !self.is_bounded() {
            return self.vals.lock().unwrap().len()
        }
        self.array_len
    }

    pub fn is_unset(&self, i: usize) -> Result<bool> {
        let val = self.get_raw_value(i)?;
        Ok(val.is_unset())
    }
    pub fn is_null(&self, i: usize) -> Result<bool> {
        let val = self.get_value(i)?;
        if val.is_unset() {
            return Ok(self.defaults[i].is_null())
        }
        Ok(val.is_null())
    }

    pub fn is_expr(&self, i: usize) -> Result<bool> {
        if !self.is_expr_allowed {
            return Ok(false)
        }
        let val = self.get_raw_value(i)?;
        Ok(val.is_expr())
    }

    pub fn get_raw_value(&self, i: usize) -> Result<PropertyValue> {
        let vals = &self.vals.lock().unwrap();
        if self.is_bounded() {
            assert_eq!(vals.len(), self.array_len);
        }
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        let val = vals[i].clone();
        Ok(val)
    }

    pub fn get_value(&self, i: usize) -> Result<PropertyValue> {
        let val = self.get_raw_value(i)?;
        if val.is_expr() {
            let cached = self.get_cached(i)?;
            if cached.is_null() {
                return Ok(self.defaults[i].clone())
            }
            return Ok(cached)
        }
        if val.is_unset() {
            return Ok(self.defaults[i].clone())
        }
        Ok(val)
    }

    pub fn get_bool(&self, i: usize) -> Result<bool> {
        self.get_value(i)?.as_bool()
    }
    pub fn get_bool_opt(&self, i: usize) -> Result<Option<bool>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_bool()?))
    }
    pub fn get_u32(&self, i: usize) -> Result<u32> {
        self.get_value(i)?.as_u32()
    }
    pub fn get_u32_opt(&self, i: usize) -> Result<Option<u32>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_u32()?))
    }
    pub fn get_f32(&self, i: usize) -> Result<f32> {
        self.get_value(i)?.as_f32()
    }
    pub fn get_f32_opt(&self, i: usize) -> Result<Option<f32>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_f32()?))
    }
    pub fn get_str(&self, i: usize) -> Result<String> {
        self.get_value(i)?.as_str()
    }
    pub fn get_str_opt(&self, i: usize) -> Result<Option<String>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_str()?))
    }
    pub fn get_enum(&self, i: usize) -> Result<String> {
        self.get_value(i)?.as_enum()
    }
    pub fn get_enum_opt(&self, i: usize) -> Result<Option<String>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_enum()?))
    }
    pub fn get_node_id(&self, i: usize) -> Result<SceneNodeId> {
        self.get_value(i)?.as_node_id()
    }
    pub fn get_node_id_opt(&self, i: usize) -> Result<Option<SceneNodeId>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_node_id()?))
    }

    pub fn get_expr(&self, i: usize) -> Result<Arc<SExprCode>> {
        self.get_raw_value(i)?.as_sexpr()
    }

    pub fn get_cached(&self, i: usize) -> Result<PropertyValue> {
        let cache = &self.cache.lock().unwrap();
        if self.is_bounded() {
            assert_eq!(cache.len(), self.array_len);
        }
        if i >= cache.len() {
            return Err(Error::PropertyWrongIndex)
        }
        Ok(cache[i].clone())
    }

    // Subs

    pub fn subscribe_modify(&self) -> Subscription<(Role, ModifyAction, BatchGuardPtr)> {
        self.on_modify.clone().subscribe()
    }

    // Dependencies

    pub fn add_depend<S: Into<String>>(&self, prop: &PropertyPtr, i: usize, local_name: S) {
        self.depends.lock().unwrap().push(PropertyDepend {
            prop: Arc::downgrade(prop),
            i,
            local_name: local_name.into(),
        });
    }

    pub fn get_depends(&self) -> Vec<PropertyDepend> {
        self.depends.lock().unwrap().clone()
    }
}

impl std::fmt::Debug for Property {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let node = {
            let mut null_name = || write!(f, "<null>:{}", self.name);
            let Ok(node) = self.node.lock() else { return null_name() };
            let Some(node) = node.clone() else { return null_name() };
            let Some(node) = node.upgrade() else { return null_name() };
            node
        };
        write!(f, "{:?}:{}", node, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Op;

    #[test]
    fn test_getset() {
        let prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop.set_f32(Role::App, 1, 4.).is_err());
        assert!(prop.is_unset(0).unwrap());
        assert!(prop.set_f32(Role::App, 0, 4.).is_ok());
        assert_eq!(prop.get_f32(0).unwrap(), 4.);
        assert!(!prop.is_unset(0).unwrap());
        prop.unset(Role::App, 0).unwrap();
        assert!(prop.is_unset(0).unwrap());
        assert_eq!(prop.get_f32(0).unwrap(), 0.);
    }

    #[test]
    fn test_nullable() {
        // default len is 1
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop.set_defaults_f32(vec![1.0, 0.0]).is_err());
        assert!(prop.set_defaults_f32(vec![2.0]).is_ok());
        prop.allow_null_values();
        prop.set_null(Role::App, 0).unwrap();

        assert!(prop.get_f32_opt(1).is_err());
        assert!(prop.get_f32_opt(0).is_ok());
        assert!(prop.get_f32_opt(0).unwrap().is_none());

        prop.clear_values(Role::App);
        assert!(prop.get_f32(0).is_ok());
        assert!(prop.get_f32_opt(0).unwrap().is_some());
        assert_eq!(prop.get_f32(0).unwrap(), 2.0);
    }

    #[test]
    fn test_nonnullable() {
        let prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop.set_null(Role::App, 0).is_err());
        assert!(prop.is_unset(0).unwrap());
    }

    #[test]
    fn test_unbounded() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        prop.set_unbounded();
        assert_eq!(prop.get_len(), 0);
        prop.push_f32(Role::App, 2.0).unwrap();
        prop.push_f32(Role::App, 3.0).unwrap();
        assert_eq!(prop.get_len(), 2);

        prop.clear_values(Role::App);
        assert_eq!(prop.get_len(), 0);
        prop.allow_null_values();
        prop.push_null(Role::App).unwrap();
        prop.push_f32(Role::App, 4.0).unwrap();
        prop.push_f32(Role::App, 5.0).unwrap();
        assert_eq!(prop.get_len(), 3);
        assert!(prop.get_f32_opt(0).unwrap().is_none());
        assert!(prop.get_f32_opt(1).unwrap().is_some());
        assert!(prop.get_f32_opt(2).unwrap().is_some());
        assert!(prop.get_f32_opt(3).is_err());

        let prop2 = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop2.push_f32(Role::App, 4.0).is_err());
    }

    #[test]
    fn test_range() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        let half_pi = 3.1415926535 / 2.;
        prop.set_range_f32(-half_pi, half_pi);
        assert!(prop.set_f32(Role::App, 0, 6.).is_err());
        assert!(prop.set_f32(Role::App, 0, 1.).is_ok());
    }

    #[test]
    fn test_enum() {
        let mut prop = Property::new("foo", PropertyType::Enum, PropertySubType::Null);
        prop.set_enum_items(vec!["ABC", "XYZ", "FOO"]).unwrap();
        assert!(prop.set_enum(Role::App, 0, "ABC").is_ok());
        assert!(prop.set_enum(Role::App, 0, "BAR").is_err());
    }

    #[test]
    fn test_expr() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        prop.allow_exprs();
        assert_eq!(prop.get_f32(0).unwrap(), 0.);
        let code = vec![Op::ConstFloat32(4.)];
        prop.set_expr(Role::App, 0, code).unwrap();
        let val = prop.get_cached(0).unwrap();
        assert!(val.is_null());
        prop.set_cache_f32(Role::App, 0, 4.).unwrap();
        let val = prop.get_cached(0).unwrap();
        assert_eq!(val.as_f32().unwrap(), 4.);
    }
}
