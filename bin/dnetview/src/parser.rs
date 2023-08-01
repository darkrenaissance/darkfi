/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::collections::hash_map::Entry;

use async_std::sync::Arc;
use log::{debug, error, info};
use serde_json::Value;
use smol::Executor;
use url::Url;

use darkfi::util::{async_util};

use crate::{
    config::{DnvConfig, Node, NodeType},
    error::{DnetViewError, DnetViewResult},
    model::{
        ConnectInfo, LilithInfo, Model, NetworkInfo, NodeInfo, SelectableObject, Session,
        SessionInfo,
    },
    rpc::RpcConnect,
    util::{is_empty_session, make_connect_id, make_empty_id, make_node_id, make_session_id},
};

pub struct DataParser {
    model: Arc<Model>,
    config: DnvConfig,
}

impl DataParser {
    pub fn new(model: Arc<Model>, config: DnvConfig) -> Arc<Self> {
        Arc::new(Self { model, config })
    }

    pub async fn start_connect_slots(self: Arc<Self>, ex: Arc<Executor<'_>>) -> DnetViewResult<()> {
        debug!(target: "dnetview", "start_connect_slots() START");
        for node in &self.config.nodes {
            debug!(target: "dnetview", "attempting to spawn...");
            ex.clone().spawn(self.clone().try_connect(node.clone())).detach();
        }
        Ok(())
    }

    async fn try_connect(self: Arc<Self>, node: Node) -> DnetViewResult<()> {
        debug!(target: "dnetview", "try_connect() START");
        loop {
            info!("Attempting to poll {}, RPC URL: {}", node.name, node.rpc_url);
            // Parse node config and execute poll.
            // On any failure, sleep and retry.
            match RpcConnect::new(Url::parse(&node.rpc_url)?, node.name.clone()).await {
                Ok(client) => {
                    if let Err(e) = self.poll(&node, client).await {
                        error!("Poll execution error: {:?}", e);
                    }
                }
                Err(e) => {
                    error!("RPC client creation error: {:?}", e);
                }
            }
            self.parse_offline(node.name.clone()).await?;
            async_util::sleep(2000).await;
        }
    }

    async fn poll(&self, node: &Node, client: RpcConnect) -> DnetViewResult<()> {
        loop {
            // Ping the node to verify if its online.
            if let Err(e) = client.ping().await {
                return Err(DnetViewError::Darkfi(e))
            }

            // Retrieve node info, based on its type
            let response = match &node.node_type {
                NodeType::LILITH => client.lilith_spawns().await,
                NodeType::NORMAL => client.get_info().await,
                NodeType::CONSENSUS => client.get_consensus_info().await,
            };

            // Parse response
            match response {
                Ok(reply) => {
                    if reply.as_object().is_none() || reply.as_object().unwrap().is_empty() {
                        return Err(DnetViewError::EmptyRpcReply)
                    }

                    match &node.node_type {
                        NodeType::LILITH => {
                            self.parse_lilith_data(
                                reply.as_object().unwrap().clone(),
                                node.name.clone(),
                            )
                            .await?
                        }
                        _ => self.parse_data(reply.as_object().unwrap(), node.name.clone()).await?,
                    };
                }
                Err(e) => return Err(e),
            }

            // Sleep until next poll
            async_util::sleep(2000).await;
        }
    }

    async fn parse_offline(&self, node_name: String) -> DnetViewResult<()> {
        let name = "Offline".to_string();
        let session_type = Session::Offline;
        let node_id = make_node_id(&node_name)?;
        let session_id = make_session_id(&node_id, &session_type)?;
        let mut connects: Vec<ConnectInfo> = Vec::new();
        let mut sessions: Vec<SessionInfo> = Vec::new();

        // initialize with empty values
        let id = make_empty_id(&node_id, &session_type, 0)?;
        let addr = "Null".to_string();
        let state = "Null".to_string();
        let parent = node_id.clone();
        let msg_log = Vec::new();
        let is_empty = true;
        let last_msg = "Null".to_string();
        let last_status = "Null".to_string();
        let remote_node_id = "Null".to_string();
        let connect_info = ConnectInfo::new(
            id,
            addr,
            state.clone(),
            parent.clone(),
            msg_log,
            is_empty,
            last_msg,
            last_status,
            remote_node_id,
        );
        connects.push(connect_info.clone());

        let accept_addr = None;
        let session_info =
            SessionInfo::new(session_id, name, is_empty, parent, connects, accept_addr, None);
        sessions.push(session_info);

        let node = NodeInfo::new(node_id, node_name, state, sessions.clone(), None, true);

        self.update_selectables(sessions, node).await?;
        Ok(())
    }

    async fn parse_data(
        &self,
        reply: &serde_json::Map<String, Value>,
        node_name: String,
    ) -> DnetViewResult<()> {
        //let addr = &reply.get("addr");
        let inbound = &reply["inbound"];
        //let _manual = &reply["session_manual"];
        let outbound = &reply["outbound"];
        let state = String::new();

        let mut sessions: Vec<SessionInfo> = Vec::new();

        let node_id = make_node_id(&node_name)?;

        //let ext_addr = self.parse_external_addr(addr).await?;
        let ext_addr = None;

        let in_session = self.parse_inbound(inbound, &node_id).await?;
        let out_session = self.parse_outbound(outbound, &node_id).await?;
        //let man_session = self.parse_manual(manual, &node_id).await?;

        sessions.push(in_session.clone());
        sessions.push(out_session.clone());
        //sessions.push(man_session.clone());

        let node = NodeInfo::new(
            node_id,
            node_name,
            state,
            //state.as_str().unwrap().to_string(),
            sessions.clone(),
            ext_addr,
            false,
        );

        self.update_selectables(sessions.clone(), node).await?;
        self.update_msgs(sessions).await?;

        //debug!("IDS: {:?}", self.model.ids.lock().await);
        //debug!("INFOS: {:?}", self.model.nodes.lock().await);

        Ok(())
    }

    async fn parse_lilith_data(
        &self,
        reply: serde_json::Map<String, Value>,
        name: String,
    ) -> DnetViewResult<()> {
        let urls: Vec<String> = serde_json::from_value(reply.get("urls").unwrap().clone()).unwrap();
        let spawns: Vec<serde_json::Map<String, Value>> =
            serde_json::from_value(reply.get("spawns").unwrap().clone()).unwrap();

        let mut networks = vec![];
        for spawn in spawns {
            let name = spawn.get("name").unwrap().as_str().unwrap().to_string();
            let id = make_node_id(&name)?;
            let urls: Vec<String> =
                serde_json::from_value(spawn.get("urls").unwrap().clone()).unwrap();
            let nodes: Vec<String> =
                serde_json::from_value(spawn.get("hosts").unwrap().clone()).unwrap();
            let network = NetworkInfo::new(id, name, urls, nodes);
            networks.push(network);
        }
        let id = make_node_id(&name)?;
        let lilith = LilithInfo::new(id.clone(), name, urls, networks);
        let lilith_obj = SelectableObject::Lilith(lilith.clone());

        self.model.selectables.lock().await.insert(id, lilith_obj);
        for network in lilith.networks {
            let network_obj = SelectableObject::Network(network.clone());
            self.model.selectables.lock().await.insert(network.id, network_obj);
        }

        Ok(())
    }

    async fn update_msgs(&self, sessions: Vec<SessionInfo>) -> DnetViewResult<()> {
        for session in sessions {
            for connection in session.children {
                if !self.model.msg_map.lock().await.contains_key(&connection.id) {
                    // we don't have this ID: it is a new node
                    self.model
                        .msg_map
                        .lock()
                        .await
                        .insert(connection.id, connection.msg_log.clone());
                } else {
                    // we have this id: append the msg values
                    match self.model.msg_map.lock().await.entry(connection.id) {
                        Entry::Vacant(e) => {
                            e.insert(connection.msg_log);
                        }
                        Entry::Occupied(mut e) => {
                            for msg in connection.msg_log {
                                e.get_mut().push(msg);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn update_selectables(
        &self,
        sessions: Vec<SessionInfo>,
        node: NodeInfo,
    ) -> DnetViewResult<()> {
        if node.is_offline {
            let node_obj = SelectableObject::Node(node.clone());
            self.model.selectables.lock().await.insert(node.id.clone(), node_obj.clone());
        } else {
            let node_obj = SelectableObject::Node(node.clone());
            self.model.selectables.lock().await.insert(node.id.clone(), node_obj.clone());
            for session in sessions {
                if !session.is_empty {
                    let session_obj = SelectableObject::Session(session.clone());
                    self.model
                        .selectables
                        .lock()
                        .await
                        .insert(session.clone().id, session_obj.clone());
                    for connect in session.children {
                        let connect_obj = SelectableObject::Connect(connect.clone());
                        self.model
                            .selectables
                            .lock()
                            .await
                            .insert(connect.clone().id, connect_obj.clone());
                    }
                }
            }
        }
        Ok(())
    }

    async fn _parse_external_addr(&self, addr: &Option<&Value>) -> DnetViewResult<Option<String>> {
        match addr {
            Some(addr) => match addr.as_str() {
                Some(addr) => Ok(Some(addr.to_string())),
                None => Ok(None),
            },
            None => Err(DnetViewError::NoExternalAddr),
        }
    }

    async fn parse_inbound(
        &self,
        inbound: &Value,
        node_id: &String,
    ) -> DnetViewResult<SessionInfo> {
        let name = "Inbound".to_string();
        let session_type = Session::Inbound;
        let parent = node_id.to_string();
        let id = make_session_id(&parent, &session_type)?;
        let mut connects: Vec<ConnectInfo> = Vec::new();
        let connections = &inbound["connected"];
        let mut connect_count = 0;
        let mut accept_vec = Vec::new();

        match connections.as_object() {
            Some(connect) => {
                match connect.is_empty() {
                    true => {
                        connect_count += 1;
                        // channel is empty. initialize with empty values
                        let id = make_empty_id(node_id, &session_type, connect_count)?;
                        let addr = "Null".to_string();
                        let state = "Null".to_string();
                        let parent = parent.clone();
                        let msg_log = Vec::new();
                        let is_empty = true;
                        let last_msg = "Null".to_string();
                        let last_status = "Null".to_string();
                        let remote_node_id = "Null".to_string();
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                            remote_node_id,
                        );
                        connects.push(connect_info);
                    }
                    false => {
                        // channel is not empty. initialize with whole values
                        for k in connect.keys() {
                            let node = connect.get(k);
                            let addr = k.to_string();
                            let info = node.unwrap().as_array();
                            // get the accept address
                            let accept_addr = info.unwrap().get(0);
                            let acc_addr = accept_addr
                                .unwrap()
                                .get("accept_addr")
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .to_string();
                            accept_vec.push(acc_addr);
                            let info2 = info.unwrap().get(1);
                            let id = info2.unwrap().get("random_id").unwrap().as_u64().unwrap();
                            let id = make_connect_id(&id)?;
                            let state = "state".to_string();
                            let parent = parent.clone();

                            // Empty message log for now.
                            let msg_log = Vec::new();

                            let is_empty = false;
                            let last_msg = info2
                                .unwrap()
                                .get("last_msg")
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .to_string();
                            let last_status = info2
                                .unwrap()
                                .get("last_status")
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .to_string();
                            let remote_node_id = info2
                                .unwrap()
                                .get("remote_node_id")
                                .unwrap()
                                .as_str()
                                .unwrap()
                                .to_string();
                            let r_node_id: String = match remote_node_id.is_empty() {
                                true => "no remote id".to_string(),
                                false => remote_node_id,
                            };
                            let connect_info = ConnectInfo::new(
                                id,
                                addr,
                                state,
                                parent,
                                msg_log,
                                is_empty,
                                last_msg,
                                last_status,
                                r_node_id,
                            );
                            connects.push(connect_info.clone());
                        }
                    }
                }
                let is_empty = is_empty_session(&connects);

                // TODO: clean this up
                if accept_vec.is_empty() {
                    let accept_addr = None;
                    let session_info =
                        SessionInfo::new(id, name, is_empty, parent, connects, accept_addr, None);
                    Ok(session_info)
                } else {
                    let accept_addr = Some(accept_vec[0].clone());
                    let session_info =
                        SessionInfo::new(id, name, is_empty, parent, connects, accept_addr, None);
                    Ok(session_info)
                }
            }
            None => {
                Err(DnetViewError::ValueIsNotObject)
            }
        }
    }

    // TODO: placeholder for now
    async fn _parse_manual(
        &self,
        _manual: &Value,
        node_id: &String,
    ) -> DnetViewResult<SessionInfo> {
        let name = "Manual".to_string();
        let session_type = Session::Manual;
        let mut connects: Vec<ConnectInfo> = Vec::new();
        let parent = node_id.to_string();

        let session_id = make_session_id(&parent, &session_type)?;
        //let id: u64 = 0;
        let connect_id = make_empty_id(node_id, &session_type, 0)?;
        //let connect_id = make_connect_id(&id)?;
        let addr = "Null".to_string();
        let state = "Null".to_string();
        let msg_log = Vec::new();
        let is_empty = true;
        let msg = "Null".to_string();
        let status = "Null".to_string();
        let remote_node_id = "Null".to_string();
        let connect_info = ConnectInfo::new(
            connect_id.clone(),
            addr,
            state,
            parent,
            msg_log,
            is_empty,
            msg,
            status,
            remote_node_id,
        );
        connects.push(connect_info);
        let parent = connect_id;
        let is_empty = is_empty_session(&connects);
        let accept_addr = None;
        let session_info = SessionInfo::new(
            session_id,
            name,
            is_empty,
            parent,
            connects.clone(),
            accept_addr,
            None,
        );

        Ok(session_info)
    }

    async fn parse_outbound(
        &self,
        outbound: &Value,
        node_id: &String,
    ) -> DnetViewResult<SessionInfo> {
        let name = "Outbound".to_string();
        let session_type = Session::Outbound;
        let parent = node_id.to_string();
        let id = make_session_id(&parent, &session_type)?;
        let mut connects: Vec<ConnectInfo> = Vec::new();
        let slots = &outbound["slots"];
        let mut slot_count = 0;

        let hosts = &outbound["hosts"];

        match slots.as_array() {
            Some(slots) => {
                for slot in slots {
                    slot_count += 1;
                    match slot["channel"].is_null() {
                        true => {
                            // TODO: this is not actually empty
                            let id = make_empty_id(node_id, &session_type, slot_count)?;
                            let addr = "Null".to_string();
                            let state = &slot["state"];
                            let state = state.as_str().unwrap().to_string();
                            let parent = parent.clone();
                            let msg_log = Vec::new();
                            let is_empty = false;
                            let last_msg = "Null".to_string();
                            let last_status = "Null".to_string();
                            let remote_node_id = "Null".to_string();
                            let connect_info = ConnectInfo::new(
                                id,
                                addr,
                                state,
                                parent,
                                msg_log,
                                is_empty,
                                last_msg,
                                last_status,
                                remote_node_id,
                            );
                            connects.push(connect_info.clone());
                        }
                        false => {
                            // channel is not empty. initialize with whole values
                            let channel = &slot["channel"];
                            let id = channel["random_id"].as_u64().unwrap();
                            let id = make_connect_id(&id)?;
                            let addr = &slot["addr"];
                            let addr = addr.as_str().unwrap().to_string();
                            let state = &slot["state"];
                            let state = state.as_str().unwrap().to_string();
                            let parent = parent.clone();

                            // Empty message log for now.
                            let msg_log = Vec::new();

                            let is_empty = false;
                            let last_msg = channel["last_msg"].as_str().unwrap().to_string();
                            let last_status = channel["last_status"].as_str().unwrap().to_string();
                            let remote_node_id =
                                channel["remote_node_id"].as_str().unwrap().to_string();
                            let r_node_id: String = match remote_node_id.is_empty() {
                                true => "no remote id".to_string(),
                                false => remote_node_id,
                            };
                            let connect_info = ConnectInfo::new(
                                id,
                                addr,
                                state,
                                parent,
                                msg_log,
                                is_empty,
                                last_msg,
                                last_status,
                                r_node_id,
                            );
                            connects.push(connect_info.clone());
                        }
                    }
                }

                let is_empty = is_empty_session(&connects);

                let accept_addr = None;

                match hosts.as_array() {
                    Some(hosts) => {
                        let hosts: Vec<String> =
                            hosts.iter().map(|addr| addr.as_str().unwrap().to_string()).collect();
                        let session_info = SessionInfo::new(
                            id,
                            name,
                            is_empty,
                            parent,
                            connects,
                            accept_addr,
                            Some(hosts),
                        );
                        Ok(session_info)
                    }
                    None => Err(DnetViewError::ValueIsNotObject),
                }
            }
            None => Err(DnetViewError::ValueIsNotObject),
        }
    }
}
