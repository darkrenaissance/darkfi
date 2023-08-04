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

use darkfi::util::{async_util, time::NanoTimestamp};

use crate::{
    config::{DnvConfig, Node, NodeType},
    error::{DnetViewError, DnetViewResult},
    model::{
        LilithInfo, Model, NetworkInfo, NodeInfo, SelectableObject, Session, SessionInfo, SlotInfo,
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
                NodeType::NORMAL => client.dnet_info().await,
                NodeType::CONSENSUS => client.get_consensus_info().await,
            };

            // Parse response
            match response {
                Ok(reply) => {
                    debug!("dnetview:: reply {:?}", reply);
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

    // If poll times out, inititalize data structures with empty values.
    async fn parse_offline(&self, name: String) -> DnetViewResult<()> {
        let name = "Offline".to_string();
        let session_type = Session::Offline;

        let mut slots: Vec<SlotInfo> = Vec::new();
        let mut info: Vec<SessionInfo> = Vec::new();
        let hosts = Vec::new();

        let node_id = make_node_id(&name)?;
        let dnet_id = make_empty_id(&node_id, &session_type, 0)?;
        let addr = "Null".to_string();
        let state = "Null".to_string();
        let random_id = "Null".to_string();
        let remote_id = "Null".to_string();
        let log = Vec::new();
        let is_empty = true;

        let slot = SlotInfo::new(
            dnet_id.clone(),
            node_id.clone(),
            addr.clone(),
            random_id,
            remote_id,
            log,
            is_empty,
        );
        slots.push(slot.clone());

        let session_info = SessionInfo::new(
            dnet_id,
            node_id.clone(),
            name.clone(),
            addr.clone(),
            state,
            slots,
            is_empty,
        );
        info.push(session_info);

        let node = NodeInfo::new(node_id.clone(), name.clone(), hosts, info.clone(), is_empty);

        self.update_selectables(info, node).await?;
        Ok(())
    }

    async fn parse_data(
        &self,
        reply: &serde_json::Map<String, Value>,
        name: String,
    ) -> DnetViewResult<()> {
        let hosts = &reply["hosts"];
        let inbound = &reply["inbound"];
        let outbound = &reply["outbound"];

        let dnet_id = make_node_id(&name)?;

        let hosts = self.parse_hosts(hosts).await?;
        let inbound = self.parse_inbound(inbound, &dnet_id).await?;
        let outbound = self.parse_outbound(outbound, &dnet_id).await?;

        let mut info: Vec<SessionInfo> = Vec::new();
        info.push(inbound.clone());
        info.push(outbound.clone());

        let node = NodeInfo::new(dnet_id, name, hosts, info.clone(), false);

        self.update_selectables(info.clone(), node).await?;
        self.update_msgs(info.clone()).await?;

        Ok(())
    }

    async fn parse_lilith_data(
        &self,
        reply: serde_json::Map<String, Value>,
        name: String,
    ) -> DnetViewResult<()> {
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
        let lilith = LilithInfo::new(id.clone(), name, networks);
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
            for connection in session.info {
                if !self.model.msg_map.lock().await.contains_key(&connection.dnet_id) {
                    // we don't have this ID: it is a new node
                    self.model
                        .msg_map
                        .lock()
                        .await
                        .insert(connection.dnet_id, connection.log.clone());
                } else {
                    // we have this id: append the msg values
                    match self.model.msg_map.lock().await.entry(connection.dnet_id) {
                        Entry::Vacant(e) => {
                            e.insert(connection.log);
                        }
                        Entry::Occupied(mut e) => {
                            for msg in connection.log {
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
            self.model.selectables.lock().await.insert(node.dnet_id.clone(), node_obj.clone());
        } else {
            let node_obj = SelectableObject::Node(node.clone());
            self.model.selectables.lock().await.insert(node.dnet_id.clone(), node_obj.clone());
            for session in sessions {
                if !session.is_empty {
                    let session_obj = SelectableObject::Session(session.clone());
                    self.model
                        .selectables
                        .lock()
                        .await
                        .insert(session.clone().dnet_id, session_obj.clone());
                    for connect in session.info {
                        let connect_obj = SelectableObject::Connect(connect.clone());
                        self.model
                            .selectables
                            .lock()
                            .await
                            .insert(connect.clone().dnet_id, connect_obj.clone());
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

    async fn parse_inbound(&self, reply: &Value, node_id: &String) -> DnetViewResult<SessionInfo> {
        let name = "Inbound".to_string();
        let session_type = Session::Inbound;
        let dnet_id = make_session_id(node_id, &session_type)?;
        let mut info: Vec<SlotInfo> = Vec::new();

        // TODO: improve this ugly hack.
        let slot_count = 0;

        // Dnetview is not enabled.
        if reply.is_null() {
            let dnet_id = make_empty_id(node_id, &session_type, slot_count)?;
            let node_id = node_id.to_string();
            let addr = "Null".to_string();
            let random_id = "Null".to_string();
            let remote_id = "Null".to_string();
            let log = Vec::new();
            let is_empty = true;

            let slot = SlotInfo::new(
                dnet_id.clone(),
                node_id.clone(),
                addr,
                random_id,
                remote_id,
                log,
                is_empty,
            );
            info.push(slot);
            // Check whether the session is empty.
            let is_empty = is_empty_session(&info);

            let addr = "Null".to_string();
            let state = "Null".to_string();
            let session = SessionInfo::new(
                dnet_id.clone(),
                node_id.clone(),
                name,
                addr,
                state,
                info,
                is_empty,
            );

            return Ok(session)
        }

        match reply.get("inbound") {
            Some(session) => {
                let inbound: Vec<serde_json::Map<String, Value>> =
                    serde_json::from_value(session.clone()).unwrap();
                for slot in inbound {
                    let addr = slot.get("addr").unwrap().as_str().unwrap().to_string();
                    let slot_info: serde_json::Map<String, Value> =
                        serde_json::from_value(slot.get("info").unwrap().clone()).unwrap();

                    let slot_addr = slot_info.get("addr").unwrap().as_str().unwrap().to_string();
                    let random_id =
                        slot_info.get("random_id").unwrap().as_str().unwrap().to_string();
                    let remote_id =
                        slot_info.get("remote_id").unwrap().as_str().unwrap().to_string();

                    let log: Vec<(NanoTimestamp, String, String)> =
                        serde_json::from_value(slot_info.get("log").unwrap().clone()).unwrap();
                    let node_id = node_id.to_string();
                    let is_empty = false;

                    let slot = SlotInfo::new(
                        dnet_id.clone(),
                        node_id.clone(),
                        slot_addr,
                        random_id,
                        remote_id,
                        log,
                        is_empty,
                    );
                    info.push(slot);
                }

                // TODO: fixme
                let addr = String::new();
                // TODO: this should be an option
                let state = String::new();
                let node_id = node_id.to_string();
                let is_empty = false;
                let session =
                    SessionInfo::new(dnet_id, node_id.clone(), name, addr, state, info, is_empty);
                Ok(session)
            }
            None => {
                // Empty data. Initialize empty values.
                // TODO: clean up empty info boilerplate.
                let dnet_id = make_empty_id(node_id, &session_type, slot_count)?;
                let node_id = node_id.to_string();
                let addr = "Null".to_string();
                let random_id = "Null".to_string();
                let remote_id = "Null".to_string();
                let log = Vec::new();
                let is_empty = true;

                let slot = SlotInfo::new(
                    dnet_id.clone(),
                    node_id.clone(),
                    addr,
                    random_id,
                    remote_id,
                    log,
                    is_empty,
                );
                info.push(slot);
                // Check whether the session is empty.
                let is_empty = is_empty_session(&info);

                let addr = "Null".to_string();
                let state = "Null".to_string();
                let session = SessionInfo::new(
                    dnet_id.clone(),
                    node_id.clone(),
                    name,
                    addr,
                    state,
                    info,
                    is_empty,
                );
                return Ok(session)
            }
        }
    }

    async fn parse_outbound(&self, reply: &Value, node_id: &String) -> DnetViewResult<SessionInfo> {
        let name = "Outbound".to_string();
        let session_type = Session::Outbound;
        let dnet_id = make_session_id(node_id, &session_type)?;
        let mut info: Vec<SlotInfo> = Vec::new();

        // TODO: improve this ugly hack.
        let mut slot_count = 0;

        // Dnetview is not enabled.
        if reply.is_null() {
            let dnet_id = make_empty_id(&node_id, &session_type, slot_count)?;
            let node_id = node_id.to_string();
            let addr = "Null".to_string();
            let random_id = "Null".to_string();
            let remote_id = "Null".to_string();
            let log = Vec::new();
            let is_empty = false;

            let slot = SlotInfo::new(
                dnet_id.clone(),
                node_id.clone(),
                addr,
                random_id,
                remote_id,
                log,
                is_empty,
            );
            info.push(slot);
        }

        match reply.get("outbound") {
            Some(session) => {
                let inbound: Vec<serde_json::Map<String, Value>> =
                    serde_json::from_value(session.clone()).unwrap();
                for slot in inbound {
                    let addr = slot.get("addr").unwrap().as_str().unwrap().to_string();
                    let state = slot.get("state").unwrap().as_str().unwrap().to_string();
                    let slot_info: serde_json::Map<String, Value> =
                        serde_json::from_value(slot.get("info").unwrap().clone()).unwrap();

                    let slot_addr = slot_info.get("addr").unwrap().as_str().unwrap().to_string();
                    let random_id =
                        slot_info.get("random_id").unwrap().as_str().unwrap().to_string();
                    let remote_id =
                        slot_info.get("remote_id").unwrap().as_str().unwrap().to_string();

                    let log: Vec<(NanoTimestamp, String, String)> =
                        serde_json::from_value(slot_info.get("log").unwrap().clone()).unwrap();
                    let node_id = node_id.to_string();
                    let is_empty = false;

                    let slot = SlotInfo::new(
                        dnet_id.clone(),
                        node_id.clone(),
                        slot_addr,
                        random_id,
                        remote_id,
                        log,
                        is_empty,
                    );
                    info.push(slot);
                }

                // TODO: fixme
                let addr = String::new();
                // TODO: this should be an option
                let state = String::new();
                let node_id = node_id.to_string();
                let is_empty = false;
                let session =
                    SessionInfo::new(dnet_id, node_id.clone(), name, addr, state, info, is_empty);
                Ok(session)
            }
            None => {
                // Empty data. Initialize empty values.
                // TODO: clean up empty info boilerplate.
                let dnet_id = make_empty_id(node_id, &session_type, slot_count)?;
                let node_id = node_id.to_string();
                let addr = "Null".to_string();
                let random_id = "Null".to_string();
                let remote_id = "Null".to_string();
                let log = Vec::new();
                let is_empty = true;

                let slot = SlotInfo::new(
                    dnet_id.clone(),
                    node_id.clone(),
                    addr,
                    random_id,
                    remote_id,
                    log,
                    is_empty,
                );
                info.push(slot);
                // Check whether the session is empty.
                let is_empty = is_empty_session(&info);

                let addr = "Null".to_string();
                let state = "Null".to_string();
                let session = SessionInfo::new(
                    dnet_id.clone(),
                    node_id.clone(),
                    name,
                    addr,
                    state,
                    info,
                    is_empty,
                );
                return Ok(session)
            }
        }
    }

    async fn parse_hosts(&self, hosts: &Value) -> DnetViewResult<Vec<String>> {
        match hosts.as_array() {
            Some(h) => match h.is_empty() {
                true => Ok(Vec::new()),
                false => {
                    let hosts: Vec<String> =
                        h.iter().map(|addr| addr.as_str().unwrap().to_string()).collect();
                    Ok(hosts)
                }
            },

            None => {
                if hosts.is_null() {
                    // TODO: this should probs just say null
                    let h = Vec::new();
                    return Ok(h)
                }
                debug!("dnetview::parse_hosts() hosts returns None and !is_null() {}", hosts);
                Err(DnetViewError::ValueIsNotObject)
            }
        }
    }
}
