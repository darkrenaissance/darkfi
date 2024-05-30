use std::sync::Arc;

use crate::{scene::SceneNode, error::{Error, Result}};
use super::Property;

pub struct PropertyBool {
    prop: Arc<Property>,
    idx: usize,
}

impl PropertyBool {
    pub fn wrap(node: &SceneNode, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_bool(idx)?;

        Ok(Self { prop, idx })
    }

    pub fn get(&self) -> bool {
        self.prop.get_bool(self.idx).unwrap()
    }

    pub fn set(&self, val: bool) {
        self.prop.set_bool(self.idx, val).unwrap()
    }
}

pub struct PropertyUint32 {
    prop: Arc<Property>,
    idx: usize,
}

impl PropertyUint32 {
    pub fn wrap(node: &SceneNode, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_u32(idx)?;

        Ok(Self { prop, idx })
    }

    pub fn get(&self) -> u32 {
        self.prop.get_u32(self.idx).unwrap()
    }

    pub fn set(&self, val: u32) {
        self.prop.set_u32(self.idx, val).unwrap()
    }
}

pub struct PropertyFloat32 {
    prop: Arc<Property>,
    idx: usize,
}

impl PropertyFloat32 {
    pub fn wrap(node: &SceneNode, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_f32(idx)?;

        Ok(Self { prop, idx })
    }

    pub fn get(&self) -> f32 {
        self.prop.get_f32(self.idx).unwrap()
    }

    pub fn set(&self, val: f32) {
        self.prop.set_f32(self.idx, val).unwrap()
    }
}

pub struct PropertyStr {
    prop: Arc<Property>,
    idx: usize,
}

impl PropertyStr {
    pub fn wrap(node: &SceneNode, prop_name: &str, idx: usize) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        // Test if it works
        let _ = prop.get_str(idx)?;

        Ok(Self { prop, idx })
    }

    pub fn get(&self) -> String {
        self.prop.get_str(self.idx).unwrap()
    }

    pub fn set<S: Into<String>>(&self, val: S) {
        self.prop.set_str(self.idx, val.into()).unwrap()
    }
}

pub struct PropertyColor {
    prop: Arc<Property>,
}

impl PropertyColor {
    pub fn wrap(node: &SceneNode, prop_name: &str) -> Result<Self> {
        let prop = node.get_property(prop_name).ok_or(Error::PropertyNotFound)?;

        if !prop.is_bounded() || prop.get_len() != 4 {
            return Err(Error::PropertyWrongLen)
        }

        // Test if it works
        let _ = prop.get_f32(0)?;

        Ok(Self { prop })
    }

    pub fn get(&self) -> [f32; 4] {
        [self.prop.get_f32(0).unwrap(),
        self.prop.get_f32(1).unwrap(),
        self.prop.get_f32(2).unwrap(),
        self.prop.get_f32(3).unwrap()]
    }

    pub fn set(&self, val: [f32; 4]) {
        self.prop.set_f32(0, val[0]).unwrap();
        self.prop.set_f32(1, val[1]).unwrap();
        self.prop.set_f32(2, val[2]).unwrap();
        self.prop.set_f32(3, val[3]).unwrap();
    }
}

