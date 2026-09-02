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

use async_lock::Mutex;
use darkfi_serial::{async_trait, deserialize, Decodable, Encodable, SerialDecodable, VarInt};
use std::{io::Cursor, sync::Arc};
use zeromq::{Socket, SocketRecv, SocketSend};

use crate::{
    app::node::{create_layer, create_vector_art},
    error::{Error, Result},
    expr::{decompile, Compiler, MachineGlobals, SExprCode, SExprMachine, SExprVal},
    gfx::{gfxtag, Renderer},
    prop::{PropertyType, Role},
    scene::{Pimpl, SceneNodeId, SceneNodePtr, SceneNodeType, ScenePath, Slot},
    ui::{
        get_ui_object3, get_ui_object_ptr, Layer, RedrawTrigger, ShapeVertex, VectorArt,
        VectorShape,
    },
    ExecutorPtr,
};

/// Stop the UI tasks and clear the buffers of a subtree about to be
/// removed at runtime, mirroring Window::stop(). Only pimpl types with a
/// UIObject mapping are touched; others (Window, Setting, plugins, Null)
/// keep their tasks until process exit.
fn stop_ui_subtree(node: &SceneNodePtr) {
    if matches!(
        node.pimpl(),
        Pimpl::Layer(_) |
            Pimpl::ScrollLayer(_) |
            Pimpl::VectorArt(_) |
            Pimpl::Text(_) |
            Pimpl::TextScramble(_) |
            Pimpl::Edit(_) |
            Pimpl::ChatView(_) |
            Pimpl::Image(_) |
            Pimpl::Video(_) |
            Pimpl::Button(_) |
            Pimpl::EmojiPicker(_) |
            Pimpl::Shortcut(_) |
            Pimpl::Menu(_) |
            Pimpl::TokenTable(_)
    ) {
        get_ui_object3(node).stop();
    }
    for child in node.get_children() {
        stop_ui_subtree(&child);
    }
}

/// Run a freshly compiled expr on a throwaway machine so unknown
/// variables (typos) reject the set request instead of failing silently
/// on every eval afterwards. The dummy values are irrelevant; only name
/// resolution matters and the globals are discarded. `global_names` are
/// the variable names the property's real eval can provide.
fn check_expr(code: &SExprCode, global_names: &[String]) -> Result<()> {
    let mut globals: MachineGlobals = vec![];
    for name in global_names {
        globals.push((name.clone(), SExprVal::Float32(1.)));
    }
    let mut machine = SExprMachine { globals, stmts: code };
    machine.call()?;
    Ok(())
}

const USE_IPV6: bool = true;

#[derive(Debug, SerialDecodable)]
#[repr(u8)]
enum Command {
    Hello = 0,
    AddNode = 1,
    RemoveNode = 9,
    RenameNode = 23,
    ScanDangling = 24,
    LookupNodeId = 12,
    AddProperty = 11,
    LinkNode = 2,
    UnlinkNode = 8,
    GetInfo = 19,
    GetChildren = 4,
    GetParents = 5,
    GetProperties = 3,
    GetPropertyValue = 6,
    SetPropertyValue = 7,
    GetSignals = 14,
    RegisterSlot = 15,
    UnregisterSlot = 16,
    LookupSlotId = 17,
    GetSlots = 18,
    GetMethods = 20,
    GetMethod = 21,
    CallMethod = 22,
}

// Missing calls todo:
// GetPropLen
// UnsetProperty
// SetPropertyNull
// PropertyPushNull
// PropertyPush
// PropertyIsUnset

pub struct ZeroMQAdapter {
    /*
    // req-reply commands
    req_socket: zmq::Socket,
    // We cannot share zmq sockets across threads, and we cannot quickly spawn
    // pub sockets due to address reuse errors.
    slot_sender: mpsc::SyncSender<(Vec<u8>, Vec<u8>)>,
    slot_recvr: Option<mpsc::Receiver<(Vec<u8>, Vec<u8>)>>,
    */
    sg_root: SceneNodePtr,
    renderer: Renderer,
    redraw: RedrawTrigger,
    ex: ExecutorPtr,

    zmq_rep: Mutex<zeromq::RepSocket>,
    zmq_pub: Mutex<zeromq::PubSocket>,
}

impl ZeroMQAdapter {
    pub async fn new(
        sg_root: SceneNodePtr,
        renderer: Renderer,
        redraw: RedrawTrigger,
        ex: ExecutorPtr,
    ) -> Arc<Self> {
        let mut zmq_rep = zeromq::RepSocket::new();
        if USE_IPV6 {
            zmq_rep.bind("tcp://[::]:9484").await.unwrap();
        } else {
            zmq_rep.bind("tcp://0.0.0.0:9484").await.unwrap();
        }

        let mut zmq_pub = zeromq::PubSocket::new();
        if USE_IPV6 {
            zmq_pub.bind("tcp://[::]:9485").await.unwrap();
        } else {
            zmq_pub.bind("tcp://0.0.0.0:9485").await.unwrap();
        }

        Arc::new(Self {
            sg_root,
            renderer,
            redraw,
            ex,
            zmq_rep: Mutex::new(zmq_rep),
            zmq_pub: Mutex::new(zmq_pub),
        })
    }

    pub async fn run(self: Arc<Self>) {
        loop {
            let req = self.zmq_rep.lock().await.recv().await.unwrap();
            assert_eq!(req.len(), 2);
            let cmd = req.get(0).unwrap().to_vec();
            assert_eq!(cmd.len(), 1);
            let payload = req.get(1).unwrap().to_vec();

            let cmd = deserialize(&cmd).unwrap();
            debug!(target: "req", "zmq: {:?} {:?}", cmd, payload);

            let self2 = self.clone();
            match self2.process_request(cmd, payload).await {
                Ok(reply) => {
                    let mut m = zeromq::ZmqMessage::from(vec![0u8]);
                    m.push_back(reply.into());

                    // [errc:1] [reply]
                    self.zmq_rep.lock().await.send(m).await.unwrap();
                }
                Err(err) => {
                    let errc = err as u8;
                    warn!(target: "req", "errc {}: {}", errc, err);

                    let mut m = zeromq::ZmqMessage::from(vec![errc]);
                    m.push_back(vec![].into());

                    // [errc:1] [reply]
                    self.zmq_rep.lock().await.send(m).await.unwrap();
                }
            }
        }
    }

    async fn process_request(self: Arc<Self>, cmd: Command, payload: Vec<u8>) -> Result<Vec<u8>> {
        let mut cur = Cursor::new(&payload);
        let mut reply = vec![];
        match cmd {
            Command::Hello => {
                debug!(target: "req", "hello()");
                assert_eq!(payload.len(), 0);
                "hello".encode(&mut reply).unwrap();
            }
            Command::GetInfo => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({})", cmd, node_id);

                let node = scene_graph.get_node(node_id).ok_or(Error::NodeNotFound)?;
                node.name.encode(&mut reply).unwrap();
                node.typ.encode(&mut reply).unwrap();
                */
            }
            Command::GetChildren => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                debug!(target: "req", "{cmd:?}({node_path})");
                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                let children: Vec<_> = node
                    .get_children()
                    .iter()
                    .map(|node| (node.name.clone(), node.id, node.typ))
                    .collect();
                children.encode(&mut reply).unwrap();
            }
            Command::GetParents => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({})", cmd, node_id);

                let node = scene_graph.get_node(node_id).ok_or(Error::NodeNotFound)?;
                let parents: Vec<_> = node
                    .parents
                    .iter()
                    .map(|node_inf| (node_inf.name.clone(), node_inf.id, node_inf.typ))
                    .collect();
                parents.encode(&mut reply).unwrap();
                */
            }
            Command::GetProperties => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                debug!(target: "req", "{cmd:?}({node_path})");
                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                VarInt(node.props.len() as u64).encode(&mut reply).unwrap();
                for prop in &node.props {
                    prop.name.encode(&mut reply).unwrap();
                    prop.typ.encode(&mut reply).unwrap();
                    prop.subtype.encode(&mut reply).unwrap();
                    //prop.defaults.encode(&mut reply).unwrap();
                    prop.ui_name.encode(&mut reply).unwrap();
                    prop.desc.encode(&mut reply).unwrap();
                    prop.is_null_allowed.encode(&mut reply).unwrap();
                    prop.is_expr_allowed.encode(&mut reply).unwrap();
                    (prop.array_len as u32).encode(&mut reply).unwrap();
                    prop.min_val.encode(&mut reply).unwrap();
                    prop.max_val.encode(&mut reply).unwrap();
                    prop.enum_items.encode(&mut reply).unwrap();

                    let depends: Vec<_> = prop
                        .get_depends()
                        .into_iter()
                        .map(|d| (d.i as u32, d.local_name))
                        .collect();
                    depends.encode(&mut reply).unwrap();
                }
            }
            Command::GetPropertyValue => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let prop_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {prop_name})");
                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                let prop = node.get_property(&prop_name).ok_or(Error::PropertyNotFound)?;
                prop.typ.encode(&mut reply).unwrap();
                VarInt(prop.get_len() as u64).encode(&mut reply).unwrap();
                for i in 0..prop.get_len() {
                    // Check the raw stored value, since get_value() resolves
                    // exprs to their cached/default value and would never
                    // report the EXPR status.
                    let val = prop.get_raw_value(i)?;
                    if val.is_expr() {
                        3u8.encode(&mut reply).unwrap();
                        let expr = prop.get_expr(i)?;
                        decompile(&expr).encode(&mut reply).unwrap();
                    } else if val.is_unset() {
                        // A null default encodes zero payload bytes, so it
                        // is reported as the NULL status instead of UNSET.
                        // This mirrors the old get_value() semantics, where
                        // an unset index with a null default resolved to
                        // null.
                        let default = &prop.defaults[i];
                        if default.is_null() {
                            2u8.encode(&mut reply).unwrap();
                        } else {
                            1u8.encode(&mut reply).unwrap();
                            // Shapes are not serialized on the get path;
                            // the python client shows a "<...>" placeholder.
                            if prop.typ != PropertyType::VectorShape {
                                default.encode(&mut reply).unwrap();
                            }
                        }
                    } else if val.is_null() {
                        2u8.encode(&mut reply).unwrap();
                    } else {
                        0u8.encode(&mut reply).unwrap();
                        if prop.typ != PropertyType::VectorShape {
                            val.encode(&mut reply).unwrap();
                        }
                    }
                }
            }
            Command::SetPropertyValue => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let prop_name = String::decode(&mut cur).unwrap();
                let prop_i = u32::decode(&mut cur).unwrap() as usize;
                let prop_type = PropertyType::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {prop_name}, {prop_i}, {prop_type:?})");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let prop = node.get_property(&prop_name).ok_or(Error::PropertyNotFound)?;

                let atom = &mut self.redraw.make_guard(gfxtag!("ZeroMQAdapter::SetPropertyValue"));

                match prop_type {
                    PropertyType::Null => {
                        prop.set_null(atom, Role::User, prop_i)?;
                    }
                    PropertyType::Bool => {
                        let val = bool::decode(&mut cur).unwrap();
                        prop.set_bool(atom, Role::User, prop_i, val)?;
                    }
                    PropertyType::Uint32 => {
                        let val = u32::decode(&mut cur).unwrap();
                        prop.set_u32(atom, Role::User, prop_i, val)?;
                    }
                    PropertyType::Float32 => {
                        let val = f32::decode(&mut cur).unwrap();
                        prop.set_f32(atom, Role::User, prop_i, val)?;
                    }
                    PropertyType::Str => {
                        let val = String::decode(&mut cur).unwrap();
                        prop.set_str(atom, Role::User, prop_i, val)?;
                    }
                    PropertyType::Enum => {
                        let val = String::decode(&mut cur).unwrap();
                        prop.set_enum(atom, Role::User, prop_i, val)?;
                    }
                    PropertyType::SceneNodeId => {
                        let val = SceneNodeId::decode(&mut cur).unwrap();
                        prop.set_node_id(atom, Role::User, prop_i, val)?;
                    }
                    PropertyType::SExpr => {
                        // Exprs are sent as source strings and compiled here.
                        // The netdebug compiler is const-free: only machine
                        // globals (w, h, ...) are available as variables.
                        let expr_str = String::decode(&mut cur).unwrap();
                        debug!(target: "req", "  compiling expr \"{expr_str}\"");
                        let code = Compiler::new().compile(&expr_str)?;
                        // The property's eval site provides its depends
                        // names plus one of the machine global sets in
                        // use (w, h for most rects, parent_*/rect_* for
                        // edit behaves), so accept the union and treat
                        // anything else as a typo.
                        let mut names: Vec<String> =
                            prop.get_depends().into_iter().map(|d| d.local_name).collect();
                        names.extend(
                            ["w", "h", "parent_w", "parent_h", "rect_w", "rect_h"]
                                .iter()
                                .map(|s| s.to_string()),
                        );
                        check_expr(&code, &names)?;
                        prop.set_expr(atom, Role::User, prop_i, code)?;
                    }
                    PropertyType::VectorShape => {
                        // Vertices carry coordinate exprs as source strings,
                        // compiled with the same const-free compiler. The
                        // payload is: vert count varint; per vert: x expr
                        // string, y expr string, 4x f32 color; index count
                        // varint; u16 indices.
                        let cc = Compiler::new();
                        // Shape verts eval with only the w/h globals.
                        let shape_globals: Vec<String> =
                            ["w", "h"].iter().map(|s| s.to_string()).collect();
                        let vert_count = VarInt::decode(&mut cur)?.0 as usize;
                        let mut verts = vec![];
                        for _ in 0..vert_count {
                            let x_src = String::decode(&mut cur)?;
                            let y_src = String::decode(&mut cur)?;
                            let color = [
                                f32::decode(&mut cur)?,
                                f32::decode(&mut cur)?,
                                f32::decode(&mut cur)?,
                                f32::decode(&mut cur)?,
                            ];
                            let x = cc.compile(&x_src)?;
                            let y = cc.compile(&y_src)?;
                            check_expr(&x, &shape_globals)?;
                            check_expr(&y, &shape_globals)?;
                            verts.push(ShapeVertex::new(x, y, color));
                        }
                        let index_count = VarInt::decode(&mut cur)?.0 as usize;
                        let mut indices = vec![];
                        for _ in 0..index_count {
                            let index = u16::decode(&mut cur)?;
                            if index as usize >= verts.len() {
                                return Err(Error::PropertyWrongIndex)
                            }
                            indices.push(index);
                        }
                        let shape = VectorShape { verts, indices };
                        prop.set_shape(atom, Role::User, prop_i, shape)?;
                    }
                }
            }
            Command::AddNode => {
                let parent_path: ScenePath = String::decode(&mut cur)?.parse()?;
                let node_name = String::decode(&mut cur)?;
                let node_type = SceneNodeType::decode(&mut cur)?;
                debug!(target: "req", "{cmd:?}({parent_path}, {node_name}, {node_type:?})");

                let parent = self.sg_root.lookup_node(parent_path).ok_or(Error::NodeNotFound)?;

                if parent.get_children().iter().any(|c| c.name == node_name) {
                    return Err(Error::NodeSiblingNameConflict)
                }

                let renderer = self.renderer.clone();
                let redraw = self.redraw.clone();
                let node = match node_type {
                    SceneNodeType::Layer => {
                        create_layer(&node_name)
                            .setup(|me| Layer::new(me, renderer.clone(), redraw.clone()))
                            .await
                    }
                    SceneNodeType::VectorArt => {
                        create_vector_art(&node_name)
                            .setup(|me| VectorArt::new(me, renderer.clone(), redraw.clone()))
                            .await
                    }
                    _ => return Err(Error::UnsupportedNodeType),
                };

                // Hold the guard over the link so the triggered pass sees
                // the attached node.
                let _atom = self.redraw.make_guard(gfxtag!("ZeroMQAdapter::AddNode"));
                parent.link(node.clone());
                node.id.encode(&mut reply).unwrap();

                // Arm the pimpl's OnModify handlers (redraw on property
                // change) exactly like window-owned nodes. The task keeps a
                // strong ref so an immediate RemoveNode cannot drop the node
                // out from under start().
                let node2 = node.clone();
                let ex2 = self.ex.clone();
                self.ex
                    .spawn(async move {
                        let obj = get_ui_object_ptr(&node2);
                        obj.start(ex2).await
                    })
                    .detach();
            }
            Command::RemoveNode => {
                let node_path: ScenePath = String::decode(&mut cur)?.parse()?;
                debug!(target: "req", "{cmd:?}({node_path})");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                // The scene root has no parent, removal is meaningless.
                if Arc::ptr_eq(&node, &self.sg_root) {
                    return Err(Error::NodeNotRemovable)
                }

                // Tear down the subtree's UI tasks and buffers before
                // unlinking, mirroring Window::stop(). Pimpl types without
                // a UIObject mapping (Window, Setting, plugins, ...) keep
                // their tasks until process exit.
                stop_ui_subtree(&node);

                node.unlink();
                self.redraw.trigger();
            }
            Command::RenameNode => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let node_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {})", cmd, node_id, node_name);
                scene_graph.rename_node(node_id, node_name)?;
                */
            }
            Command::ScanDangling => {
                /*
                let dangling = scene_graph.scan_dangling();
                dangling.encode(&mut reply).unwrap();
                */
            }
            Command::LookupNodeId => {
                /*
                let node_path: String = deserialize(&payload).unwrap();
                debug!(target: "req", "{:?}({})", cmd, node_path);
                let node_id = scene_graph.lookup_node_id(&node_path).ok_or(Error::NodeNotFound)?;
                node_id.encode(&mut reply).unwrap();
                */
            }
            Command::AddProperty => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let prop_name = String::decode(&mut cur).unwrap();
                let prop_type = PropertyType::decode(&mut cur).unwrap();
                let prop_subtype = PropertySubType::decode(&mut cur).unwrap();

                debug!(target: "req", "{:?}({}, {}, {:?}, {:?}, ...)", cmd, node_id, prop_name, prop_type, prop_subtype);
                let mut prop = Property::new(prop_name, prop_type, prop_subtype);

                let prop_array_len = u32::decode(&mut cur).unwrap();
                prop.set_array_len(prop_array_len as usize);

                let prop_defaults_is_some = bool::decode(&mut cur).unwrap();
                if prop_defaults_is_some {
                    let prop_defaults_len = VarInt::decode(&mut cur).unwrap();
                    match prop_type {
                        PropertyType::Uint32 => {
                            let mut prop_defaults = vec![];
                            for _ in 0..prop_defaults_len.0 {
                                prop_defaults.push(u32::decode(&mut cur).unwrap());
                            }
                            prop.set_defaults_u32(prop_defaults)?;
                        }
                        PropertyType::Float32 => {
                            let mut prop_defaults = vec![];
                            for _ in 0..prop_defaults_len.0 {
                                prop_defaults.push(f32::decode(&mut cur).unwrap());
                            }
                            prop.set_defaults_f32(prop_defaults)?;
                        }
                        PropertyType::Str => {
                            let mut prop_defaults = vec![];
                            for _ in 0..prop_defaults_len.0 {
                                prop_defaults.push(String::decode(&mut cur).unwrap());
                            }
                            prop.set_defaults_str(prop_defaults)?;
                        }
                        _ => return Err(Error::PropertyWrongType),
                    }
                }

                let prop_ui_name = String::decode(&mut cur).unwrap();
                let prop_desc = String::decode(&mut cur).unwrap();
                let prop_is_null_allowed = bool::decode(&mut cur).unwrap();
                let prop_is_expr_allowed = bool::decode(&mut cur).unwrap();

                match prop_type {
                    PropertyType::Uint32 => {
                        let min_is_some = bool::decode(&mut cur).unwrap();
                        let min = if min_is_some {
                            let min = u32::decode(&mut cur).unwrap();
                            Some(PropertyValue::Uint32(min))
                        } else {
                            None
                        };
                        let max_is_some = bool::decode(&mut cur).unwrap();
                        let max = if max_is_some {
                            let max = u32::decode(&mut cur).unwrap();
                            Some(PropertyValue::Uint32(max))
                        } else {
                            None
                        };
                        prop.min_val = min;
                        prop.max_val = max;
                    }
                    PropertyType::Float32 => {
                        let min_is_some = bool::decode(&mut cur).unwrap();
                        let min = if min_is_some {
                            let min = f32::decode(&mut cur).unwrap();
                            Some(PropertyValue::Float32(min))
                        } else {
                            None
                        };
                        let max_is_some = bool::decode(&mut cur).unwrap();
                        let max = if max_is_some {
                            let max = f32::decode(&mut cur).unwrap();
                            Some(PropertyValue::Float32(max))
                        } else {
                            None
                        };
                        prop.min_val = min;
                        prop.max_val = max;
                    }
                    _ => {
                        let min_is_some = bool::decode(&mut cur).unwrap();
                        if min_is_some {
                            return Err(Error::PropertyWrongType)
                        }
                        let max_is_some = bool::decode(&mut cur).unwrap();
                        if max_is_some {
                            return Err(Error::PropertyWrongType)
                        }
                    }
                }

                let prop_enum_items = Vec::<String>::decode(&mut cur).unwrap();

                let node = scene_graph.get_node_mut(node_id).ok_or(Error::NodeNotFound)?;

                prop.set_ui_text(prop_ui_name, prop_desc);
                prop.is_null_allowed = prop_is_null_allowed;
                prop.is_expr_allowed = prop_is_expr_allowed;
                if !prop_enum_items.is_empty() {
                    prop.set_enum_items(prop_enum_items)?;
                }
                node.add_property(prop)?;
                */
            }
            Command::LinkNode => {
                /*
                let child_id = SceneNodeId::decode(&mut cur).unwrap();
                let parent_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {})", cmd, child_id, parent_id);
                scene_graph.link(child_id, parent_id)?;
                */
            }
            Command::UnlinkNode => {
                /*
                let child_id = SceneNodeId::decode(&mut cur).unwrap();
                let parent_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {})", cmd, child_id, parent_id);
                scene_graph.unlink(child_id, parent_id)?;
                */
            }
            Command::GetSignals => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                debug!(target: "req", "{cmd:?}({node_path})");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                let sigs = node.sigs.read().unwrap();
                let sig_names: Vec<_> = sigs.iter().map(|sig| sig.name.clone()).collect();
                sig_names.encode(&mut reply).unwrap();
            }
            Command::RegisterSlot => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let sig_name = String::decode(&mut cur).unwrap();
                let slot_name = String::decode(&mut cur).unwrap();
                let user_data = Vec::<u8>::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {sig_name}, {slot_name}, {user_data:?})");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                let (sendr, recvr) = async_channel::unbounded();
                let slot = Slot { name: slot_name, notify: sendr };

                // This task will auto-die when the slot is unregistered
                let self2 = self.clone();
                self.ex
                    .spawn(async move {
                        loop {
                            let Ok(signal_data) = recvr.recv().await else {
                                // Die
                                break
                            };

                            let mut m = zeromq::ZmqMessage::from(signal_data);
                            m.push_back(user_data.clone().into());

                            self2.zmq_pub.lock().await.send(m).await.unwrap();
                        }
                    })
                    .detach();

                let slot_id = node.register(&sig_name, slot)?;
                slot_id.encode(&mut reply).unwrap();
            }
            Command::UnregisterSlot => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let sig_name = String::decode(&mut cur).unwrap();
                let slot_id = SlotId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {}, {})", cmd, node_id, sig_name, slot_id);

                let node = scene_graph.get_node_mut(node_id).ok_or(Error::NodeNotFound)?;
                node.unregister(&sig_name, slot_id)?;
                */
            }
            Command::LookupSlotId => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let sig_name = String::decode(&mut cur).unwrap();
                let slot_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {}, {})", cmd, node_id, sig_name, slot_name);

                let node = scene_graph.get_node(node_id).ok_or(Error::NodeNotFound)?;
                let signal = node.get_signal(&sig_name).ok_or(Error::SignalNotFound)?;
                let slot_id = signal.lookup_slot_id(&slot_name).ok_or(Error::SlotNotFound)?;
                slot_id.encode(&mut reply).unwrap();
                */
            }
            Command::GetSlots => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let sig_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {sig_name})");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let signal = node.get_signal(&sig_name).ok_or(Error::SignalNotFound)?;

                let slots = signal.get_slots();
                let slot_names: Vec<_> =
                    slots.iter().map(|(slot_id, slot)| (slot.name.clone(), *slot_id)).collect();
                slot_names.encode(&mut reply).unwrap();
            }
            Command::GetMethods => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                debug!(target: "req", "{cmd:?}({node_path})");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let method_names: Vec<_> = node.methods.iter().map(|m| m.name.clone()).collect();
                method_names.encode(&mut reply).unwrap();
            }
            Command::GetMethod => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let method_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {})", cmd, node_path, method_name);

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let method = node.get_method(&method_name).ok_or(Error::MethodNotFound)?;

                method.args.encode(&mut reply).unwrap();
                method.result.encode(&mut reply).unwrap();
            }
            Command::CallMethod => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let method_name = String::decode(&mut cur).unwrap();
                let arg_data = Vec::<u8>::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {method_name}, ...)");

                let node = self.sg_root.lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let result = node.call_method(&method_name, arg_data).await?;
                result.encode(&mut reply).unwrap();
            }
        }

        Ok(reply)
    }
}
