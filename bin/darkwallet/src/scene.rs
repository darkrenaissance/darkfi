use crate::error::{Error, Result};
use atomic_float::AtomicF32;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use std::{
    fmt,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

pub struct ScenePath(Vec<String>);

impl<S: Into<String>> From<S> for ScenePath {
    fn from(path: S) -> Self {
        let path: String = path.into();
        (&path).parse().expect("invalid ScenePath &str")
    }
}

impl fmt::Display for ScenePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/")?;
        for token in &self.0 {
            write!(f, "{}/", token)?;
        }
        Ok(())
    }
}

impl FromStr for ScenePath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        if s.is_empty() || s.chars().nth(0).unwrap() != '/' {
            return Err(Error::InvalidScenePath);
        }
        if s == "/" {
            return Ok(ScenePath(vec![]));
        }

        let mut tokens = s.split('/');
        // Should start with a /
        let initial = tokens.next().expect("should not be empty");
        if !initial.is_empty() {
            return Err(Error::InvalidScenePath);
        }

        let mut path = vec![];
        for token in tokens {
            // There should not be any double slashes //
            if token.is_empty() {
                return Err(Error::InvalidScenePath);
            }
            path.push(token.to_string());
        }
        Ok(ScenePath(path))
    }
}

pub type SceneGraphPtr = Arc<Mutex<SceneGraph>>;

pub struct SceneGraph {
    // Node 0 is always the root
    nodes: Vec<SceneNode>,
    freed: Vec<SceneNodeId>,
}

impl SceneGraph {
    pub const ROOT_ID: SceneNodeId = 0;

    pub fn new() -> Self {
        let root = SceneNode {
            name: "/".to_string(),
            id: 0,
            typ: SceneNodeType::Root,
            parents: vec![],
            children: vec![],
            props: vec![],
            sigs: vec![],
            methods: vec![],
        };
        Self { nodes: vec![root], freed: vec![] }
    }

    pub fn add_node<S: Into<String>>(&mut self, name: S, typ: SceneNodeType) -> &mut SceneNode {
        let node = SceneNode {
            name: name.into(),
            // We set this at the end
            id: 0,
            typ,
            parents: vec![],
            children: vec![],
            props: vec![],
            sigs: vec![],
            methods: vec![],
        };

        let node_id = if self.freed.is_empty() {
            let node_id = self.nodes.len() as SceneNodeId;
            self.nodes.push(node);
            node_id
        } else {
            let node_id = self.freed.pop().unwrap();
            let _ = std::mem::replace(&mut self.nodes[node_id as usize], node);
            node_id
        };

        self.nodes[node_id as usize].id = node_id;
        &mut self.nodes[node_id as usize]
    }

    pub fn remove_node(&mut self, id: SceneNodeId) -> Result<()> {
        let node = self.get_node_mut(id).ok_or(Error::NodeNotFound)?;
        if !node.parents.is_empty() {
            return Err(Error::NodeHasParents);
        }
        if !node.children.is_empty() {
            return Err(Error::NodeHasChildren);
        }
        node.name.clear();
        node.typ = SceneNodeType::Null;
        node.props.clear();
        self.freed.push(id);
        Ok(())
    }

    fn root(&self) -> &SceneNode {
        &self.nodes[0]
    }
    fn root_mut(&mut self) -> &mut SceneNode {
        &mut self.nodes[0]
    }

    fn exists(&self, id: SceneNodeId) -> bool {
        id < self.nodes.len() as SceneNodeId && !self.freed.contains(&id)
    }

    pub fn get_node(&self, id: SceneNodeId) -> Option<&SceneNode> {
        if self.exists(id) {
            Some(&self.nodes[id as usize])
        } else {
            None
        }
    }
    pub fn get_node_mut(&mut self, id: SceneNodeId) -> Option<&mut SceneNode> {
        if self.exists(id) {
            Some(&mut self.nodes[id as usize])
        } else {
            None
        }
    }

    pub fn link(&mut self, child_id: SceneNodeId, parent_id: SceneNodeId) -> Result<()> {
        // Check both nodes are not already linked
        let is_linked = self.is_linked(child_id, parent_id)?;
        if is_linked {
            return Err(Error::NodesAreLinked);
        }

        let parent = self.get_node(parent_id).unwrap();
        let parent_inf =
            SceneNodeInfo { name: parent.name.clone(), id: parent_id, typ: parent.typ };
        let child_name = &self.get_node(child_id).unwrap().name;
        if parent.has_child(child_name) {
            return Err(Error::NodeChildNameConflict);
        }

        // Link parent into child
        let child = self.get_node_mut(child_id).unwrap();
        if child.has_parent(&parent_inf.name) {
            return Err(Error::NodeParentNameConflict);
        }
        let child_inf = SceneNodeInfo { name: child.name.clone(), id: child_id, typ: child.typ };
        assert!(!child.has_parent_id(parent_id));
        child.parents.push(parent_inf);

        // Link child into parent
        let parent = self.get_node_mut(parent_id).unwrap();
        assert!(!parent.has_child(&child_inf.name));
        parent.children.push(child_inf);
        Ok(())
    }

    pub fn unlink(&mut self, child_id: SceneNodeId, parent_id: SceneNodeId) -> Result<()> {
        // Check both nodes are actually linked
        let is_linked = self.is_linked(child_id, parent_id)?;
        if !is_linked {
            return Err(Error::NodesNotLinked);
        }

        // Unlink parent from child
        let child = self.get_node_mut(child_id).unwrap();
        child.remove_parent(parent_id);

        // Unlink child from parent
        let parent = self.get_node_mut(parent_id).unwrap();
        parent.remove_child(child_id);
        Ok(())
    }

    pub fn is_linked(&self, child_id: SceneNodeId, parent_id: SceneNodeId) -> Result<bool> {
        let parent = self.get_node(parent_id).ok_or(Error::ParentNodeNotFound)?;
        let child = self.get_node(child_id).ok_or(Error::ChildNodeNotFound)?;
        let parent_has_child = parent.has_child_id(child_id);
        let child_has_parent = child.has_parent_id(parent_id);
        // Internal consistency checks
        if parent_has_child {
            assert!(child_has_parent);
        } else {
            assert!(!child_has_parent);
        }
        Ok(parent_has_child)
    }

    pub fn lookup_node_id<P: Into<ScenePath>>(&self, path: P) -> Option<SceneNodeId> {
        let path: ScenePath = path.into();
        let mut current_id = Self::ROOT_ID;
        for node_name in path.0 {
            let parent_node = self.get_node(current_id).unwrap();
            match parent_node.get_child(&node_name) {
                Some(child_id) => {
                    current_id = child_id;
                }
                None => return None,
            }
        }
        Some(current_id)
    }

    pub fn lookup_node<P: Into<ScenePath>>(&self, path: P) -> Option<&SceneNode> {
        let node_id = self.lookup_node_id(path)?;
        Some(self.get_node(node_id).unwrap())
    }
    pub fn lookup_node_mut<P: Into<ScenePath>>(&mut self, path: P) -> Option<&mut SceneNode> {
        let node_id = self.lookup_node_id(path)?;
        Some(self.get_node_mut(node_id).unwrap())
    }

    pub fn rename_node<S: Into<String>>(
        &mut self,
        node_id: SceneNodeId,
        node_name: S,
    ) -> Result<()> {
        let node_name = node_name.into();
        for sibling_inf in self.node_siblings(node_id)? {
            if sibling_inf.name == node_name {
                return Err(Error::NodeSiblingNameConflict)
            }
        }
        let node = self.get_node_mut(node_id).unwrap();
        node.name = node_name;
        Ok(())
    }
    fn node_siblings(&self, node_id: SceneNodeId) -> Result<Vec<SceneNodeInfo>> {
        let mut siblings = vec![];
        let node = self.get_node(node_id).ok_or(Error::NodeNotFound)?;
        for parent_inf in &node.parents {
            let parent = self.get_node(parent_inf.id).ok_or(Error::ParentNodeNotFound)?;
            let mut sibling_infs = parent
                .children
                .iter()
                .cloned()
                .filter(|child_inf| child_inf.id != node_id)
                .collect();
            siblings.append(&mut sibling_infs);
        }
        Ok(siblings)
    }
}

pub type SceneNodeId = u32;

#[derive(Debug, Copy, Clone, PartialEq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum SceneNodeType {
    Null = 0,
    Root = 1,
    Window = 2,
    WindowInput = 6,
    Keyboard = 7,
    Mouse = 8,
    RenderLayer = 3,
    RenderObject = 4,
    RenderMesh = 5,
    RenderText = 9,
    RenderTexture = 13,
    Fonts = 10,
    Font = 11,
    LinePosition = 12,
}

#[derive(Clone)]
pub struct SceneNodeInfo {
    pub name: String,
    pub id: SceneNodeId,
    pub typ: SceneNodeType,
}

pub struct SceneNode {
    pub name: String,
    pub id: SceneNodeId,
    pub typ: SceneNodeType,
    pub parents: Vec<SceneNodeInfo>,
    pub children: Vec<SceneNodeInfo>,
    pub props: Vec<Arc<Property>>,
    pub sigs: Vec<Signal>,
    pub methods: Vec<Method>,
}

impl SceneNode {
    fn has_parent_id(&self, parent_id: SceneNodeId) -> bool {
        self.parents.iter().any(|parent| parent.id == parent_id)
    }
    fn has_child_id(&self, child_id: SceneNodeId) -> bool {
        self.children.iter().any(|child| child.id == child_id)
    }

    fn has_parent(&self, parent_name: &str) -> bool {
        self.parents.iter().any(|parent| parent.name == parent_name)
    }
    fn has_child(&self, child_name: &str) -> bool {
        self.children.iter().any(|child| child.name == child_name)
    }
    fn get_child(&self, child_name: &str) -> Option<SceneNodeId> {
        for child in &self.children {
            if child.name == child_name {
                return Some(child.id);
            }
        }
        None
    }

    // Panics if parent is not linked
    fn remove_parent(&mut self, parent_id: SceneNodeId) {
        let parent_idx = self.parents.iter().position(|parent| parent.id == parent_id).unwrap();
        self.parents.swap_remove(parent_idx);
    }
    // Panics if child is not linked
    fn remove_child(&mut self, child_id: SceneNodeId) {
        let child_idx = self.children.iter().position(|child| child.id == child_id).unwrap();
        self.children.swap_remove(child_idx);
    }

    pub fn iter_children<'a>(
        &'a self,
        scene_graph: &'a SceneGraph,
        typ: SceneNodeType,
    ) -> impl Iterator<Item = &'a Self> + 'a {
        self.children
            .iter()
            .filter(move |child_inf| child_inf.typ == typ)
            .map(|child_inf| scene_graph.get_node(child_inf.id).unwrap())
    }

    pub fn add_property<S: Into<String>>(
        &mut self,
        name: S,
        typ: PropertyType,
    ) -> Result<Arc<Property>> {
        let name = name.into();
        if self.has_property(&name) {
            return Err(Error::PropertyAlreadyExists);
        }
        let prop = Property::new(name, typ);
        self.props.push(prop.clone());
        Ok(prop)
    }

    // Convenience methods
    pub fn add_property_buf(&mut self, name: &str, val: Vec<u8>) -> Result<()> {
        self.add_property(name, PropertyType::Buffer)?.set_buf(val)?;
        Ok(())
    }
    pub fn add_property_bool(&mut self, name: &str, val: bool) -> Result<()> {
        self.add_property(name, PropertyType::Bool)?.set_bool(val)?;
        Ok(())
    }
    pub fn add_property_u32(&mut self, name: &str, val: u32) -> Result<()> {
        self.add_property(name, PropertyType::Uint32)?.set_u32(val)?;
        Ok(())
    }
    pub fn add_property_f32(&mut self, name: &str, val: f32) -> Result<()> {
        self.add_property(name, PropertyType::Float32)?.set_f32(val)?;
        Ok(())
    }
    pub fn add_property_str<S: Into<String>>(&mut self, name: &str, val: S) -> Result<()> {
        self.add_property(name, PropertyType::Str)?.set_str(val)?;
        Ok(())
    }
    pub fn add_property_node_id(&mut self, name: &str, val: SceneNodeId) -> Result<()> {
        self.add_property(name, PropertyType::SceneNodeId)?.set_node_id(val)?;
        Ok(())
    }

    fn has_property(&self, name: &str) -> bool {
        self.props.iter().any(|prop| prop.name == name)
    }

    pub fn get_property(&self, name: &str) -> Option<Arc<Property>> {
        self.props.iter().find(|prop| prop.name == name).map(|prop| prop.clone())
    }

    // Convenience methods
    pub fn get_property_bool(&self, name: &str) -> Result<bool> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_bool()
    }
    pub fn get_property_u32(&self, name: &str) -> Result<u32> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_u32()
    }
    pub fn get_property_f32(&self, name: &str) -> Result<f32> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_f32()
    }
    pub fn get_property_str(&self, name: &str) -> Result<String> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_str()
    }
    pub fn get_property_node_id(&self, name: &str) -> Result<SceneNodeId> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_node_id()
    }
    // Setters
    pub fn set_property_bool(&self, name: &str, val: bool) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_bool(val)
    }
    pub fn set_property_u32(&self, name: &str, val: u32) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_u32(val)
    }
    pub fn set_property_f32(&self, name: &str, val: f32) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_f32(val)
    }
    pub fn set_property_str<S: Into<String>>(&self, name: &str, val: S) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_str(val)
    }
    pub fn set_property_node_id(&self, name: &str, val: SceneNodeId) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_node_id(val)
    }

    pub fn add_signal<S: Into<String>>(&mut self, name: S) -> Result<()> {
        let name = name.into();
        if self.has_signal(&name) {
            return Err(Error::SignalAlreadyExists);
        }
        self.sigs.push(Signal { name: name.into(), slots: vec![], freed: vec![] });
        Ok(())
    }

    fn has_signal(&self, name: &str) -> bool {
        self.sigs.iter().any(|sig| sig.name == name)
    }
    pub fn get_signal(&self, name: &str) -> Option<&Signal> {
        self.sigs.iter().find(|sig| sig.name == name)
    }
    fn get_signal_mut(&mut self, name: &str) -> Option<&mut Signal> {
        self.sigs.iter_mut().find(|sig| sig.name == name)
    }

    pub fn register(&mut self, sig_name: &str, slot: Slot) -> Result<SlotId> {
        let sig = self.get_signal_mut(sig_name).ok_or(Error::SignalNotFound)?;
        let slot_id = if sig.freed.is_empty() {
            let slot_id = sig.slots.len() as SlotId;
            sig.slots.push(slot);
            slot_id
        } else {
            let slot_id = sig.freed.pop().unwrap();
            let _ = std::mem::replace(&mut sig.slots[slot_id as usize], slot);
            slot_id
        };
        Ok(slot_id)
    }
    pub fn unregister(&mut self, sig_name: &str, slot_id: SlotId) -> Result<()> {
        let sig = self.get_signal_mut(sig_name).ok_or(Error::SignalNotFound)?;
        if !sig.slot_exists(slot_id) {
            return Err(Error::SlotNotFound);
        }
        sig.freed.push(slot_id);
        Ok(())
    }
    pub fn trigger(&self, sig_name: &str) -> Result<()> {
        let sig = self.get_signal(sig_name).ok_or(Error::SignalNotFound)?;
        for (_, slot) in sig.get_slots() {
            // Trigger the slot
            slot.call();
        }
        Ok(())
    }

    pub fn add_method<S: Into<String>>(
        &mut self,
        name: S,
        args: Vec<(S, PropertyType)>,
        result: Vec<(S, PropertyType)>,
    ) {
        let args = args.into_iter().map(|(s, p)| (s.into(), p)).collect();
        let result = result.into_iter().map(|(s, p)| (s.into(), p)).collect();
        self.methods.push(Method { name: name.into(), args, result, queue: vec![] })
    }

    pub fn get_method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|method| method.name == name)
    }
    fn get_method_mut(&mut self, name: &str) -> Option<&mut Method> {
        self.methods.iter_mut().find(|method| method.name == name)
    }

    pub fn call_method(
        &mut self,
        name: &str,
        arg_data: Vec<u8>,
        response_fn: MethodResponseFn,
    ) -> Result<()> {
        let method = self.get_method_mut(name).ok_or(Error::MethodNotFound)?;
        method.queue.push((arg_data, response_fn));
        Ok(())
    }
}

type BufferGuard<'a> = MutexGuard<'a, Vec<u8>>;

#[derive(Debug, Copy, Clone, PartialEq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum PropertyType {
    Null = 0,
    Buffer = 1,
    Bool = 2,
    Uint32 = 3,
    Float32 = 4,
    Str = 5,
    SceneNodeId = 6,
}

pub enum PropertyValue {
    Null,
    Buffer(Mutex<Vec<u8>>),
    Bool(AtomicBool),
    Uint32(AtomicU32),
    Float32(AtomicF32),
    Str(Mutex<String>),
    SceneNodeId(AtomicU32),
}

pub struct Property {
    pub name: String,
    //typ: PropertyType,
    pub val: PropertyValue,
}

impl Property {
    fn new(name: String, typ: PropertyType) -> Arc<Self> {
        let val = match typ {
            PropertyType::Null => PropertyValue::Null,
            PropertyType::Buffer => PropertyValue::Buffer(Mutex::new(Vec::new())),
            PropertyType::Bool => PropertyValue::Bool(AtomicBool::new(false)),
            PropertyType::Uint32 => PropertyValue::Uint32(AtomicU32::new(0)),
            PropertyType::Float32 => PropertyValue::Float32(AtomicF32::new(0.)),
            PropertyType::Str => PropertyValue::Str(Mutex::new(String::new())),
            PropertyType::SceneNodeId => PropertyValue::SceneNodeId(AtomicU32::new(0)),
        };
        Arc::new(Self { name, val })
    }

    pub fn get_type(&self) -> PropertyType {
        match self.val {
            PropertyValue::Null => PropertyType::Null,
            PropertyValue::Buffer(_) => PropertyType::Buffer,
            PropertyValue::Bool(_) => PropertyType::Bool,
            PropertyValue::Uint32(_) => PropertyType::Uint32,
            PropertyValue::Float32(_) => PropertyType::Float32,
            PropertyValue::Str(_) => PropertyType::Str,
            PropertyValue::SceneNodeId(_) => PropertyType::SceneNodeId,
        }
    }

    pub fn get_buf<'a>(&'a self) -> Result<BufferGuard<'a>> {
        match &self.val {
            PropertyValue::Buffer(propval) => Ok(propval.lock().unwrap()),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn get_bool(&self) -> Result<bool> {
        match &self.val {
            PropertyValue::Bool(propval) => Ok(propval.load(Ordering::SeqCst)),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn get_u32(&self) -> Result<u32> {
        match &self.val {
            PropertyValue::Uint32(propval) => Ok(propval.load(Ordering::SeqCst)),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn get_f32(&self) -> Result<f32> {
        match &self.val {
            PropertyValue::Float32(propval) => Ok(propval.load(Ordering::SeqCst)),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn get_str(&self) -> Result<String> {
        match &self.val {
            PropertyValue::Str(propval) => Ok(propval.lock().unwrap().clone()),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn get_node_id(&self) -> Result<SceneNodeId> {
        match &self.val {
            PropertyValue::SceneNodeId(propval) => Ok(propval.load(Ordering::SeqCst)),
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn set_buf(&self, val: Vec<u8>) -> Result<()> {
        match &self.val {
            PropertyValue::Buffer(propval) => {
                let mut buf = propval.lock().unwrap();
                let _ = std::mem::replace(&mut *buf, val);
                Ok(())
            }
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn set_bool(&self, val: bool) -> Result<()> {
        match &self.val {
            PropertyValue::Bool(propval) => {
                propval.store(val, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn set_u32(&self, val: u32) -> Result<()> {
        match &self.val {
            PropertyValue::Uint32(propval) => {
                propval.store(val, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn set_f32(&self, val: f32) -> Result<()> {
        match &self.val {
            PropertyValue::Float32(propval) => {
                propval.store(val, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn set_str<S: Into<String>>(&self, val: S) -> Result<()> {
        match &self.val {
            PropertyValue::Str(propval) => {
                let mut pv = propval.lock().unwrap();
                *pv = val.into();
                Ok(())
            }
            _ => Err(Error::PropertyWrongType),
        }
    }

    pub fn set_node_id(&self, val: SceneNodeId) -> Result<()> {
        match &self.val {
            PropertyValue::SceneNodeId(propval) => {
                propval.store(val, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Error::PropertyWrongType),
        }
    }
}

type SlotFn = Box<dyn Fn() + Send>;
pub type SlotId = u32;

pub struct Slot {
    pub name: String,
    pub func: SlotFn,
}

impl Slot {
    fn call(&self) {
        (self.func)()
    }
}

pub struct Signal {
    pub name: String,
    slots: Vec<Slot>,
    freed: Vec<SlotId>,
}

impl Signal {
    fn slot_exists(&self, slot_id: SlotId) -> bool {
        if slot_id >= self.slots.len() as SlotId {
            return false;
        }
        return !self.freed.contains(&slot_id);
    }

    pub fn get_slots<'a>(&'a self) -> impl Iterator<Item = (SlotId, &'a Slot)> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(slot_id, _)| !self.freed.contains(&(*slot_id as SlotId)))
            .map(|(slot_id, slot)| (slot_id as SlotId, slot))
    }

    pub fn lookup_slot_id(&self, slot_name: &str) -> Option<SlotId> {
        for (slot_id, slot) in self.get_slots() {
            if slot.name == slot_name {
                return Some(slot_id);
            }
        }
        None
    }
}

type MethodResponseFn = Box<dyn Fn(Result<Vec<u8>>) + Send>;

pub struct Method {
    pub name: String,
    pub args: Vec<(String, PropertyType)>,
    pub result: Vec<(String, PropertyType)>,
    pub queue: Vec<(Vec<u8>, MethodResponseFn)>,
}
