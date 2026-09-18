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
    error::{Error, Result},
    expr::{SExprMachine, SExprVal},
    gfx::{Dimension, Rectangle},
    scene::SceneNode as SceneNode3,
    ui::VectorShape,
};
use std::sync::Arc;

use super::{PropertyAtomicGuard, PropertyPtr, PropertyType, Role};

/// Shared bounded f32-array expression evaluator (the generalization of
/// the old `PropertyRect::eval_with` pattern). Gathers globals from the
/// property's dependency edges plus `extras`, evaluates every expr-bound
/// index in `range` against them, and writes results into the cache
/// (cache writes bypass `set_f32` range validation by design — they hold
/// derived eval artifacts, not authored values). Indices holding plain
/// values are left untouched.
///
/// Dead dependency edges (target property dropped, e.g. mid-teardown of
/// a theme token node) are skipped rather than erroring: the unload
/// contract removes edges before their targets die, and skipping keeps a
/// racing draw pass from failing outright. An expression still
/// referencing a skipped global fails at evaluation time instead.
///
/// Read-mask enforcement: dependency reads happen on behalf of `role`,
/// so each dependency's read mask is checked here.
pub fn eval_f32_multi(
    prop: &PropertyPtr,
    atom: &mut PropertyAtomicGuard,
    role: Role,
    range: &[usize],
    extras: Vec<(String, f32)>,
) -> Result<()> {
    let mut globals = vec![];

    for dep in prop.get_depends() {
        let Some(dep_prop) = dep.prop.upgrade() else { continue };

        if !dep_prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        let value = dep_prop.get_f32(dep.i)?;
        globals.push((dep.local_name, SExprVal::Float32(value)));
    }

    for (name, val) in extras {
        globals.push((name, SExprVal::Float32(val)));
    }

    let mut changes = vec![];
    for &i in range {
        if !prop.is_expr(i)? {
            continue
        }

        let expr = prop.get_expr(i)?;

        let mut machine = SExprMachine { globals: globals.clone(), stmts: &expr };

        let v = machine.call()?.as_f32()?;
        changes.push((i, v));
    }

    prop.set_cache_f32_multi(atom, role, changes).unwrap();
    Ok(())
}

#[derive(Clone)]
pub struct PropertyShape {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyShape {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        // Test if it works
        let _ = prop.get_shape(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> Arc<VectorShape> {
        self.prop.get_shape(self.idx).unwrap()
    }

    #[allow(dead_code)]
    pub fn set(&self, atom: &mut PropertyAtomicGuard, val: VectorShape) {
        self.prop().set_shape(atom, self.role, self.idx, val).unwrap()
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyBool {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyBool {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        // Test if it works
        let _ = prop.get_bool(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> bool {
        self.prop.get_bool(self.idx).unwrap()
    }

    pub fn set(&self, atom: &mut PropertyAtomicGuard, val: bool) {
        self.prop().set_bool(atom, self.role, self.idx, val).unwrap()
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyUint32 {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyUint32 {
    /*
    pub fn from(prop: PropertyPtr, role: Role, idx: usize) -> Result<Self> {
        // Test if it works
        let _ = prop.get_u32(idx)?;

        Ok(Self { prop, role, idx })
    }
    */

    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        // Test if it works
        let _ = prop.get_u32(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> u32 {
        self.prop.get_u32(self.idx).unwrap()
    }

    #[allow(dead_code)]
    pub fn set(&self, atom: &mut PropertyAtomicGuard, val: u32) {
        self.prop().set_u32(atom, self.role, self.idx, val).unwrap()
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyFloat32 {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyFloat32 {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        // Test if it works
        let _ = prop.get_f32(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> f32 {
        self.prop.get_f32(self.idx).unwrap()
    }

    /// Re-evaluate this property's expression (if any) against its
    /// dependency globals. Called from the owning widget's draw path so
    /// every pass computes from current dependency values.
    pub fn eval(&self, atom: &mut PropertyAtomicGuard) -> Result<()> {
        eval_f32_multi(&self.prop(), atom, self.role, &[self.idx], vec![])
    }

    pub fn set(&self, atom: &mut PropertyAtomicGuard, val: f32) {
        self.prop().set_f32(atom, self.role, self.idx, val).unwrap()
    }

    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyStr {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyStr {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        // Test if it works
        let _ = prop.get_str(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> String {
        self.prop.get_str(self.idx).unwrap()
    }

    pub fn set<S: Into<String>>(&self, atom: &mut PropertyAtomicGuard, val: S) {
        self.prop().set_str(atom, self.role, self.idx, val.into()).unwrap()
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyColor {
    prop: PropertyPtr,
    role: Role,
}

impl PropertyColor {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        if !prop.is_bounded() || prop.get_len() != 4 {
            return Err(Error::PropertyWrongLen)
        }

        // Test if it works
        let _ = prop.get_f32(0)?;

        Ok(Self { prop, role })
    }

    pub fn get(&self) -> [f32; 4] {
        [
            self.prop.get_f32(0).unwrap(),
            self.prop.get_f32(1).unwrap(),
            self.prop.get_f32(2).unwrap(),
            self.prop.get_f32(3).unwrap(),
        ]
    }

    /// Re-evaluate all four color components against the property's
    /// dependency globals (no extras — colors have no parent-rect
    /// globals). Called at the top of the owning widget's `draw()`,
    /// alongside rect evaluation, so every draw pass reflects current
    /// token values. Cache writes are stamped `Role::Internal`, so
    /// `when_change_external` handlers skip them as eval echoes.
    pub fn eval(&self, atom: &mut PropertyAtomicGuard) -> Result<()> {
        eval_f32_multi(&self.prop(), atom, self.role, &[0, 1, 2, 3], vec![])
    }

    #[allow(dead_code)]
    pub fn set(&self, atom: &mut PropertyAtomicGuard, val: [f32; 4]) {
        self.prop().set_f32(atom, self.role, 0, val[0]).unwrap();
        self.prop().set_f32(atom, self.role, 1, val[1]).unwrap();
        self.prop().set_f32(atom, self.role, 2, val[2]).unwrap();
        self.prop().set_f32(atom, self.role, 3, val[3]).unwrap();
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyDimension {
    prop: PropertyPtr,
    role: Role,
}

impl PropertyDimension {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        if !prop.is_bounded() || prop.get_len() != 2 {
            return Err(Error::PropertyWrongLen)
        }

        // Test if it works
        let _ = prop.get_f32(0)?;

        Ok(Self { prop, role })
    }

    pub fn get(&self) -> Dimension {
        [self.prop.get_f32(0).unwrap(), self.prop.get_f32(1).unwrap()].into()
    }

    pub fn set(&self, atom: &mut PropertyAtomicGuard, dim: Dimension) {
        self.prop().set_f32(atom, self.role, 0, dim.w).unwrap();
        self.prop().set_f32(atom, self.role, 1, dim.h).unwrap();
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

#[derive(Clone)]
pub struct PropertyEnum {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyEnum {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        // Verify property type is Enum
        if prop.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> String {
        // Handle both PropertyValue::Str (defaults) and PropertyValue::Enum (set values)
        match self.prop.get_value(self.idx) {
            Ok(val) => match val.as_str() {
                Ok(s) => s,
                Err(_) => val.as_enum().unwrap(),
            },
            Err(_) => self.prop.get_enum(self.idx).unwrap(),
        }
    }

    pub fn set<S: Into<String>>(&self, atom: &mut PropertyAtomicGuard, val: S) {
        self.prop().set_enum(atom, self.role, self.idx, val.into()).unwrap()
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}
/*
#[derive(Clone)]
pub struct PropertyPoint {
    prop: PropertyPtr,
    role: Role,
}

impl PropertyPoint {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        if !prop.is_bounded() || prop.get_len() != 2 {
            return Err(Error::PropertyWrongLen)
        }

        // Test if it works
        let _ = prop.get_f32(0)?;

        Ok(Self { prop, role })
    }

    pub fn get(&self) -> Point {
        [self.prop.get_f32(0).unwrap(), self.prop.get_f32(1).unwrap()].into()
    }

    pub fn set(&self, atom: &mut PropertyAtomicGuard, pos: Point) {
        self.prop().set_f32(atom, self.role, 0, pos.x).unwrap();
        self.prop().set_f32(atom, self.role, 1, pos.y).unwrap();
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}
*/

#[derive(Clone)]
pub struct PropertyRect {
    prop: PropertyPtr,
    role: Role,
}

impl PropertyRect {
    pub fn wrap(node: &SceneNode3, role: Role, prop_name: &str) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Read-mask enforcement for the wrap layer: the handle acts with
        // `role` for its whole lifetime and permissions are immutable,
        // so validating here denies unreadable handles at construction
        // instead of at first read.
        if !prop.can_read(role) {
            return Err(Error::PropertyPermissionDenied)
        }

        if !prop.is_bounded() || prop.get_len() != 4 {
            return Err(Error::PropertyWrongLen)
        }

        // Test if it works
        let _ = prop.get_f32(0)?;

        Ok(Self { prop, role })
    }

    pub fn eval(&self, atom: &mut PropertyAtomicGuard, parent_rect: &Rectangle) -> Result<()> {
        self.eval_with(
            atom,
            (0..4).collect(),
            vec![("w".to_string(), parent_rect.w), ("h".to_string(), parent_rect.h)],
        )
    }

    /// Rect variant of [`eval_f32_multi`]: all four indices, with the
    /// parent's width/height available as the `w`/`h` globals.
    pub fn eval_with(
        &self,
        atom: &mut PropertyAtomicGuard,
        range: Vec<usize>,
        extras: Vec<(String, f32)>,
    ) -> Result<()> {
        eval_f32_multi(&self.prop(), atom, self.role, &range, extras)
    }

    pub fn get(&self) -> Rectangle {
        Rectangle::from([
            self.prop.get_f32(0).unwrap(),
            self.prop.get_f32(1).unwrap(),
            self.prop.get_f32(2).unwrap(),
            self.prop.get_f32(3).unwrap(),
        ])
    }

    pub fn get_width(&self) -> f32 {
        self.prop.get_f32(2).unwrap()
    }
    pub fn get_height(&self) -> f32 {
        self.prop.get_f32(3).unwrap()
    }

    /*
    pub fn get_opt(&self) -> Option<Rectangle> {
        Some(Rectangle::from([
            self.prop.get_f32(0).ok()?,
            self.prop.get_f32(1).ok()?,
            self.prop.get_f32(2).ok()?,
            self.prop.get_f32(3).ok()?,
        ]))
    }
    */

    #[cfg(target_os = "android")]
    pub fn set(&self, atom: &mut PropertyAtomicGuard, rect: &Rectangle) {
        self.prop().set_f32(atom, self.role, 0, rect.x).unwrap();
        self.prop().set_f32(atom, self.role, 1, rect.y).unwrap();
        self.prop().set_f32(atom, self.role, 2, rect.w).unwrap();
        self.prop().set_f32(atom, self.role, 3, rect.h).unwrap();
    }

    #[inline]
    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }

    fn is_f32_or_has_cached(&self, i: usize) -> bool {
        if self.prop.is_expr(i).unwrap() {
            if self.prop.get_cached(i).unwrap().is_null() {
                return false
            }
        }
        true
    }
    pub fn has_cached(&self) -> bool {
        self.is_f32_or_has_cached(0) &&
            self.is_f32_or_has_cached(1) &&
            self.is_f32_or_has_cached(2) &&
            self.is_f32_or_has_cached(3)
    }
}
