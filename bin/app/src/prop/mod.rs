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
    ui::VectorShape,
};

mod guard;
pub use guard::{BatchGuard, BatchGuardPtr, PropertyAtomicGuard};
mod wrap;
pub use wrap::{
    eval_f32_multi, PropertyBool, PropertyColor, PropertyDimension, PropertyEnum, PropertyFloat32,
    PropertyRect, PropertyShape, PropertyStr, PropertyUint32,
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
    VectorShape = 9,
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
            Self::VectorShape => PropertyValue::VectorShape(Arc::new(VectorShape::new())),
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
    Flag = 5,
}

/// Acting role on a property mutation. Doubles as a bitmask for
/// `PropertyPermission` read/write masks, so it is a hand-rolled bitflag
/// set over `u8` (kept dependency-free on purpose).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Role(u8);

impl Role {
    // Constants keep CamelCase names on purpose: they are used as
    // `Role::App` etc. at hundreds of call sites, mimicking the old
    // enum-variant ergonomics.
    #![allow(non_upper_case_globals)]

    /// End-user action (UI input, settings)
    pub const User: Role = Role(1 << 0);
    /// Application/schema logic
    pub const App: Role = Role(1 << 1);
    /// Widget-internal writes (draw-pass evals, runtime-computed state)
    pub const Internal: Role = Role(1 << 2);
    /// Marker role: "don't notify", rarely part of masks
    pub const Ignored: Role = Role(1 << 3);
    /// Theme engine writes (stamped by `ThemeCtx` setters)
    pub const Theme: Role = Role(1 << 4);

    /// The empty mask
    pub const NONE: Role = Role(0);
    /// All roles
    pub const ALL: Role = Role(0b1_1111);

    /// True when every bit of `other` is also set in `self`.
    pub fn contains(self, other: Role) -> bool {
        other.0 & self.0 == other.0
    }

    /// True when `self` and `other` share at least one bit.
    pub fn intersects(self, other: Role) -> bool {
        self.0 & other.0 != 0
    }

    /// Union of two role sets.
    pub const fn union(self, other: Role) -> Role {
        Role(self.0 | other.0)
    }
}

impl std::ops::BitOr for Role {
    type Output = Role;
    fn bitor(self, rhs: Role) -> Role {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for Role {
    fn bitor_assign(&mut self, rhs: Role) {
        self.0 |= rhs.0;
    }
}

/// Read/write access masks for a property. Supply at creation; treat as
/// immutable afterwards. The default allows every role, which preserves
/// the pre-permission behavior for call sites that have not been
/// assigned real masks yet.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PropertyPermission {
    /// Roles allowed to read.
    pub read: Role,
    /// Roles allowed to write (set/unset/push/insert/remove/expr).
    pub write: Role,
}

impl Default for PropertyPermission {
    fn default() -> Self {
        Self { read: Role::ALL, write: Role::ALL }
    }
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
    VectorShape(Arc<VectorShape>),
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
            Self::VectorShape(_) => PropertyType::VectorShape,
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

    pub fn as_shape(&self) -> Result<Arc<VectorShape>> {
        match self {
            Self::VectorShape(v) => Ok(v.clone()),
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
            Self::VectorShape(v) => v.encode(s),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModifyAction {
    Clear,
    Set(usize),
    SetVec,
    SetCache(Vec<usize>),
    Unset(usize),
    Push(usize),
    Insert(usize),
    Remove(usize, PropertyValue),
    /// The property's dependency list changed (edge added or removed).
    /// Control-plane, not a value mutation: `when_change` loops use it to
    /// resync their poll sets (design D5). Consumers that persist or
    /// forward value changes should ignore it.
    DependsChanged,
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
    // Defaults are construction-time metadata, but they can also be
    // installed post-creation (see `set_default_*`), so access is
    // synchronized like `vals`.
    pub defaults: SyncMutex<Vec<PropertyValue>>,
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

    pub permission: PropertyPermission,

    on_modify: ModifyPublisher,
    depends: SyncMutex<Vec<PropertyDepend>>,
}

impl Property {
    pub fn new<S: Into<String>>(
        name: S,
        typ: PropertyType,
        subtype: PropertySubType,
        permission: PropertyPermission,
    ) -> Self {
        Self {
            name: name.into(),
            node: SyncMutex::new(None),
            typ,
            subtype,

            defaults: SyncMutex::new(vec![typ.default_value()]),
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

            permission,

            on_modify: Publisher::new(),
            depends: SyncMutex::new(vec![]),
        }
    }

    /// Just used for debugging
    pub fn set_parent(&self, node: SceneNodeWeak) {
        *self.node.lock().unwrap() = Some(node);
    }

    /// Read-mask check for the acting role.
    #[inline]
    pub fn can_read(&self, role: Role) -> bool {
        self.permission.read.contains(role)
    }

    /// Write-mask check for the acting role.
    #[inline]
    pub fn can_write(&self, role: Role) -> bool {
        self.permission.write.contains(role)
    }

    /// Central write enforcement: called at the top of every mutating
    /// API, before any mutation or guard journaling, so a denial leaves
    /// the property untouched. Exempt (by design): `set_default_*`
    /// (construction metadata), `set_cache_*` (derived eval artifacts),
    /// and `add_depend` (wiring metadata).
    #[inline]
    fn check_write(&self, role: Role) -> Result<()> {
        if self.can_write(role) {
            Ok(())
        } else {
            Err(Error::PropertyPermissionDenied)
        }
    }

    pub fn set_ui_text<S: Into<String>>(&mut self, ui_name: S, desc: S) {
        self.ui_name = ui_name.into();
        self.desc = desc.into();
    }

    pub fn set_array_len(&mut self, len: usize) {
        self.array_len = len;
        {
            let defaults = &mut self.defaults.lock().unwrap();
            defaults.resize(len, self.typ.default_value());
            defaults.shrink_to_fit();
        }

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
        if self.is_bounded() && defaults_len != self.array_len {
            return Err(Error::PropertyWrongLen)
        }
        Ok(())
    }
    pub fn set_defaults_bool(&mut self, defaults: Vec<bool>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        *self.defaults.lock().unwrap() =
            defaults.into_iter().map(|v| PropertyValue::Bool(v)).collect();
        Ok(())
    }
    pub fn set_defaults_u32(&mut self, defaults: Vec<u32>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        *self.defaults.lock().unwrap() =
            defaults.into_iter().map(|v| PropertyValue::Uint32(v)).collect();
        Ok(())
    }
    pub fn set_defaults_f32(&mut self, defaults: Vec<f32>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        *self.defaults.lock().unwrap() =
            defaults.into_iter().map(|v| PropertyValue::Float32(v)).collect();
        Ok(())
    }
    pub fn set_defaults_str(&mut self, defaults: Vec<String>) -> Result<()> {
        self.check_defaults_len(defaults.len())?;
        *self.defaults.lock().unwrap() =
            defaults.into_iter().map(|v| PropertyValue::Str(v)).collect();
        Ok(())
    }
    pub fn set_defaults_null(&mut self) -> Result<()> {
        if !self.is_null_allowed {
            return Err(Error::PropertyNullNotAllowed)
        }
        if !self.is_bounded() {
            return Err(Error::PropertyWrongLen)
        }
        *self.defaults.lock().unwrap() = (0..self.array_len).map(|_| PropertyValue::Null).collect();
        Ok(())
    }
    /// Install expression defaults (builder variant). Requires `allow_exprs()`
    /// to have been called first, same as the post-creation variant.
    pub fn set_defaults_expr(&mut self, defaults: Vec<SExprCode>) -> Result<()> {
        if !self.is_expr_allowed {
            return Err(Error::PropertySExprNotAllowed)
        }
        self.check_defaults_len(defaults.len())?;
        *self.defaults.lock().unwrap() =
            defaults.into_iter().map(|v| PropertyValue::SExpr(Arc::new(v))).collect();
        Ok(())
    }

    // Post-creation default installation.
    //
    // Mutates `defaults[i]` on a live property with the same type/length
    // checks as the builder variants. Installing a default emits NO modify
    // event: it is a construction-time operation (before first frame) or
    // happens inside a switch batch where the accompanying unsets already
    // notify. Defaults are never mutated as a live styling mechanism.
    // Write masks do not apply (construction metadata).

    /// Raw variant; used by token-node construction. `SExpr` and `Null`
    /// are cross-type by design (mirroring `set_expr`/`set_null`) and are
    /// gated by `is_expr_allowed`/`is_null_allowed` instead.
    pub fn set_default_value(&self, i: usize, val: PropertyValue) -> Result<()> {
        match &val {
            PropertyValue::Unset => return Err(Error::PropertyWrongType),
            PropertyValue::SExpr(_) => {
                if !self.is_expr_allowed {
                    return Err(Error::PropertySExprNotAllowed)
                }
            }
            PropertyValue::Null => {
                if !self.is_null_allowed {
                    return Err(Error::PropertyNullNotAllowed)
                }
            }
            other => {
                if self.typ != other.as_type() {
                    return Err(Error::PropertyWrongType)
                }
            }
        }
        let defaults = &mut self.defaults.lock().unwrap();
        if i >= defaults.len() {
            return Err(Error::PropertyWrongIndex)
        }
        defaults[i] = val;
        Ok(())
    }

    pub fn set_default_bool(&self, i: usize, val: bool) -> Result<()> {
        self.set_default_value(i, PropertyValue::Bool(val))
    }
    pub fn set_default_u32(&self, i: usize, val: u32) -> Result<()> {
        self.set_default_value(i, PropertyValue::Uint32(val))
    }
    pub fn set_default_f32(&self, i: usize, val: f32) -> Result<()> {
        self.set_default_value(i, PropertyValue::Float32(val))
    }
    /// Set all indices of a bounded array at once.
    pub fn set_default_f32_multi(&self, vals: &[f32]) -> Result<()> {
        if self.is_bounded() && vals.len() != self.array_len {
            return Err(Error::PropertyWrongLen)
        }
        let mut defaults = self.defaults.lock().unwrap();
        if self.is_bounded() {
            for (i, val) in vals.iter().enumerate() {
                defaults[i] = PropertyValue::Float32(*val);
            }
        } else {
            defaults.clear();
            defaults.extend(vals.iter().map(|v| PropertyValue::Float32(*v)));
        }
        Ok(())
    }
    pub fn set_default_str<S: Into<String>>(&self, i: usize, val: S) -> Result<()> {
        self.set_default_value(i, PropertyValue::Str(val.into()))
    }
    /// Writes a proper `PropertyValue::Enum` (unlike the builder
    /// `set_defaults_str`, which writes `Str` onto Enum properties) and
    /// validates the item against `enum_items`.
    pub fn set_default_enum<S: Into<String>>(&self, i: usize, val: S) -> Result<()> {
        if self.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }
        let val = val.into();
        if let Some(items) = &self.enum_items {
            if !items.contains(&val) {
                return Err(Error::PropertyWrongEnumItem)
            }
        }
        self.set_default_value(i, PropertyValue::Enum(val))
    }
    pub fn set_default_node_id(&self, i: usize, val: SceneNodeId) -> Result<()> {
        self.set_default_value(i, PropertyValue::SceneNodeId(val))
    }
    pub fn set_default_shape(&self, i: usize, val: VectorShape) -> Result<()> {
        self.set_default_value(i, PropertyValue::VectorShape(Arc::new(val)))
    }
    pub fn set_default_null(&self, i: usize) -> Result<()> {
        self.set_default_value(i, PropertyValue::Null)
    }
    /// Install an expression default on a live property. Requires the
    /// factory to have opted in via `allow_exprs()`.
    pub fn set_default_expr(&self, i: usize, code: SExprCode) -> Result<()> {
        self.set_default_value(i, PropertyValue::SExpr(Arc::new(code)))
    }

    // Set

    /// This will clear all values, resetting them to the default
    pub fn clear_values(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
    ) -> Result<()> {
        self.check_write(role)?;
        {
            let vals = &mut self.vals.lock().unwrap();
            vals.clear();
            vals.resize(self.array_len, PropertyValue::Unset);
        }
        atom.add(self.clone(), role, ModifyAction::Clear);
        Ok(())
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
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
    ) -> Result<()> {
        self.check_write(role)?;
        {
            let vals = &mut self.vals.lock().unwrap();
            if i >= vals.len() {
                return Err(Error::PropertyWrongIndex)
            }
            vals[i] = PropertyValue::Unset;
        }
        atom.add(self.clone(), role, ModifyAction::Unset(i));
        Ok(())
    }

    pub fn set_null(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
    ) -> Result<()> {
        self.check_write(role)?;
        if !self.is_null_allowed {
            return Err(Error::PropertyNullNotAllowed)
        }

        let mut vals = self.vals.lock().unwrap();
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        vals[i] = PropertyValue::Null;
        drop(vals);

        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }

    pub fn set_bool(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: bool,
    ) -> Result<()> {
        self.check_write(role)?;
        self.set_raw_value(i, PropertyValue::Bool(val))?;
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_u32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: u32,
    ) -> Result<()> {
        self.check_write(role)?;
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
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_f32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: f32,
    ) -> Result<()> {
        self.check_write(role)?;
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
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_str<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: S,
    ) -> Result<()> {
        self.check_write(role)?;
        self.set_raw_value(i, PropertyValue::Str(val.into()))?;
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_enum<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: S,
    ) -> Result<()> {
        self.check_write(role)?;
        if self.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }
        let val = val.into();
        if !self.enum_items.as_ref().unwrap().contains(&val) {
            return Err(Error::PropertyWrongEnumItem)
        }
        self.set_raw_value(i, PropertyValue::Enum(val.into()))?;
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_node_id(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: SceneNodeId,
    ) -> Result<()> {
        self.check_write(role)?;
        self.set_raw_value(i, PropertyValue::SceneNodeId(val))?;
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }
    pub fn set_expr(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: SExprCode,
    ) -> Result<()> {
        self.check_write(role)?;
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
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }

    /// Typed dispatch over `PropertyValue`, routing through the typed
    /// setters so range/enum checks and write-mask enforcement apply.
    pub fn set_value(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: PropertyValue,
    ) -> Result<()> {
        match val {
            PropertyValue::Unset => self.unset(atom, role, i),
            PropertyValue::Null => self.set_null(atom, role, i),
            PropertyValue::Bool(v) => self.set_bool(atom, role, i, v),
            PropertyValue::Uint32(v) => self.set_u32(atom, role, i, v),
            PropertyValue::Float32(v) => self.set_f32(atom, role, i, v),
            PropertyValue::Str(v) => self.set_str(atom, role, i, v),
            PropertyValue::Enum(v) => self.set_enum(atom, role, i, v),
            PropertyValue::SceneNodeId(v) => self.set_node_id(atom, role, i, v),
            PropertyValue::SExpr(v) => self.set_expr(atom, role, i, (*v).clone()),
            // VectorShape is not Clone and set_shape has no extra
            // checks beyond set_raw_value, so write the Arc directly.
            PropertyValue::VectorShape(_) => {
                self.check_write(role)?;
                self.set_raw_value(i, val)?;
                atom.add(self.clone(), role, ModifyAction::Set(i));
                Ok(())
            }
        }
    }

    pub fn set_shape(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: VectorShape,
    ) -> Result<()> {
        self.check_write(role)?;
        self.set_raw_value(i, PropertyValue::VectorShape(Arc::new(val)))?;
        atom.add(self.clone(), role, ModifyAction::Set(i));
        Ok(())
    }

    fn set_value_vec<T, F>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<T>,
        f: F,
    ) -> Result<()>
    where
        F: Fn(T) -> PropertyValue,
    {
        self.check_write(role)?;
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }

        {
            let mut vals = self.vals.lock().unwrap();
            vals.clear();
            vals.extend(val.into_iter().map(f));
        }

        atom.add(self.clone(), role, ModifyAction::SetVec);
        Ok(())
    }

    pub fn set_bool_vec(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<bool>,
    ) -> Result<()> {
        self.set_value_vec(atom, role, val, PropertyValue::Bool)
    }

    pub fn set_u32_vec(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<u32>,
    ) -> Result<()> {
        self.set_value_vec(atom, role, val, PropertyValue::Uint32)
    }

    pub fn set_f32_vec(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<f32>,
    ) -> Result<()> {
        self.set_value_vec(atom, role, val, PropertyValue::Float32)
    }

    pub fn set_str_vec<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<S>,
    ) -> Result<()> {
        self.set_value_vec(atom, role, val, |v| PropertyValue::Str(v.into()))
    }

    pub fn set_enum_vec<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<S>,
    ) -> Result<()> {
        if self.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }
        self.set_value_vec(atom, role, val, |v| PropertyValue::Enum(v.into()))
    }

    pub fn set_node_id_vec(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: Vec<SceneNodeId>,
    ) -> Result<()> {
        self.set_value_vec(atom, role, val, PropertyValue::SceneNodeId)
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
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: f32,
    ) -> Result<()> {
        self.set_cache(i, PropertyValue::Float32(val))?;
        atom.add(self.clone(), role, ModifyAction::SetCache(vec![i]));
        Ok(())
    }
    pub fn set_cache_u32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        i: usize,
        val: u32,
    ) -> Result<()> {
        self.set_cache(i, PropertyValue::Uint32(val))?;
        atom.add(self.clone(), role, ModifyAction::SetCache(vec![i]));
        Ok(())
    }

    pub fn set_cache_f32_multi(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        changes: Vec<(usize, f32)>,
    ) -> Result<()> {
        let mut idxs = vec![];
        for (idx, val) in changes {
            self.set_cache(idx, PropertyValue::Float32(val))?;
            idxs.push(idx);
        }
        atom.add(self.clone(), role, ModifyAction::SetCache(idxs));
        Ok(())
    }
    pub fn set_cache_u32_range(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        changes: Vec<(usize, u32)>,
    ) -> Result<()> {
        let mut idxs = vec![];
        for (idx, val) in changes {
            self.set_cache(idx, PropertyValue::Uint32(val))?;
            idxs.push(idx);
        }
        atom.add(self.clone(), role, ModifyAction::SetCache(idxs));
        Ok(())
    }

    // Push

    fn push_value(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        value: PropertyValue,
    ) -> Result<usize> {
        self.check_write(role)?;
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }

        let mut vals = self.vals.lock().unwrap();
        let i = vals.len();
        vals.push(value);
        drop(vals);

        atom.add(self.clone(), role, ModifyAction::Push(i));
        Ok(i)
    }

    pub fn push_null(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Null)
    }
    pub fn push_bool(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: bool,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Bool(val))
    }
    pub fn push_u32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: u32,
    ) -> Result<usize> {
        // TODO: none of these push calls are enforcing constraints that are required
        // see the set_XX calls.
        self.push_value(atom, role, PropertyValue::Uint32(val))
    }
    pub fn push_f32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: f32,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Float32(val))
    }
    pub fn push_str<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: S,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Str(val.into()))
    }
    pub fn push_enum<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: S,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::Enum(val.into()))
    }
    pub fn push_node_id(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        val: SceneNodeId,
    ) -> Result<usize> {
        self.push_value(atom, role, PropertyValue::SceneNodeId(val))
    }

    // Insert

    fn insert_value(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        value: PropertyValue,
    ) -> Result<usize> {
        self.check_write(role)?;
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }
        if index > self.get_len() {
            return Err(Error::PropertyWrongIndex)
        }

        let mut vals = self.vals.lock().unwrap();
        vals.insert(index, value);
        drop(vals);

        atom.add(self.clone(), role, ModifyAction::Insert(index));
        Ok(index)
    }

    pub fn insert_null(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::Null)
    }

    pub fn insert_bool(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        val: bool,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::Bool(val))
    }

    pub fn insert_u32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        val: u32,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::Uint32(val))
    }

    pub fn insert_f32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        val: f32,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::Float32(val))
    }

    pub fn insert_str<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        val: S,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::Str(val.into()))
    }

    pub fn insert_enum<S: Into<String>>(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        val: S,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::Enum(val.into()))
    }

    pub fn insert_node_id(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
        val: SceneNodeId,
    ) -> Result<usize> {
        self.insert_value(atom, role, index, PropertyValue::SceneNodeId(val))
    }

    // Remove

    fn remove_value(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<PropertyValue> {
        self.check_write(role)?;
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }
        if index >= self.get_len() {
            return Err(Error::PropertyWrongIndex)
        }

        let mut vals = self.vals.lock().unwrap();
        let value = vals.remove(index);
        drop(vals);

        atom.add(self.clone(), role, ModifyAction::Remove(index, value.clone()));
        Ok(value)
    }

    pub fn remove_null(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<()> {
        self.remove_value(atom, role, index)?;
        Ok(())
    }

    pub fn remove_bool(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<bool> {
        match self.remove_value(atom, role, index)? {
            PropertyValue::Bool(v) => Ok(v),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn remove_u32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<u32> {
        match self.remove_value(atom, role, index)? {
            PropertyValue::Uint32(v) => Ok(v),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn remove_f32(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<f32> {
        match self.remove_value(atom, role, index)? {
            PropertyValue::Float32(v) => Ok(v),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn remove_str(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<String> {
        match self.remove_value(atom, role, index)? {
            PropertyValue::Str(v) => Ok(v),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn remove_enum(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<String> {
        match self.remove_value(atom, role, index)? {
            PropertyValue::Enum(v) => Ok(v),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn remove_node_id(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        index: usize,
    ) -> Result<SceneNodeId> {
        match self.remove_value(atom, role, index)? {
            PropertyValue::SceneNodeId(v) => Ok(v),
            _ => Err(Error::PropertyWrongType),
        }
    }

    // Remove by item

    pub fn remove_str_item(
        self: &Arc<Self>,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        item: &str,
    ) -> Option<usize> {
        for i in 0..self.get_len() {
            if self.get_str(i).unwrap() == item {
                self.remove_str(atom, role, i).unwrap();
                return Some(i)
            }
        }
        None
    }

    // Get

    pub fn is_bounded(&self) -> bool {
        self.array_len != 0
    }

    pub fn get_len(&self) -> usize {
        // Avoid locking unless we need to
        // If array len is nonzero, then vals len should be the same.
        if !self.is_bounded() {
            let vals_len = self.vals.lock().unwrap().len();
            if vals_len > 0 {
                return vals_len
            }
            return self.defaults.lock().unwrap().len()
        }
        self.array_len
    }

    pub fn is_unset(&self, i: usize) -> Result<bool> {
        let val = self.get_raw_value(i)?;
        Ok(val.is_unset())
    }
    pub fn is_null(&self, i: usize) -> Result<bool> {
        let val = self.get_value(i)?;
        Ok(val.is_null())
    }

    /// Effective-expression check: true when the active source for `i`
    /// is an expression — the value slot's expression if present, else
    /// (when the slot holds neither a plain value nor null) the
    /// default's expression.
    pub fn is_expr(&self, i: usize) -> Result<bool> {
        if !self.is_expr_allowed {
            return Ok(false)
        }
        let val = self.get_raw_value(i)?;
        if val.is_expr() {
            return Ok(true)
        }
        if !val.is_unset() {
            return Ok(false)
        }
        let defaults = self.defaults.lock().unwrap();
        if i >= defaults.len() {
            return Err(Error::PropertyWrongIndex)
        }
        Ok(defaults[i].is_expr())
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

    /// The effective expression source: the value slot's expression if
    /// present, else the default's expression (when the slot is unset).
    /// Errors when neither slot supplies one.
    pub fn get_expr(&self, i: usize) -> Result<Arc<SExprCode>> {
        let val = self.get_raw_value(i)?;
        if val.is_expr() {
            return val.as_sexpr()
        }
        if val.is_unset() {
            let defaults = self.defaults.lock().unwrap();
            if i >= defaults.len() {
                return Err(Error::PropertyWrongIndex)
            }
            if let PropertyValue::SExpr(code) = &defaults[i] {
                return Ok(code.clone())
            }
        }
        Err(Error::PropertyWrongType)
    }

    /// Effective value: NEVER an unresolved expression. Resolution
    /// order: set value → set expression's last computed result →
    /// default → default expression's last computed result → type
    /// default. One cache per index is shared between the two
    /// expression sources; draw-side evaluation recomputes every expr
    /// index each pass, so a stale cache after a source switch is
    /// unobservable past the next pass (which the switch triggers).
    pub fn get_value(&self, i: usize) -> Result<PropertyValue> {
        // Unbounded with empty vals: the defaults list IS the value
        // (theme unload clears vals → baseline palette shows through).
        if !self.is_bounded() {
            let vals = self.vals.lock().unwrap();
            if vals.is_empty() {
                drop(vals);
                let defaults = self.defaults.lock().unwrap();
                if i < defaults.len() {
                    return Ok(defaults[i].clone())
                }
                return Err(Error::PropertyWrongIndex)
            }
        }
        let val = self.get_raw_value(i)?;
        match val {
            PropertyValue::SExpr(_) => {
                let cached = self.get_cached(i)?;
                if !cached.is_null() {
                    return Ok(cached)
                }
                // fall through to the default layer
                Ok(self.default_or_type_default(i))
            }
            PropertyValue::Unset => Ok(self.default_or_type_default(i)),
            v => Ok(v),
        }
    }

    /// Default layer: plain default → default-expr cache → type default.
    fn default_or_type_default(&self, i: usize) -> PropertyValue {
        let defaults = self.defaults.lock().unwrap();
        match &defaults[i] {
            PropertyValue::SExpr(_) => match self.get_cached(i) {
                Ok(c) if !c.is_null() => c,
                _ => self.typ.default_value(),
            },
            d => d.clone(),
        }
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

    pub fn get_shape(&self, i: usize) -> Result<Arc<VectorShape>> {
        self.get_value(i)?.as_shape()
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

    fn get_value_vec<T, F>(&self, f: F) -> Result<Vec<T>>
    where
        F: Fn(&PropertyValue) -> Option<T>,
    {
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }

        let vals = self.vals.lock().unwrap();
        let mut result = Vec::with_capacity(vals.len());
        for val in vals.iter() {
            match f(val) {
                Some(v) => result.push(v),
                None => return Err(Error::PropertyWrongType),
            }
        }
        Ok(result)
    }

    pub fn get_bool_vec(&self) -> Result<Vec<bool>> {
        self.get_value_vec(|val| match val {
            PropertyValue::Bool(v) => Some(*v),
            _ => None,
        })
    }

    pub fn get_u32_vec(&self) -> Result<Vec<u32>> {
        self.get_value_vec(|val| match val {
            PropertyValue::Uint32(v) => Some(*v),
            _ => None,
        })
    }

    pub fn get_f32_vec(&self) -> Result<Vec<f32>> {
        self.get_value_vec(|val| match val {
            PropertyValue::Float32(v) => Some(*v),
            _ => None,
        })
    }

    pub fn get_str_vec(&self) -> Result<Vec<String>> {
        self.get_value_vec(|val| match val {
            PropertyValue::Str(v) => Some(v.clone()),
            _ => None,
        })
    }

    pub fn get_enum_vec(&self) -> Result<Vec<String>> {
        self.get_value_vec(|val| match val {
            PropertyValue::Enum(v) => Some(v.clone()),
            _ => None,
        })
    }

    pub fn get_node_id_vec(&self) -> Result<Vec<SceneNodeId>> {
        self.get_value_vec(|val| match val {
            PropertyValue::SceneNodeId(v) => Some(*v),
            _ => None,
        })
    }

    // Contains

    pub fn contains_str(&self, s: &str) -> bool {
        for i in 0..self.get_len() {
            if let Ok(item) = self.get_str(i) {
                if item == s {
                    return true
                }
            }
        }
        false
    }

    // Subs

    pub fn subscribe_modify(&self) -> Subscription<(Role, ModifyAction, BatchGuardPtr)> {
        self.on_modify.clone().subscribe()
    }

    // Dependencies

    pub fn add_depend<S: Into<String>>(
        &self,
        role: Role,
        prop: &PropertyPtr,
        i: usize,
        local_name: S,
    ) {
        self.depends.lock().unwrap().push(PropertyDepend {
            prop: Arc::downgrade(prop),
            i,
            local_name: local_name.into(),
        });
        // Wake live subscribers so they resync their poll sets (D5);
        // buffered events cover tasks that have not started polling yet.
        self.on_modify.notify((role, ModifyAction::DependsChanged, guard::BatchGuard::detached()));
    }

    /// Remove edges matching `(dep prop, index, local name)`. Theme
    /// unload restores original wiring with it: repeated switches must
    /// not accumulate stale edges pointing at dead token nodes.
    pub fn remove_depend(&self, role: Role, prop: &PropertyPtr, i: usize, local_name: &str) {
        let depends = &mut self.depends.lock().unwrap();
        depends.retain(|dep| {
            !(dep.i == i && dep.local_name == local_name && dep.prop.ptr_eq(&Arc::downgrade(prop)))
        });
        drop(depends);
        self.on_modify.notify((role, ModifyAction::DependsChanged, guard::BatchGuard::detached()));
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
    use crate::{expr::Op, ui::VectorShape as Shape};

    #[test]
    fn test_shape() {
        let mut shape = Shape::new();
        shape.add_filled_box(
            vec![Op::ConstFloat32(0.)],
            vec![Op::ConstFloat32(0.)],
            vec![Op::ConstFloat32(10.)],
            vec![Op::ConstFloat32(10.)],
            [0., 0., 0., 1.],
        );

        let prop = Arc::new(Property::new(
            "shape",
            PropertyType::VectorShape,
            PropertySubType::Null,
            PropertyPermission::default(),
        ));
        let atom = &mut PropertyAtomicGuard::none();
        // Default is an empty shape
        assert_eq!(prop.get_shape(0).unwrap().verts.len(), 0);
        prop.set_shape(atom, Role::App, 0, shape).unwrap();
        assert_eq!(prop.get_shape(0).unwrap().verts.len(), 4);
        assert_eq!(prop.get_shape(0).unwrap().indices.len(), 6);
        // Wrong index
        assert!(prop.set_shape(atom, Role::App, 4, Shape::new()).is_err());
    }

    #[test]
    fn test_getset() {
        let prop = Arc::new(Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        ));
        let atom = &mut PropertyAtomicGuard::none();
        assert!(prop.set_f32(atom, Role::App, 1, 4.).is_err());
        assert!(prop.is_unset(0).unwrap());
        assert!(prop.set_f32(atom, Role::App, 0, 4.).is_ok());
        assert_eq!(prop.get_f32(0).unwrap(), 4.);
        assert!(!prop.is_unset(0).unwrap());
        prop.unset(atom, Role::App, 0).unwrap();
        assert!(prop.is_unset(0).unwrap());
        assert_eq!(prop.get_f32(0).unwrap(), 0.);
    }

    #[test]
    fn test_nullable() {
        // default len is 1
        let mut prop_temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        assert!(prop_temp.set_defaults_f32(vec![1.0, 0.0]).is_err());
        assert!(prop_temp.set_defaults_f32(vec![2.0]).is_ok());
        prop_temp.allow_null_values();
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();
        prop.set_null(atom, Role::App, 0).unwrap();

        assert!(prop.get_f32_opt(1).is_err());
        assert!(prop.get_f32_opt(0).is_ok());
        assert!(prop.get_f32_opt(0).unwrap().is_none());

        prop.clear_values(atom, Role::App).unwrap();
        assert!(prop.get_f32(0).is_ok());
        assert!(prop.get_f32_opt(0).unwrap().is_some());
        assert_eq!(prop.get_f32(0).unwrap(), 2.0);
    }

    #[test]
    fn test_nonnullable() {
        let prop = Arc::new(Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        ));
        let atom = &mut PropertyAtomicGuard::none();
        assert!(prop.set_null(atom, Role::App, 0).is_err());
        assert!(prop.is_unset(0).unwrap());
    }

    #[test]
    fn test_unbounded() {
        let mut prop_temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.set_unbounded();
        prop_temp.allow_null_values();
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();
        assert_eq!(prop.get_len(), 0);
        prop.push_f32(atom, Role::App, 2.0).unwrap();
        prop.push_f32(atom, Role::App, 3.0).unwrap();
        assert_eq!(prop.get_len(), 2);

        prop.clear_values(atom, Role::App).unwrap();
        assert_eq!(prop.get_len(), 0);
        prop.push_null(atom, Role::App).unwrap();
        prop.push_f32(atom, Role::App, 4.0).unwrap();
        prop.push_f32(atom, Role::App, 5.0).unwrap();
        assert_eq!(prop.get_len(), 3);
        assert!(prop.get_f32_opt(0).unwrap().is_none());
        assert!(prop.get_f32_opt(1).unwrap().is_some());
        assert!(prop.get_f32_opt(2).unwrap().is_some());
        assert!(prop.get_f32_opt(3).is_err());

        let prop2 = Arc::new(Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        ));
        let atom2 = &mut PropertyAtomicGuard::none();
        assert!(prop2.push_f32(atom2, Role::App, 4.0).is_err());
    }

    #[test]
    fn test_range() {
        let mut prop_temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        let half_pi = 3.1415926535 / 2.;
        prop_temp.set_range_f32(-half_pi, half_pi);
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();
        assert!(prop.set_f32(atom, Role::App, 0, 6.).is_err());
        assert!(prop.set_f32(atom, Role::App, 0, 1.).is_ok());
    }

    #[test]
    fn test_enum() {
        let mut prop_temp = Property::new(
            "foo",
            PropertyType::Enum,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.set_enum_items(vec!["ABC", "XYZ", "FOO"]).unwrap();
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();
        assert!(prop.set_enum(atom, Role::App, 0, "ABC").is_ok());
        assert!(prop.set_enum(atom, Role::App, 0, "BAR").is_err());
    }

    #[test]
    fn test_expr() {
        let mut prop_temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.allow_exprs();
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();
        assert_eq!(prop.get_f32(0).unwrap(), 0.);
        let code = vec![Op::ConstFloat32(4.)];
        prop.set_expr(atom, Role::App, 0, code).unwrap();
        let val = prop.get_cached(0).unwrap();
        assert!(val.is_null());
        prop.set_cache_f32(atom, Role::App, 0, 4.).unwrap();
        let val = prop.get_cached(0).unwrap();
        assert_eq!(val.as_f32().unwrap(), 4.);
    }

    fn setup_test_property() -> Arc<Property> {
        let mut prop_temp = Property::new(
            "test",
            PropertyType::Str,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.set_unbounded();
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();
        prop.push_str(atom, Role::Internal, "Item 1").unwrap();
        prop.push_str(atom, Role::Internal, "Item 2").unwrap();
        prop.push_str(atom, Role::Internal, "Item 3").unwrap();
        prop
    }

    #[test]
    fn test_insert_at_beginning() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        prop.insert_str(atom, Role::App, 0, "NEW").unwrap();

        assert_eq!(prop.get_len(), 4);
        assert_eq!(prop.get_str(0).unwrap(), "NEW");
        assert_eq!(prop.get_str(1).unwrap(), "Item 1");
        assert_eq!(prop.get_str(2).unwrap(), "Item 2");
        assert_eq!(prop.get_str(3).unwrap(), "Item 3");
    }

    #[test]
    fn test_insert_at_end() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        prop.insert_str(atom, Role::App, 3, "NEW").unwrap();

        assert_eq!(prop.get_len(), 4);
        assert_eq!(prop.get_str(0).unwrap(), "Item 1");
        assert_eq!(prop.get_str(1).unwrap(), "Item 2");
        assert_eq!(prop.get_str(2).unwrap(), "Item 3");
        assert_eq!(prop.get_str(3).unwrap(), "NEW");
    }

    #[test]
    fn test_insert_in_middle() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        prop.insert_str(atom, Role::App, 1, "NEW").unwrap();

        assert_eq!(prop.get_len(), 4);
        assert_eq!(prop.get_str(0).unwrap(), "Item 1");
        assert_eq!(prop.get_str(1).unwrap(), "NEW");
        assert_eq!(prop.get_str(2).unwrap(), "Item 2");
        assert_eq!(prop.get_str(3).unwrap(), "Item 3");
    }

    #[test]
    fn test_insert_bounded_fails() {
        let mut prop_temp = Property::new(
            "test",
            PropertyType::Str,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.set_array_len(3);
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();

        let result = prop.insert_str(atom, Role::App, 0, "NEW");
        assert!(matches!(result, Err(Error::PropertyIsBounded)));
    }

    #[test]
    fn test_insert_out_of_bounds_fails() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        let result = prop.insert_str(atom, Role::App, 10, "NEW");
        assert!(matches!(result, Err(Error::PropertyWrongIndex)));
    }

    #[test]
    fn test_remove_from_beginning() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        let removed = prop.remove_str(atom, Role::App, 0).unwrap();

        assert_eq!(removed, "Item 1");
        assert_eq!(prop.get_len(), 2);
        assert_eq!(prop.get_str(0).unwrap(), "Item 2");
        assert_eq!(prop.get_str(1).unwrap(), "Item 3");
    }

    #[test]
    fn test_remove_from_end() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        let removed = prop.remove_str(atom, Role::App, 2).unwrap();

        assert_eq!(removed, "Item 3");
        assert_eq!(prop.get_len(), 2);
        assert_eq!(prop.get_str(0).unwrap(), "Item 1");
        assert_eq!(prop.get_str(1).unwrap(), "Item 2");
    }

    #[test]
    fn test_remove_from_middle() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        let removed = prop.remove_str(atom, Role::App, 1).unwrap();

        assert_eq!(removed, "Item 2");
        assert_eq!(prop.get_len(), 2);
        assert_eq!(prop.get_str(0).unwrap(), "Item 1");
        assert_eq!(prop.get_str(1).unwrap(), "Item 3");
    }

    #[test]
    fn test_remove_bounded_fails() {
        let mut prop_temp = Property::new(
            "test",
            PropertyType::Str,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.set_array_len(3);
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();

        let result = prop.remove_str(atom, Role::App, 0);
        assert!(matches!(result, Err(Error::PropertyIsBounded)));
    }

    #[test]
    fn test_remove_out_of_bounds_fails() {
        let prop = setup_test_property();
        let atom = &mut PropertyAtomicGuard::none();

        let result = prop.remove_str(atom, Role::App, 10);
        assert!(matches!(result, Err(Error::PropertyWrongIndex)));
    }

    #[test]
    fn test_insert_remove_mixed_types() {
        let mut prop_temp = Property::new(
            "test",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        prop_temp.set_unbounded();
        let prop = Arc::new(prop_temp);
        let atom = &mut PropertyAtomicGuard::none();

        prop.push_f32(atom, Role::App, 1.0).unwrap();
        prop.push_f32(atom, Role::App, 3.0).unwrap();

        prop.insert_f32(atom, Role::App, 1, 2.0).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 1.0);
        assert_eq!(prop.get_f32(1).unwrap(), 2.0);
        assert_eq!(prop.get_f32(2).unwrap(), 3.0);

        let removed = prop.remove_f32(atom, Role::App, 1).unwrap();
        assert_eq!(removed, 2.0);
        assert_eq!(prop.get_len(), 2);
        assert_eq!(prop.get_f32(0).unwrap(), 1.0);
        assert_eq!(prop.get_f32(1).unwrap(), 3.0);
    }

    // ------------------------------------------------------------------
    // Post-creation default installation (task 2.1)
    // ------------------------------------------------------------------

    #[test]
    fn test_default_install_on_live_property() {
        // A live (Arc'd, "linked") property whose value is unset reads the
        // installed default once one is installed.
        let prop = Arc::new(Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        ));
        let atom = &mut PropertyAtomicGuard::none();

        assert_eq!(prop.get_f32(0).unwrap(), 0.);
        prop.set_default_f32(0, 42.).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 42.);

        // Explicit value wins over the installed default
        prop.set_f32(atom, Role::App, 0, 7.).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 7.);

        // Clearing the value falls back to the default again
        prop.unset(atom, Role::App, 0).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 42.);
    }

    #[test]
    fn test_default_install_checks() {
        // Wrong type
        let prop = Arc::new(Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        ));
        assert!(prop.set_default_bool(0, true).is_err());

        // Wrong index
        assert!(prop.set_default_f32(1, 1.).is_err());

        // Unbounded properties reject per-index defaults (no defaults tier)
        let mut temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        temp.set_unbounded();
        let prop = Arc::new(temp);
        assert!(prop.set_default_f32(0, 1.).is_err());

        // Enum default validates items and writes the Enum variant
        let mut temp = Property::new(
            "foo",
            PropertyType::Enum,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        temp.set_enum_items(vec!["A", "B"]).unwrap();
        let prop = Arc::new(temp);
        assert!(prop.set_default_enum(0, "C").is_err());
        prop.set_default_enum(0, "B").unwrap();
        assert!(matches!(prop.get_value(0).unwrap(), PropertyValue::Enum(v) if v == "B"));

        // Multi-f32 default installs all indices at once
        let mut temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Color,
            PropertyPermission::default(),
        );
        temp.set_array_len(4);
        let prop = Arc::new(temp);
        prop.set_default_f32_multi(&[0., 1., 2., 3.]).unwrap();
        for i in 0..4 {
            assert_eq!(prop.get_f32(i).unwrap(), i as f32);
        }
    }

    // ------------------------------------------------------------------
    // Expression defaults and effective source (task 2.2)
    // ------------------------------------------------------------------

    #[test]
    fn test_default_expr_and_unset_override() {
        let mut temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        temp.allow_exprs();
        let prop = Arc::new(temp);
        let atom = &mut PropertyAtomicGuard::none();

        // Default expression governs while the value slot is unset
        prop.set_default_expr(0, vec![Op::ConstFloat32(10.)]).unwrap();
        assert!(prop.is_expr(0).unwrap());
        assert!(!prop.get_raw_value(0).unwrap().is_expr()); // source is the default

        // Before first evaluation a concrete read succeeds with the type default
        assert_eq!(prop.get_f32(0).unwrap(), 0.);

        // Simulate an evaluation writing the cache
        prop.set_cache_f32(atom, Role::Internal, 0, 10.).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 10.);

        // Theme-style override: plain value wins, expr no longer effective
        prop.set_f32(atom, Role::App, 0, 5.).unwrap();
        assert!(!prop.is_expr(0).unwrap());
        assert_eq!(prop.get_f32(0).unwrap(), 5.);

        // Override expression (value slot) wins over the default expression
        prop.set_expr(atom, Role::App, 0, vec![Op::ConstFloat32(20.)]).unwrap();
        assert!(prop.is_expr(0).unwrap());
        // One cache per index is shared between the two expression sources
        // (design D3): until the next evaluation, reads observe the stale
        // cache from the default expression — the window closes at the next
        // draw pass, which the source switch triggers.
        assert_eq!(prop.get_f32(0).unwrap(), 10.);
        prop.set_cache_f32(atom, Role::Internal, 0, 20.).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 20.);

        // Unsetting the override returns control to the default expression
        prop.unset(atom, Role::App, 0).unwrap();
        assert!(prop.is_expr(0).unwrap());
        // Still the override's cached result until the next pass...
        assert_eq!(prop.get_f32(0).unwrap(), 20.);
        // ...which recomputes the default expression from scratch
        prop.set_cache_f32(atom, Role::Internal, 0, 10.).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 10.);
    }

    #[test]
    fn test_get_value_never_returns_expr() {
        let mut temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        temp.allow_exprs();
        let prop = Arc::new(temp);
        let atom = &mut PropertyAtomicGuard::none();

        // Value-slot expr, never evaluated
        prop.set_expr(atom, Role::App, 0, vec![Op::ConstFloat32(1.)]).unwrap();
        assert!(matches!(prop.get_value(0).unwrap(), PropertyValue::Float32(_)));

        // Default expr, never evaluated
        prop.unset(atom, Role::App, 0).unwrap();
        prop.set_default_expr(0, vec![Op::ConstFloat32(2.)]).unwrap();
        assert!(matches!(prop.get_value(0).unwrap(), PropertyValue::Float32(_)));
        assert_eq!(prop.get_f32(0).unwrap(), 0.);

        // Builder variant round-trips through the same checks
        let mut temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        temp.allow_exprs();
        assert!(temp.set_defaults_expr(vec![vec![Op::ConstFloat32(3.)]]).is_ok());
        // Without allow_exprs it fails
        let mut temp2 = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission::default(),
        );
        assert!(matches!(
            temp2.set_defaults_expr(vec![vec![Op::ConstFloat32(3.)]]),
            Err(Error::PropertySExprNotAllowed)
        ));
    }

    // ------------------------------------------------------------------
    // Role permissions (task 2.3)
    // ------------------------------------------------------------------

    #[test]
    fn test_permission_denied_write_leaves_value() {
        let mut temp = Property::new(
            "foo",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission { read: Role::ALL, write: Role::Internal },
        );
        temp.set_defaults_f32(vec![1.]).unwrap();
        let prop = Arc::new(temp);
        let atom = &mut PropertyAtomicGuard::none();

        // Allowed writer
        prop.set_f32(atom, Role::Internal, 0, 5.).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 5.);

        // Denied writer: error, value unchanged
        assert!(matches!(
            prop.set_f32(atom, Role::Theme, 0, 9.),
            Err(Error::PropertyPermissionDenied)
        ));
        assert_eq!(prop.get_f32(0).unwrap(), 5.);

        // Denied unset
        assert!(matches!(prop.unset(atom, Role::User, 0), Err(Error::PropertyPermissionDenied)));
        assert_eq!(prop.get_f32(0).unwrap(), 5.);

        // Denied push (unbounded)
        let mut temp = Property::new(
            "foo",
            PropertyType::Str,
            PropertySubType::Null,
            PropertyPermission { read: Role::ALL, write: Role::User },
        );
        temp.set_unbounded();
        let prop = Arc::new(temp);
        assert!(matches!(
            prop.push_str(atom, Role::App, "x"),
            Err(Error::PropertyPermissionDenied)
        ));
        assert_eq!(prop.get_len(), 0);
    }

    #[test]
    fn test_permission_denied_wrapped_read() {
        use crate::scene::{SceneNode, SceneNodeType};

        let mut node = SceneNode::new("test", SceneNodeType::Null);
        let mut temp = Property::new(
            "secret",
            PropertyType::Float32,
            PropertySubType::Null,
            // Readable by App only
            PropertyPermission { read: Role::App, write: Role::ALL },
        );
        temp.set_defaults_f32(vec![3.]).unwrap();
        node.add_property(temp).unwrap();
        let node = Arc::new(node);

        // Wrapping with a role lacking the read bit fails with
        // PropertyPermissionDenied (validated upfront at wrap time since
        // permissions are immutable).
        assert!(matches!(
            PropertyFloat32::wrap(&node, Role::Theme, "secret", 0),
            Err(Error::PropertyPermissionDenied)
        ));

        // The allowed role wraps fine and reads the value
        let wrapped = PropertyFloat32::wrap(&node, Role::App, "secret", 0).unwrap();
        assert_eq!(wrapped.get(), 3.);
    }

    #[test]
    fn test_permission_theme_denied_on_widget_owned() {
        // D4/D13: widget-written runtime properties carry write masks
        // without Theme, so a theme attempt fails instead of losing a
        // write race.
        let mut temp = Property::new(
            "alpha",
            PropertyType::Float32,
            PropertySubType::Null,
            PropertyPermission { read: Role::ALL, write: Role::Internal | Role::App },
        );
        temp.set_defaults_f32(vec![0.]).unwrap();
        let prop = Arc::new(temp);
        let atom = &mut PropertyAtomicGuard::none();

        assert!(prop.can_write(Role::Internal));
        assert!(prop.can_write(Role::App));
        assert!(!prop.can_write(Role::Theme));
        assert!(matches!(
            prop.set_f32(atom, Role::Theme, 0, 1.),
            Err(Error::PropertyPermissionDenied)
        ));
        // The widget's computed value stands
        prop.set_f32(atom, Role::Internal, 0, 0.5).unwrap();
        assert_eq!(prop.get_f32(0).unwrap(), 0.5);
    }

    #[test]
    fn test_role_bitflags() {
        let mask = Role::App | Role::Theme;
        assert!(mask.contains(Role::App));
        assert!(mask.contains(Role::Theme));
        assert!(!mask.contains(Role::User));
        assert!(!mask.contains(Role::Internal));
        assert!(Role::ALL.contains(Role::User));
        assert!(Role::ALL.contains(Role::Theme));
        assert!(!Role::NONE.contains(Role::User));
        assert!(mask.intersects(Role::User | Role::App));
        assert!(!mask.intersects(Role::User | Role::Internal));
        // Equality comparisons used by when_change filters still work
        assert_eq!(Role::Internal, Role::Internal);
        assert_ne!(Role::Internal, Role::Ignored);
    }

    // ------------------------------------------------------------------
    // f32-array expression evaluation (task 3.1)
    // ------------------------------------------------------------------

    #[test]
    fn test_eval_f32_multi_color_follows_token() {
        use crate::{
            expr,
            scene::{SceneNode, SceneNodeType},
        };

        // Token property: a 4-component color with plain values
        let mut token_temp = Property::new(
            "accent",
            PropertyType::Float32,
            PropertySubType::Color,
            PropertyPermission::default(),
        );
        token_temp.set_array_len(4);
        let token = Arc::new(token_temp);
        let atom = &mut PropertyAtomicGuard::none();
        for i in 0..4 {
            token.set_f32(atom, Role::App, i, i as f32 * 0.25).unwrap();
        }

        // Widget color property: per-index exprs referencing the token
        let mut node = SceneNode::new("widget", SceneNodeType::Null);
        let mut color_temp = Property::new(
            "text_color",
            PropertyType::Float32,
            PropertySubType::Color,
            PropertyPermission::default(),
        );
        color_temp.set_array_len(4);
        color_temp.allow_exprs();
        node.add_property(color_temp).unwrap();
        let node = Arc::new(node);
        let color = PropertyColor::wrap(&node, Role::Internal, "text_color").unwrap();

        let prop = color.prop();
        for i in 0..4 {
            let local = format!("accent_{i}");
            prop.set_default_expr(i, expr::load_var(&local)).unwrap();
            prop.add_depend(Role::App, &token, i, local);
        }

        // Before evaluation, reads fall through to the type default
        assert_eq!(prop.get_f32(0).unwrap(), 0.);

        // Evaluate: results land in the cache and are returned by get_f32
        color.eval(atom).unwrap();
        for i in 0..4 {
            assert_eq!(prop.get_f32(i).unwrap(), i as f32 * 0.25);
        }
        assert_eq!(color.get(), [0., 0.25, 0.5, 0.75]);

        // Changing the token and re-evaluating recomputes every index
        token.set_f32(atom, Role::App, 0, 1.).unwrap();
        color.eval(atom).unwrap();
        assert_eq!(color.get(), [1., 0.25, 0.5, 0.75]);
    }

    #[test]
    fn test_eval_f32_multi_mixed_indices() {
        use crate::{
            expr,
            scene::{SceneNode, SceneNodeType},
        };

        let token = {
            let mut t = Property::new(
                "size",
                PropertyType::Float32,
                PropertySubType::Pixel,
                PropertyPermission::default(),
            );
            t.set_defaults_f32(vec![18.]).unwrap();
            Arc::new(t)
        };

        // One expr index (0 → token) and one plain value (1 → 99.)
        let mut node = SceneNode::new("widget", SceneNodeType::Null);
        let mut temp = Property::new(
            "geom",
            PropertyType::Float32,
            PropertySubType::Pixel,
            PropertyPermission::default(),
        );
        temp.set_array_len(2);
        temp.allow_exprs();
        node.add_property(temp).unwrap();
        let node = Arc::new(node);
        let prop = node.get_property("geom").unwrap();

        prop.set_default_expr(0, expr::load_var("size")).unwrap();
        prop.add_depend(Role::App, &token, 0, "size");
        let atom = &mut PropertyAtomicGuard::none();
        prop.set_f32(atom, Role::App, 1, 99.).unwrap();

        eval_f32_multi(&prop, atom, Role::Internal, &[0, 1], vec![]).unwrap();

        // Only the expr index was recomputed; the plain index kept its value
        assert_eq!(prop.get_f32(0).unwrap(), 18.);
        assert_eq!(prop.get_f32(1).unwrap(), 99.);
        assert!(prop.get_raw_value(1).unwrap().is_expr() == false);

        // Single-f32 wrap variant evaluates its index from dependencies
        let font_size = PropertyFloat32::wrap(&node, Role::Internal, "geom", 0).unwrap();
        font_size.eval(atom).unwrap();
        assert_eq!(font_size.get(), 18.);
    }
}
