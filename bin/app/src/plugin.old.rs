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

use darkfi_serial::{Decodable, Encodable};
use std::{
    io::Cursor,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use crate::{
    error::Result,
    prop::{Property, PropertySubType, PropertyType},
    py::PythonPlugin,
    res::{ResourceId, ResourceManager},
    scene::{MethodResponseFn, SceneGraph, SceneGraphPtr, SceneNodeId, SceneNodeType},
};

pub enum Category {
    Null,
}

pub enum SubCategory {
    Null,
}

pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: String,
    pub build: String,
}

pub struct PluginMetadata {
    pub name: String,
    pub title: String,
    pub desc: String,
    pub author: String,
    pub version: SemVer,

    pub cat: Category,
    pub subcat: SubCategory,
    // icon

    // Permissions
    // whitelisted nodes + props/methods (use * for all)
    // /window/input/*
}

pub enum PluginEvent {
    // (signal_data, user_data)
    RecvSignal((Vec<u8>, Vec<u8>)),
}

pub type PluginInstancePtr = Arc<Mutex<Box<dyn PluginInstance + Send>>>;

pub trait Plugin {
    fn metadata(&self) -> PluginMetadata;
    // Spawns a new context and begins running the plugin in that context
    fn start(&self) -> Result<PluginInstancePtr>;
}

pub trait PluginInstance {
    fn update(&mut self, event: PluginEvent) -> Result<()>;
}

enum SentinelMethodEvent {
    ImportPlugin,
    StartPlugin(ResourceId),
}

pub struct Sentinel {
    scene_graph: SceneGraphPtr,
    plugins: ResourceManager<Box<dyn Plugin>>,
    insts: ResourceManager<PluginInstancePtr>,

    method_recvr: mpsc::Receiver<(SentinelMethodEvent, SceneNodeId, Vec<u8>, MethodResponseFn)>,
    method_sender: mpsc::SyncSender<(SentinelMethodEvent, SceneNodeId, Vec<u8>, MethodResponseFn)>,
}

impl Sentinel {
    pub fn new(scene_graph: SceneGraphPtr) -> Self {
        // Create /plugin in scene graph
        //
        // Methods provided in SceneGraph under /plugin:
        //
        // * import_plugin(pycode)

        let mut sg = scene_graph.lock().unwrap();
        let (method_sender, method_recvr) = mpsc::sync_channel(100);

        let node = sg.add_node("plugin", SceneNodeType::Plugins);

        let sender = method_sender.clone();
        let node_id = node.id;
        let method_fn = Box::new(move |arg_data, response_fn| {
            sender.send((SentinelMethodEvent::ImportPlugin, node_id, arg_data, response_fn));
        });
        node.add_method("import", vec![("pycode", "", PropertyType::Str)], vec![], method_fn);

        sg.link(node_id, SceneGraph::ROOT_ID).unwrap();
        drop(sg);

        Self {
            scene_graph,
            plugins: ResourceManager::new(),
            insts: ResourceManager::new(),
            method_recvr,
            method_sender,
        }
    }

    pub fn run(&mut self) {
        loop {
            // Monitor all running plugins
            // Check last update times
            // Kill any slowpokes

            // Check any SceneGraph method requests
            let deadline = Instant::now() + Duration::from_millis(4000);

            let Ok((event, node_id, arg_data, response_fn)) =
                self.method_recvr.recv_deadline(deadline)
            else {
                break
            };
            let res = match event {
                SentinelMethodEvent::ImportPlugin => self.import_py_plugin(node_id, arg_data),
                SentinelMethodEvent::StartPlugin(rid) => self.start_plugin(rid, node_id, arg_data),
            };
            response_fn(res);
        }
    }

    fn import_py_plugin(&mut self, node_id: SceneNodeId, arg_data: Vec<u8>) -> Result<Vec<u8>> {
        // Load the python code
        let mut cur = Cursor::new(&arg_data);
        let pycode = String::decode(&mut cur).unwrap();

        let plugin = Box::new(PythonPlugin::new(self.scene_graph.clone(), pycode));
        self.import_plugin(plugin)?;

        // This function doesn't return anything
        // Only success or an Err which is already handled elsewhere
        Ok(vec![])
    }

    fn import_plugin(&mut self, plugin: Box<dyn Plugin>) -> Result<()> {
        let metadata = plugin.metadata();
        let plugin_rid = self.plugins.alloc(plugin);

        let mut scene_graph = self.scene_graph.lock().unwrap();

        // Create /plugin/foo

        let node = scene_graph.add_node(metadata.name.clone(), SceneNodeType::Plugin);
        let node_id = node.id;

        // name
        let mut prop = Property::new("name", PropertyType::Str, PropertySubType::Null);
        prop.set_str(0, metadata.name);
        node.add_property(prop).unwrap();
        // title
        let mut prop = Property::new("title", PropertyType::Str, PropertySubType::Null);
        prop.set_str(0, metadata.title);
        node.add_property(prop).unwrap();
        // desc
        let mut prop = Property::new("desc", PropertyType::Str, PropertySubType::Null);
        prop.set_str(0, metadata.desc);
        node.add_property(prop).unwrap();
        // author
        let mut prop = Property::new("author", PropertyType::Str, PropertySubType::Null);
        prop.set_str(0, metadata.author);
        node.add_property(prop).unwrap();
        // version
        let mut prop = Property::new("version", PropertyType::Uint32, PropertySubType::Null);
        prop.set_array_len(3);
        prop.set_u32(0, metadata.version.major);
        prop.set_u32(1, metadata.version.minor);
        prop.set_u32(2, metadata.version.patch);
        node.add_property(prop).unwrap();
        // TODO: add version.pre and patch, and cat/subcat enums

        let mut prop = Property::new("insts", PropertyType::Uint32, PropertySubType::ResourceId);
        prop.set_ui_text("instance resource IDs", "The currently running instances of this plugin");
        prop.set_unbounded();
        node.add_property(prop).unwrap();

        // Add method start()

        let sender = self.method_sender.clone();
        let method_fn = Box::new(move |arg_data, response_fn| {
            sender.send((
                SentinelMethodEvent::StartPlugin(plugin_rid),
                node_id,
                arg_data,
                response_fn,
            ));
        });
        node.add_method("start", vec![], vec![("inst_rid", "", PropertyType::Uint32)], method_fn);

        // Link node

        let parent_id = scene_graph.lookup_node_id("/plugin").expect("no plugin node attached");
        scene_graph.link(node_id, parent_id).unwrap();

        Ok(())
    }

    fn start_plugin(
        &mut self,
        plugin_rid: ResourceId,
        node_id: SceneNodeId,
        arg_data: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let plugin = self.plugins.get(plugin_rid).expect("plugin not found");

        // Call init()
        // Spawn a new thread, allocate it an ID
        // Thread waits for events from the scene_graph and calls update() when they occur.
        // See src/net.rs:81 for an example
        let inst = plugin.start()?;
        let inst2 = inst.clone();
        let inst_rid = self.insts.alloc(inst);

        let _ = thread::spawn(move || {
            inst2.lock().unwrap().update(PluginEvent::RecvSignal((vec![], vec![]))).unwrap();
        });

        let scene_graph = self.scene_graph.lock().unwrap();
        let node = scene_graph.get_node(node_id).expect("node not found");
        let prop = node.get_property("insts").unwrap();
        prop.push_u32(inst_rid)?;

        // TODO: when the plugin finishes, the instance ID should be cleared up somehow
        // both from the resource manager and from the property
        // https://www.chromium.org/developers/design-documents/inter-process-communication/

        let mut reply = vec![];
        inst_rid.encode(&mut reply).unwrap();
        Ok(reply)
    }
}
