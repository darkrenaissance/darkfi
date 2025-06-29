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

use async_lock::Mutex;
use darkfi_serial::{async_trait, deserialize, Decodable, Encodable, SerialDecodable, VarInt};
use std::{io::Cursor, sync::Arc};
use zeromq::{Socket, SocketRecv, SocketSend};

use crate::{
    error::{Error, Result},
    expr::SExprCode,
    prop::{PropertyAtomicGuard, PropertyType, Role},
    scene::{SceneNodeId, SceneNodePtr, ScenePath},
    ExecutorPtr,
};

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
    ex: ExecutorPtr,

    zmq_rep: Mutex<zeromq::RepSocket>,
    zmq_pub: Mutex<zeromq::PubSocket>,
}

impl ZeroMQAdapter {
    pub async fn new(sg_root: SceneNodePtr, ex: ExecutorPtr) -> Arc<Self> {
        let mut zmq_rep = zeromq::RepSocket::new();
        zmq_rep.bind("tcp://0.0.0.0:9484").await.unwrap();

        let mut zmq_pub = zeromq::PubSocket::new();
        zmq_pub.bind("tcp://0.0.0.0:9485").await.unwrap();

        Arc::new(Self { sg_root, ex, zmq_rep: Mutex::new(zmq_rep), zmq_pub: Mutex::new(zmq_pub) })
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
                let node =
                    self.sg_root.clone().lookup_node(node_path).ok_or(Error::NodeNotFound)?;

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
                let node =
                    self.sg_root.clone().lookup_node(node_path).ok_or(Error::NodeNotFound)?;

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
                let node =
                    self.sg_root.clone().lookup_node(node_path).ok_or(Error::NodeNotFound)?;

                let prop = node.get_property(&prop_name).ok_or(Error::PropertyNotFound)?;
                prop.typ.encode(&mut reply).unwrap();
                VarInt(prop.get_len() as u64).encode(&mut reply).unwrap();
                for i in 0..prop.get_len() {
                    let val = prop.get_value(i)?;
                    if val.is_unset() {
                        1u8.encode(&mut reply).unwrap();
                        let default = &prop.defaults[i];
                        default.encode(&mut reply).unwrap();
                    } else if val.is_null() {
                        2u8.encode(&mut reply).unwrap();
                    } else if val.is_expr() {
                        3u8.encode(&mut reply).unwrap();
                    } else {
                        0u8.encode(&mut reply).unwrap();
                        val.encode(&mut reply).unwrap();
                    }
                }
            }
            Command::SetPropertyValue => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let prop_name = String::decode(&mut cur).unwrap();
                let prop_i = u32::decode(&mut cur).unwrap() as usize;
                let prop_type = PropertyType::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {prop_name}, {prop_i}, {prop_type:?})");

                let node =
                    self.sg_root.clone().lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let prop = node.get_property(&prop_name).ok_or(Error::PropertyNotFound)?;

                let atom = &mut PropertyAtomicGuard::new();

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
                        let val = SExprCode::decode(&mut cur).unwrap();
                        debug!(target: "req", "  received code {:?}", val);
                        prop.set_expr(atom, Role::User, prop_i, val)?;
                    }
                }
            }
            Command::AddNode => {
                /*
                let node_name = String::decode(&mut cur).unwrap();
                let node_type = SceneNodeType::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {:?})", cmd, node_name, node_type);

                let node_id = scene_graph.add_node(&node_name, node_type).id;
                node_id.encode(&mut reply).unwrap();
                */
            }
            Command::RemoveNode => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({})", cmd, node_id);
                scene_graph.remove_node(node_id)?;
                */
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
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({})", cmd, node_id);

                let node = scene_graph.get_node_mut(node_id).ok_or(Error::NodeNotFound)?;

                let mut sigs = vec![];
                for sig in &node.sigs {
                    sigs.push(sig.name.clone());
                }
                sigs.encode(&mut reply).unwrap();
                */
            }
            Command::RegisterSlot => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let sig_name = String::decode(&mut cur).unwrap();
                let slot_name = String::decode(&mut cur).unwrap();
                let user_data = Vec::<u8>::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {}, {}, {:?})", cmd, node_id, sig_name, slot_name, user_data);

                let node = scene_graph.get_node_mut(node_id).ok_or(Error::NodeNotFound)?;

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
                */
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
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let sig_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {})", cmd, node_id, sig_name);

                let node = scene_graph.get_node(node_id).ok_or(Error::NodeNotFound)?;
                let signal = node.get_signal(&sig_name).ok_or(Error::SignalNotFound)?;

                let mut slots = vec![];
                for (slot_id, slot) in signal.get_slots() {
                    slots.push((slot.name.clone(), slot_id));
                }
                slots.encode(&mut reply).unwrap();
                */
            }
            Command::GetMethods => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({})", cmd, node_id);

                let node = scene_graph.get_node(node_id).ok_or(Error::NodeNotFound)?;
                let method_names: Vec<_> = node.methods.iter().map(|m| m.name.clone()).collect();

                method_names.encode(&mut reply).unwrap();
                */
            }
            Command::GetMethod => {
                /*
                let node_id = SceneNodeId::decode(&mut cur).unwrap();
                let method_name = String::decode(&mut cur).unwrap();
                debug!(target: "req", "{:?}({}, {})", cmd, node_id, method_name);

                let node = scene_graph.get_node(node_id).ok_or(Error::NodeNotFound)?;
                let method = node.get_method(&method_name).ok_or(Error::MethodNotFound)?;

                method.args.encode(&mut reply).unwrap();
                method.result.encode(&mut reply).unwrap();
                */
            }
            Command::CallMethod => {
                let node_path: ScenePath = String::decode(&mut cur).unwrap().parse()?;
                let method_name = String::decode(&mut cur).unwrap();
                let arg_data = Vec::<u8>::decode(&mut cur).unwrap();
                debug!(target: "req", "{cmd:?}({node_path}, {method_name}, ...)");

                let node =
                    self.sg_root.clone().lookup_node(node_path).ok_or(Error::NodeNotFound)?;
                let result = node.call_method(&method_name, arg_data).await?;
                result.encode(&mut reply).unwrap();
            }
        }

        Ok(reply)
    }
}
