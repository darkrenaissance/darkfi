use crate::error::{Error, Result};
use atomic_float::AtomicF32;
use darkfi_serial::{Encodable, SerialDecodable, SerialEncodable, WriteExt};
use std::{
    fmt,
    io::Write,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

use crate::{expr::SExprCode, scene::SceneNodeId};

type Buffer = Arc<Vec<u8>>;

#[derive(Debug, Copy, Clone, PartialEq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum PropertyType {
    Null = 0,
    Bool = 1,
    Uint32 = 2,
    Float32 = 3,
    Str = 4,
    Enum = 5,
    Buffer = 6,
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
            Self::Buffer => PropertyValue::Buffer(Arc::new(vec![])),
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
}

#[derive(Debug, Clone)]
pub enum PropertyValue {
    Unset,
    Null,
    Bool(bool),
    Uint32(u32),
    Float32(f32),
    Str(String),
    Enum(String),
    Buffer(Arc<Vec<u8>>),
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
            Self::Buffer(_) => PropertyType::Buffer,
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

    fn as_bool(&self) -> Result<bool> {
        match self {
            Self::Bool(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_u32(&self) -> Result<u32> {
        match self {
            Self::Uint32(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_f32(&self) -> Result<f32> {
        match self {
            Self::Float32(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_str(&self) -> Result<String> {
        match self {
            Self::Str(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_enum(&self) -> Result<String> {
        match self {
            Self::Enum(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_buf(&self) -> Result<Buffer> {
        match self {
            Self::Buffer(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_node_id(&self) -> Result<SceneNodeId> {
        match self {
            Self::SceneNodeId(v) => Ok(*v),
            _ => Err(Error::PropertyWrongType),
        }
    }
    fn as_sexpr(&self) -> Result<Arc<SExprCode>> {
        match self {
            Self::SExpr(v) => Ok(v.clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }
}

impl Encodable for PropertyValue {
    fn encode<S: Write>(&self, s: &mut S) -> std::result::Result<usize, std::io::Error> {
        match self {
            Self::Unset | Self::Null | Self::Buffer(_) => {
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

pub struct Property {
    pub name: String,
    pub typ: PropertyType,
    pub subtype: PropertySubType,
    pub defaults: Vec<PropertyValue>,
    pub vals: Mutex<Vec<PropertyValue>>,

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
}

impl Property {
    pub fn new<S: Into<String>>(name: S, typ: PropertyType, subtype: PropertySubType) -> Self {
        Self {
            name: name.into(),
            typ,
            subtype,

            defaults: vec![typ.default_value()],
            vals: Mutex::new(vec![PropertyValue::Unset]),

            ui_name: String::new(),
            desc: String::new(),

            is_null_allowed: false,
            is_expr_allowed: false,

            array_len: 1,
            min_val: None,
            max_val: None,
            enum_items: None,
        }
    }

    pub fn set_ui_text<S: Into<String>>(&mut self, ui_name: S, desc: S) {
        self.ui_name = ui_name.into();
        self.desc = desc.into();
    }

    pub fn set_array_len(&mut self, len: usize) {
        self.array_len = len;
        self.defaults.resize(len, self.typ.default_value());
        self.vals.lock().unwrap().resize(len, PropertyValue::Unset);
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

    /// This will clear all values, resetting them to the default
    pub fn clear_values(&self) {
        let vals = &mut self.vals.lock().unwrap();
        vals.clear();
        vals.resize(self.array_len, PropertyValue::Unset);
    }

    // Set

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

    pub fn unset(&self, i: usize) -> Result<()> {
        let vals = &mut self.vals.lock().unwrap();
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        vals[i] = PropertyValue::Unset;
        Ok(())
    }

    pub fn set_null(&self, i: usize) -> Result<()> {
        if !self.is_null_allowed {
            return Err(Error::PropertyNullNotAllowed)
        }
        let vals = &mut self.vals.lock().unwrap();
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        vals[i] = PropertyValue::Null;
        Ok(())
    }

    pub fn set_bool(&self, i: usize, val: bool) -> Result<()> {
        self.set_raw_value(i, PropertyValue::Bool(val))
    }
    pub fn set_u32(&self, i: usize, val: u32) -> Result<()> {
        if self.min_val.is_some() {
            let min = self.min_val.as_ref().unwrap().as_u32()?;
            if val < min {
                return Err(Error::PropertyOutOfRange);
            }
        }
        if self.max_val.is_some() {
            let max = self.max_val.as_ref().unwrap().as_u32()?;
            if val > max {
                return Err(Error::PropertyOutOfRange);
            }
        }
        self.set_raw_value(i, PropertyValue::Uint32(val))
    }
    pub fn set_f32(&self, i: usize, val: f32) -> Result<()> {
        if self.min_val.is_some() {
            let min = self.min_val.as_ref().unwrap().as_f32()?;
            if val < min {
                return Err(Error::PropertyOutOfRange);
            }
        }
        if self.max_val.is_some() {
            let max = self.max_val.as_ref().unwrap().as_f32()?;
            if val > max {
                return Err(Error::PropertyOutOfRange);
            }
        }
        self.set_raw_value(i, PropertyValue::Float32(val))
    }
    pub fn set_str<S: Into<String>>(&self, i: usize, val: S) -> Result<()> {
        self.set_raw_value(i, PropertyValue::Str(val.into()))
    }
    pub fn set_enum<S: Into<String>>(&self, i: usize, val: S) -> Result<()> {
        if self.typ != PropertyType::Enum {
            return Err(Error::PropertyWrongType)
        }
        let val = val.into();
        if !self.enum_items.as_ref().unwrap().contains(&val) {
            return Err(Error::PropertyWrongEnumItem)
        }
        self.set_raw_value(i, PropertyValue::Enum(val.into()))
    }
    pub fn set_buf(&self, i: usize, val: Vec<u8>) -> Result<()> {
        self.set_raw_value(i, PropertyValue::Buffer(Arc::new(val)))
    }
    pub fn set_node_id(&self, i: usize, val: SceneNodeId) -> Result<()> {
        self.set_raw_value(i, PropertyValue::SceneNodeId(val))
    }
    pub fn set_expr(&self, i: usize, val: SExprCode) -> Result<()> {
        if !self.is_expr_allowed {
            return Err(Error::PropertySExprNotAllowed)
        }
        let vals = &mut self.vals.lock().unwrap();
        if i >= vals.len() {
            return Err(Error::PropertyWrongIndex)
        }
        vals[i] = PropertyValue::SExpr(Arc::new(val));
        Ok(())
    }

    // Push

    pub fn push_null(&self) -> Result<usize> {
        if self.is_bounded() {
            return Err(Error::PropertyIsBounded)
        }
        let vals = &mut self.vals.lock().unwrap();
        let i = vals.len();
        vals.push(PropertyValue::Null);
        Ok(i)
    }

    pub fn push_bool(&self, val: bool) -> Result<usize> {
        let i = self.push_null()?;
        self.set_bool(i, val)?;
        Ok(i)
    }
    pub fn push_u32(&self, val: u32) -> Result<usize> {
        let i = self.push_null()?;
        self.set_u32(i, val)?;
        Ok(i)
    }
    pub fn push_f32(&self, val: f32) -> Result<usize> {
        let i = self.push_null()?;
        self.set_f32(i, val)?;
        Ok(i)
    }
    pub fn push_str<S: Into<String>>(&self, val: S) -> Result<usize> {
        let i = self.push_null()?;
        self.set_str(i, val)?;
        Ok(i)
    }
    pub fn push_enum<S: Into<String>>(&self, val: S) -> Result<usize> {
        let i = self.push_null()?;
        self.set_enum(i, val)?;
        Ok(i)
    }
    pub fn push_buf(&self, val: Vec<u8>) -> Result<usize> {
        let i = self.push_null()?;
        self.set_buf(i, val)?;
        Ok(i)
    }
    pub fn push_node_id(&self, val: SceneNodeId) -> Result<usize> {
        let i = self.push_null()?;
        self.set_node_id(i, val)?;
        Ok(i)
    }

    // Get

    fn is_bounded(&self) -> bool {
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
        Ok(vals[i].clone())
    }

    pub fn get_value(&self, i: usize) -> Result<PropertyValue> {
        let val = self.get_raw_value(i)?;
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
    pub fn get_buf(&self, i: usize) -> Result<Buffer> {
        self.get_value(i)?.as_buf()
    }
    pub fn get_buf_opt(&self, i: usize) -> Result<Option<Buffer>> {
        let val = self.get_value(i)?;
        if val.is_null() {
            return Ok(None)
        }
        Ok(Some(val.as_buf()?))
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
        self.get_value(i)?.as_sexpr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_getset() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop.set_f32(1, 4.).is_err());
        assert!(prop.is_unset(0).unwrap());
        assert!(prop.set_f32(0, 4.).is_ok());
        assert_eq!(prop.get_f32(0).unwrap(), 4.);
        assert!(!prop.is_unset(0).unwrap());
        prop.unset(0).unwrap();
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
        prop.set_null(0).unwrap();

        assert!(prop.get_f32_opt(1).is_err());
        assert!(prop.get_f32_opt(0).is_ok());
        assert!(prop.get_f32_opt(0).unwrap().is_none());

        prop.clear_values();
        assert!(prop.get_f32(0).is_ok());
        assert!(prop.get_f32_opt(0).unwrap().is_some());
        assert_eq!(prop.get_f32(0).unwrap(), 2.0);
    }

    #[test]
    fn test_nonnullable() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop.set_null(0).is_err());
        assert!(prop.is_unset(0).unwrap());
    }

    #[test]
    fn test_unbounded() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        prop.set_unbounded();
        assert_eq!(prop.get_len(), 0);
        prop.push_f32(2.0).unwrap();
        prop.push_f32(3.0).unwrap();
        assert_eq!(prop.get_len(), 2);

        prop.clear_values();
        assert_eq!(prop.get_len(), 0);
        prop.allow_null_values();
        prop.push_null().unwrap();
        prop.push_f32(4.0).unwrap();
        prop.push_f32(5.0).unwrap();
        assert_eq!(prop.get_len(), 3);
        assert!(prop.get_f32_opt(0).unwrap().is_none());
        assert!(prop.get_f32_opt(1).unwrap().is_some());
        assert!(prop.get_f32_opt(2).unwrap().is_some());
        assert!(prop.get_f32_opt(3).is_err());

        let mut prop2 = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        assert!(prop2.push_f32(4.0).is_err());
    }

    #[test]
    fn test_range() {
        let mut prop = Property::new("foo", PropertyType::Float32, PropertySubType::Null);
        let half_pi = 3.1415926535 / 2.;
        prop.set_range_f32(-half_pi, half_pi);
        assert!(prop.set_f32(0, 6.).is_err());
        assert!(prop.set_f32(0, 1.).is_ok());
    }

    #[test]
    fn test_enum() {
        let mut prop = Property::new("foo", PropertyType::Enum, PropertySubType::Null);
        prop.set_enum_items(vec!["ABC", "XYZ", "FOO"]).unwrap();
        assert!(prop.set_enum(0, "ABC").is_ok());
        assert!(prop.set_enum(0, "BAR").is_err());
    }
}
