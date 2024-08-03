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

use std::sync::Arc;

use super::{Property, PropertyPtr, Role};
use crate::{
    error::{Error, Result},
    scene::SceneNode,
};

pub struct PropertyBool {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyBool {
    pub fn wrap(node: &SceneNode, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_bool(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> bool {
        self.prop.get_bool(self.idx).unwrap()
    }

    pub fn set(&self, val: bool) {
        self.prop.set_bool(self.role, self.idx, val).unwrap()
    }

    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

pub struct PropertyUint32 {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyUint32 {
    pub fn from(prop: PropertyPtr, role: Role, idx: usize) -> Result<Self> {
        // Test if it works
        let _ = prop.get_u32(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn wrap(node: &SceneNode, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_u32(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> u32 {
        self.prop.get_u32(self.idx).unwrap()
    }

    pub fn set(&self, val: u32) {
        self.prop.set_u32(self.role, self.idx, val).unwrap()
    }

    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

pub struct PropertyFloat32 {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyFloat32 {
    pub fn wrap(node: &SceneNode, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_f32(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> f32 {
        self.prop.get_f32(self.idx).unwrap()
    }

    pub fn set(&self, val: f32) {
        self.prop.set_f32(self.role, self.idx, val).unwrap()
    }

    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

pub struct PropertyStr {
    prop: PropertyPtr,
    role: Role,
    idx: usize,
}

impl PropertyStr {
    pub fn wrap(node: &SceneNode, role: Role, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_str(idx)?;

        Ok(Self { prop, role, idx })
    }

    pub fn get(&self) -> String {
        self.prop.get_str(self.idx).unwrap()
    }

    pub fn set<S: Into<String>>(&self, val: S) {
        self.prop.set_str(self.role, self.idx, val.into()).unwrap()
    }

    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}

pub struct PropertyColor {
    prop: PropertyPtr,
    role: Role,
}

impl PropertyColor {
    pub fn wrap(node: &SceneNode, role: Role, prop_name: &str) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

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

    pub fn set(&self, val: [f32; 4]) {
        self.prop.set_f32(self.role, 0, val[0]).unwrap();
        self.prop.set_f32(self.role, 1, val[1]).unwrap();
        self.prop.set_f32(self.role, 2, val[2]).unwrap();
        self.prop.set_f32(self.role, 3, val[3]).unwrap();
    }

    pub fn prop(&self) -> PropertyPtr {
        self.prop.clone()
    }
}
