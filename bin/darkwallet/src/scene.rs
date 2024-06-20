use async_channel::Sender;
use async_lock::Mutex;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use futures::{stream::FuturesUnordered, StreamExt};
use std::{fmt, str::FromStr, sync::Arc};

use crate::{
    error::{Error, Result},
    prop::{Property, PropertyPtr, PropertyType},
    ui,
};

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
    Plugins = 14,
    Plugin = 15,
    ChatView = 16,
    EditBox = 17,
}

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

pub type SceneGraphPtr = Arc<std::sync::Mutex<SceneGraph>>;
pub type SceneGraphPtr2 = Arc<Mutex<SceneGraph>>;

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
            pimpl: Pimpl::Null,
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
            pimpl: Pimpl::Null,
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
        node.name = node_name.clone();

        // Now update it for all children and parents too
        let parent_ids: Vec<_> = node.parents.iter().map(|parent_inf| parent_inf.id).collect();
        let child_ids: Vec<_> = node.children.iter().map(|child_inf| child_inf.id).collect();

        'next_parent: for parent_id in parent_ids {
            let parent = self.get_node_mut(parent_id).unwrap();
            for child in &mut parent.children {
                if child.id == node_id {
                    child.name = node_name.clone();
                    continue 'next_parent
                }
            }
            panic!("child {} not found in parent {}!", node_id, parent.id)
        }

        'next_child: for child_id in child_ids {
            let child = self.get_node_mut(child_id).unwrap();
            for parent in &mut child.parents {
                if parent.id == node_id {
                    parent.name = node_name.clone();
                    continue 'next_child
                }
            }
            panic!("parent {} not found in child {}!", node_id, child.id)
        }
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

    pub fn scan_dangling(&self) -> Vec<SceneNodeId> {
        let mut dangling = vec![];
        for node in &self.nodes {
            if node.id == Self::ROOT_ID {
                continue
            }
            if self.freed.contains(&node.id) {
                continue
            }
            if node.parents.is_empty() {
                dangling.push(node.id);
            }
        }
        dangling
    }
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
    pub props: Vec<PropertyPtr>,
    pub sigs: Vec<Signal>,
    pub methods: Vec<Method>,
    pub pimpl: Pimpl,
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

    pub fn get_children(&self, allowed_types: &[SceneNodeType]) -> Vec<SceneNodeInfo> {
        self.children
            .iter()
            .cloned()
            .filter(move |child_inf| allowed_types.contains(&child_inf.typ))
            .collect()
    }
    pub fn get_children2(&self) -> Vec<SceneNodeInfo> {
        self.children.iter().cloned().collect()
    }

    pub fn add_property(&mut self, prop: Property) -> Result<()> {
        if self.has_property(&prop.name) {
            return Err(Error::PropertyAlreadyExists);
        }
        self.props.push(Arc::new(prop));
        Ok(())
    }

    fn has_property(&self, name: &str) -> bool {
        self.props.iter().any(|prop| prop.name == name)
    }

    pub fn get_property(&self, name: &str) -> Option<PropertyPtr> {
        self.props.iter().find(|prop| prop.name == name).map(|prop| prop.clone())
    }

    // Convenience methods
    pub fn get_property_bool(&self, name: &str) -> Result<bool> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_bool(0)
    }
    pub fn get_property_u32(&self, name: &str) -> Result<u32> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_u32(0)
    }
    pub fn get_property_f32(&self, name: &str) -> Result<f32> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_f32(0)
    }
    pub fn get_property_str(&self, name: &str) -> Result<String> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_str(0)
    }
    pub fn get_property_enum(&self, name: &str) -> Result<String> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_enum(0)
    }
    pub fn get_property_node_id(&self, name: &str) -> Result<SceneNodeId> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.get_node_id(0)
    }
    // Setters
    pub fn set_property_bool(&self, name: &str, val: bool) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_bool(0, val)
    }
    pub fn set_property_u32(&self, name: &str, val: u32) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_u32(0, val)
    }
    pub fn set_property_f32(&self, name: &str, val: f32) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_f32(0, val)
    }
    pub fn set_property_str<S: Into<String>>(&self, name: &str, val: S) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_str(0, val)
    }
    pub fn set_property_node_id(&self, name: &str, val: SceneNodeId) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_node_id(0, val)
    }

    pub fn add_signal<S: Into<String>>(
        &mut self,
        name: S,
        desc: S,
        fmt: Vec<(S, S, PropertyType)>,
    ) -> Result<()> {
        let name = name.into();
        if self.has_signal(&name) {
            return Err(Error::SignalAlreadyExists);
        }
        let fmt = fmt
            .into_iter()
            .map(|(n, d, t)| CallArg { name: n.into(), desc: d.into(), typ: t })
            .collect();
        self.sigs.push(Signal {
            name: name.into(),
            desc: desc.into(),
            fmt,
            slots: vec![],
            freed: vec![],
        });
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
    pub async fn trigger(&self, sig_name: &str, data: Vec<u8>) -> Result<()> {
        let sig = self.get_signal(sig_name).ok_or(Error::SignalNotFound)?;
        let futures = FuturesUnordered::new();
        for (_, slot) in sig.get_slots() {
            // Trigger the slot
            futures.push(async {
                // Ignore the result
                let _ = slot.notify.send(data.clone()).await;
            });
        }
        let _: Vec<_> = futures.collect().await;
        Ok(())
    }

    pub fn add_method<S: Into<String>>(
        &mut self,
        name: S,
        args: Vec<(S, S, PropertyType)>,
        result: Vec<(S, S, PropertyType)>,
        method_fn: MethodRequestFn,
    ) -> Result<()> {
        let name = name.into();
        if self.has_signal(&name) {
            return Err(Error::MethodAlreadyExists);
        }
        let args = args
            .into_iter()
            .map(|(n, d, t)| CallArg { name: n.into(), desc: d.into(), typ: t })
            .collect();
        let result = result
            .into_iter()
            .map(|(n, d, t)| CallArg { name: n.into(), desc: d.into(), typ: t })
            .collect();
        self.methods.push(Method { name: name.into(), args, result, method_fn });
        Ok(())
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
        let method = self.get_method(name).ok_or(Error::MethodNotFound)?;
        (method.method_fn)(arg_data, response_fn);
        Ok(())
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CallArg {
    pub name: String,
    pub desc: String,
    pub typ: PropertyType,
}

type SlotFn = Box<dyn Fn(Vec<u8>) + Send>;
pub type SlotId = u32;

pub struct Slot {
    pub name: String,
    pub notify: Sender<Vec<u8>>,
}

pub struct Signal {
    pub name: String,
    pub desc: String,
    pub fmt: Vec<CallArg>,
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

type MethodRequestFn = Box<dyn Fn(Vec<u8>, MethodResponseFn) + Send + Sync>;
pub type MethodResponseFn = Box<dyn Fn(Result<Vec<u8>>) + Send + Sync>;

pub struct Method {
    pub name: String,
    pub args: Vec<CallArg>,
    pub result: Vec<CallArg>,
    method_fn: MethodRequestFn,
}

pub enum Pimpl {
    Null,
    //EditBox(editbox::EditBoxPtr),
    //ChatView(chatview::ChatViewPtr),
    Window(ui::WindowPtr),
    RenderLayer(ui::RenderLayerPtr),
    Mesh(ui::MeshPtr),
}

impl std::fmt::Debug for SceneNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}':{}", self.name, self.id)
    }
}
