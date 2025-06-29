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

use async_channel::{Receiver, Sender};
use async_trait::async_trait;
use darkfi_serial::{FutAsyncWriteExt, SerialDecodable, SerialEncodable};
use futures::{stream::FuturesUnordered, StreamExt};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    future::Future,
    str::FromStr,
    sync::{Arc, OnceLock, RwLock as SyncRwLock, Weak},
};

use crate::{
    error::{Error, Result},
    plugin,
    prop::{Property, PropertyAtomicGuard, PropertyPtr, Role},
    pubsub::{Publisher, PublisherPtr, Subscription},
    ui,
};

macro_rules! t { ($($arg:tt)*) => { trace!(target: "scene", $($arg)*); } }

pub struct ScenePath(VecDeque<String>);

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
            return Err(Error::InvalidScenePath)
        }
        if s == "/" {
            return Ok(ScenePath(VecDeque::new()))
        }

        let mut tokens = s.split('/');
        // Should start with a /
        let initial = tokens.next().expect("should not be empty");
        if !initial.is_empty() {
            return Err(Error::InvalidScenePath)
        }

        let mut path = VecDeque::new();
        for token in tokens {
            // There should not be any double slashes //
            if token.is_empty() {
                return Err(Error::InvalidScenePath)
            }
            path.push_back(token.to_string());
        }
        Ok(ScenePath(path))
    }
}

pub type SceneNodePtr = Arc<SceneNode>;
pub type SceneNodeWeak = Weak<SceneNode>;

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
    Layer = 3,
    Object = 4,
    VectorArt = 5,
    Text = 9,
    Texture = 13,
    Fonts = 10,
    Font = 11,
    //Plugins = 14,
    //Plugin = 15,
    ChatView = 16,
    EditBox = 17,
    ChatEdit = 18,
    Image = 19,
    Button = 20,
    Shortcut = 21,
    Gesture = 22,
    EmojiPicker = 23,
    SettingRoot = 24,
    Setting = 25,
    PluginRoot = 100,
    Plugin = 101,
}

pub struct SceneNode {
    pub name: String,
    pub id: SceneNodeId,
    pub typ: SceneNodeType,
    parent: SyncRwLock<Option<Weak<Self>>>,
    children: SyncRwLock<Vec<SceneNodePtr>>,
    pub props: Vec<PropertyPtr>,
    pub sigs: SyncRwLock<Vec<SignalPtr>>,
    pub methods: Vec<Method>,
    pub pimpl: OnceLock<Pimpl>,
}

impl SceneNode {
    pub fn root() -> SceneNodePtr {
        Arc::new(Self::new("", SceneNodeType::Root))
    }

    pub fn new<S: Into<String>>(name: S, typ: SceneNodeType) -> Self {
        Self {
            name: name.into(),
            id: rand::random(),
            typ,
            parent: SyncRwLock::new(None),
            children: SyncRwLock::new(vec![]),
            props: vec![],
            sigs: SyncRwLock::new(vec![]),
            methods: vec![],
            pimpl: OnceLock::new(),
        }
    }

    pub async fn setup<F, Fut>(self, pimpl_fn: F) -> Arc<Self>
    where
        F: FnOnce(SceneNodeWeak) -> Fut,
        Fut: Future<Output = Pimpl>,
    {
        let self_ = Arc::new(self);
        let weak_self = Arc::downgrade(&self_);

        // Initial props
        for prop in &self_.props {
            prop.set_parent(weak_self.clone());
        }

        let pimpl = pimpl_fn(weak_self).await;
        assert_eq!(Arc::strong_count(&self_), 1);
        self_.pimpl.set(pimpl).unwrap();
        self_
    }

    pub fn setup_null(self) -> Arc<Self> {
        let self_ = Arc::new(self);
        let weak_self = Arc::downgrade(&self_);

        // Initial props
        for prop in &self_.props {
            prop.set_parent(weak_self.clone());
        }

        assert_eq!(Arc::strong_count(&self_), 1);
        self_.pimpl.set(Pimpl::Null).unwrap();
        self_
    }

    pub fn pimpl<'a>(&'a self) -> &'a Pimpl {
        self.pimpl.get().unwrap()
    }

    pub fn link(self: Arc<Self>, child: SceneNodePtr) {
        let mut childs_parent = child.parent.write().unwrap();
        assert!(childs_parent.is_none());
        *childs_parent = Some(Arc::downgrade(&self));
        drop(childs_parent);

        let mut children = self.children.write().unwrap();
        children.push(child);
    }

    pub fn get_children(&self) -> Vec<SceneNodePtr> {
        self.children.read().unwrap().clone()
    }

    pub fn lookup_node<P: Into<ScenePath>>(self: Arc<Self>, path: P) -> Option<SceneNodePtr> {
        let path: ScenePath = path.into();
        let mut path = path.0;
        if path.is_empty() {
            return Some(self)
        }
        let child_name = path.pop_front().unwrap();
        for child in self.get_children() {
            if child.name == child_name {
                let path = ScenePath(path);
                return child.lookup_node(path)
            }
        }
        None
    }

    fn has_property(&self, name: &str) -> bool {
        self.props.iter().any(|prop| prop.name == name)
    }
    pub fn add_property(&mut self, prop: Property) -> Result<()> {
        if self.has_property(&prop.name) {
            return Err(Error::PropertyAlreadyExists)
        }
        self.props.push(Arc::new(prop));
        Ok(())
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
    pub fn set_property_bool(
        &self,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        name: &str,
        val: bool,
    ) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_bool(atom, role, 0, val)
    }
    pub fn set_property_u32(
        &self,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        name: &str,
        val: u32,
    ) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_u32(atom, role, 0, val)
    }
    pub fn set_property_f32(
        &self,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        name: &str,
        val: f32,
    ) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_f32(atom, role, 0, val)
    }
    pub fn set_property_str<S: Into<String>>(
        &self,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        name: &str,
        val: S,
    ) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_str(atom, role, 0, val)
    }
    pub fn set_property_node_id(
        &self,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        name: &str,
        val: SceneNodeId,
    ) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_node_id(atom, role, 0, val)
    }

    pub fn set_property_f32_vec(
        &self,
        atom: &mut PropertyAtomicGuard,
        role: Role,
        name: &str,
        val: Vec<f32>,
    ) -> Result<()> {
        self.get_property(name).ok_or(Error::PropertyNotFound)?.set_f32_vec(atom, role, val)
    }

    pub fn add_signal<S: Into<String>>(
        &mut self,
        name: S,
        desc: S,
        fmt: Vec<(S, S, CallArgType)>,
    ) -> Result<()> {
        let name = name.into();
        if self.has_signal(&name) {
            return Err(Error::SignalAlreadyExists)
        }
        let fmt = fmt
            .into_iter()
            .map(|(n, d, t)| CallArg { name: n.into(), desc: d.into(), typ: t })
            .collect();
        let mut sigs = self.sigs.write().unwrap();
        sigs.push(Arc::new(Signal {
            name: name.into(),
            desc: desc.into(),
            fmt,
            slots: SyncRwLock::new(HashMap::new()),
        }));
        Ok(())
    }

    fn has_signal(&self, name: &str) -> bool {
        let sigs = self.sigs.read().unwrap();
        sigs.iter().any(|sig| sig.name == name)
    }
    pub fn get_signal(&self, name: &str) -> Option<SignalPtr> {
        let sigs = self.sigs.read().unwrap();
        sigs.iter().find(|sig| sig.name == name).cloned()
    }

    pub fn register(&self, sig_name: &str, slot: Slot) -> Result<SlotId> {
        let slot_id = rand::random();
        let sig = self.get_signal(sig_name).ok_or(Error::SignalNotFound)?;
        let mut slots = sig.slots.write().unwrap();
        slots.insert(slot_id, slot);
        Ok(slot_id)
    }
    pub fn unregister(&self, sig_name: &str, slot_id: SlotId) -> Result<()> {
        let sig = self.get_signal(sig_name).ok_or(Error::SignalNotFound)?;
        let mut slots = sig.slots.write().unwrap();
        slots.remove(&slot_id).ok_or(Error::SlotNotFound)?;
        Ok(())
    }

    pub async fn trigger(&self, sig_name: &str, data: Vec<u8>) -> Result<()> {
        t!("trigger({sig_name}, {data:?}) [node={self:?}]");
        let sig = self.get_signal(sig_name).ok_or(Error::SignalNotFound)?;
        let futures = FuturesUnordered::new();
        let slots: Vec<_> = sig.slots.read().unwrap().values().cloned().collect();
        // TODO: autoremove failed slots
        for slot in slots {
            t!("  triggering {}", slot.name);
            // Trigger the slot
            let data = data.clone();
            futures.push(async move { slot.notify.send(data).await.is_ok() });
        }
        let success: Vec<_> = futures.collect().await;
        t!("trigger success: {success:?}");
        Ok(())
    }

    pub fn add_method<S: Into<String>>(
        &mut self,
        name: S,
        args: Vec<(S, S, CallArgType)>,
        result: Option<Vec<(S, S, CallArgType)>>,
    ) -> Result<()> {
        let name = name.into();
        if self.has_method(&name) {
            return Err(Error::MethodAlreadyExists)
        }
        let args = args
            .into_iter()
            .map(|(n, d, t)| CallArg { name: n.into(), desc: d.into(), typ: t })
            .collect();
        let result = match result {
            Some(result) => Some(
                result
                    .into_iter()
                    .map(|(n, d, t)| CallArg { name: n.into(), desc: d.into(), typ: t })
                    .collect(),
            ),
            None => None,
        };
        self.methods.push(Method::new(name.into(), args, result));
        Ok(())
    }

    fn has_method(&self, name: &str) -> bool {
        self.methods.iter().any(|sig| sig.name == name)
    }

    pub fn get_method(&self, name: &str) -> Option<&Method> {
        self.methods.iter().find(|method| method.name == name)
    }

    pub async fn call_method(&self, name: &str, arg_data: CallData) -> Result<Option<CallData>> {
        let method = self.get_method(name).ok_or(Error::MethodNotFound)?;
        Ok(method.call(arg_data).await)
    }

    pub fn subscribe_method_call(&self, name: &str) -> Result<MethodCallSub> {
        let method = self.get_method(name).ok_or(Error::MethodNotFound)?;
        let method_sub = method.pubsub.clone().subscribe();
        Ok(method_sub)
    }

    pub fn get_full_path(&self) -> Option<String> {
        let subpath = "/".to_string() + &self.name;
        let Some(parent_weak) = self.parent.read().unwrap().clone() else { return Some(subpath) };
        let Some(parent) = parent_weak.upgrade() else { return None };

        // Handle root /
        if parent.typ == SceneNodeType::Root {
            return Some(subpath)
        }

        Some(parent.get_full_path()? + &subpath)
    }
}

impl std::fmt::Debug for SceneNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = self.get_full_path() {
            write!(f, "{path}")
        } else {
            write!(f, "{}:{}", self.name, self.id)
        }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum CallArgType {
    Uint32,
    Uint64,
    Float32,
    Bool,
    Str,
    Hash,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CallArg {
    pub name: String,
    pub desc: String,
    pub typ: CallArgType,
}

pub type CallData = Vec<u8>;

pub type SlotId = u32;

#[derive(Clone)]
pub struct Slot {
    pub name: String,
    pub notify: Sender<CallData>,
}

impl Slot {
    pub fn new<S: Into<String>>(name: S) -> (Self, Receiver<CallData>) {
        let (notify, recvr) = async_channel::unbounded();
        let self_ = Self { name: name.into(), notify };
        (self_, recvr)
    }
}

type SignalPtr = Arc<Signal>;

pub struct Signal {
    pub name: String,
    #[allow(dead_code)]
    pub desc: String,
    #[allow(dead_code)]
    pub fmt: Vec<CallArg>,
    slots: SyncRwLock<HashMap<SlotId, Slot>>,
}

#[derive(Clone, Debug)]
pub struct MethodCall {
    pub data: CallData,
    pub send_res: Option<Sender<CallData>>,
}

impl MethodCall {
    fn new(data: CallData, send_res: Option<Sender<CallData>>) -> Self {
        Self { data, send_res }
    }
}

pub type MethodCallSub = Subscription<MethodCall>;

pub struct Method {
    pub name: String,
    pub args: Vec<CallArg>,
    pub result: Option<Vec<CallArg>>,
    pub pubsub: PublisherPtr<MethodCall>,
}

impl Method {
    fn new(name: String, args: Vec<CallArg>, result: Option<Vec<CallArg>>) -> Self {
        Self { name, args, result, pubsub: Publisher::new() }
    }

    async fn call(&self, data: CallData) -> Option<CallData> {
        match &self.result {
            Some(_) => {
                let (send_res, recv_res) = async_channel::bounded(1);
                self.pubsub.notify(MethodCall::new(data, Some(send_res)));
                Some(recv_res.recv().await.unwrap())
            }
            None => {
                self.pubsub.notify(MethodCall::new(data, None));
                None
            }
        }
    }
}

pub enum Pimpl {
    Null,
    Window(ui::WindowPtr),
    Layer(ui::LayerPtr),
    VectorArt(ui::VectorArtPtr),
    Text(ui::TextPtr),
    //EditBox(ui::EditBoxPtr),
    ChatEdit(ui::ChatEditPtr),
    ChatView(ui::ChatViewPtr),
    Image(ui::ImagePtr),
    Button(ui::ButtonPtr),
    Shortcut(ui::ShortcutPtr),
    Gesture(ui::GesturePtr),
    EmojiPicker(ui::EmojiPickerPtr),
    DarkIrc(plugin::DarkIrcPtr),
}

impl std::fmt::Debug for Pimpl {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Pimpl")
    }
}
