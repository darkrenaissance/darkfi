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
    util::{
        make_empty_id, make_info_id, make_network_id, make_node_id, make_null_id, make_session_id,
    },
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
    async fn parse_offline(&self, node_name: String) -> DnetViewResult<()> {
        debug!(target: "dnetview", "parse_offline() START");
        //let name = "Offline".to_string();
        let sort = Session::Offline;

        let mut sessions: Vec<SessionInfo> = Vec::new();
        let hosts = Vec::new();

        let node_id = make_node_id(&node_name)?;
        let dnet_id = make_empty_id(&node_id, &sort, 0)?;
        let addr = "Null".to_string();
        let state = None;
        let random_id = 0;
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

        let session_info = SessionInfo::new(
            dnet_id,
            node_id.clone(),
            addr.clone(),
            state,
            slot,
            sort.clone(),
            is_empty,
        );
        sessions.push(session_info);

        // TODO: clean this up
        let node = NodeInfo::new(
            node_id.clone(),
            node_name.clone(),
            hosts,
            sessions.clone(),
            sessions.clone(),
            is_empty,
            true,
        );

        self.update_selectables(node).await?;
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

        let node_id = make_node_id(&name)?;

        let dnet_enabled: bool = {
            if hosts.is_null() && inbound.is_null() && outbound.is_null() {
                false
            } else {
                true
            }
        };
        debug!("dnet_enabled? {}", dnet_enabled);

        let hosts = self.parse_hosts(hosts).await?;
        let inbound = self.parse_session(inbound, &node_id, Session::Inbound).await?;
        let outbound = self.parse_session(outbound, &node_id, Session::Outbound).await?;

        let node = NodeInfo::new(
            node_id,
            name,
            hosts,
            inbound.clone(),
            outbound.clone(),
            false,
            dnet_enabled,
        );

        self.update_selectables(node).await?;
        self.update_msgs(inbound.clone(), outbound.clone()).await?;

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
            let id = make_network_id(&name)?;
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

    async fn update_msgs(
        &self,
        inbounds: Vec<SessionInfo>,
        outbounds: Vec<SessionInfo>,
    ) -> DnetViewResult<()> {
        for inbound in inbounds {
            if !self.model.msg_map.lock().await.contains_key(&inbound.info.dnet_id) {
                // we don't have this ID: it is a new node
                self.model
                    .msg_map
                    .lock()
                    .await
                    .insert(inbound.info.dnet_id, inbound.info.log.clone());
            } else {
                // we have this id: append the msg values
                match self.model.msg_map.lock().await.entry(inbound.info.dnet_id) {
                    Entry::Vacant(e) => {
                        e.insert(inbound.info.log);
                    }
                    Entry::Occupied(mut e) => {
                        for msg in inbound.info.log {
                            e.get_mut().push(msg);
                        }
                    }
                }
            }
        }
        for outbound in outbounds {
            if !self.model.msg_map.lock().await.contains_key(&outbound.info.dnet_id) {
                // we don't have this ID: it is a new node
                self.model
                    .msg_map
                    .lock()
                    .await
                    .insert(outbound.info.dnet_id, outbound.info.log.clone());
            } else {
                // we have this id: append the msg values
                match self.model.msg_map.lock().await.entry(outbound.info.dnet_id) {
                    Entry::Vacant(e) => {
                        e.insert(outbound.info.log);
                    }
                    Entry::Occupied(mut e) => {
                        for msg in outbound.info.log {
                            e.get_mut().push(msg);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn update_selectables(&self, node: NodeInfo) -> DnetViewResult<()> {
        if node.is_offline {
            let node_obj = SelectableObject::Node(node.clone());
            self.model.selectables.lock().await.insert(node.dnet_id.clone(), node_obj.clone());
        } else {
            let node_obj = SelectableObject::Node(node.clone());
            self.model.selectables.lock().await.insert(node.dnet_id.clone(), node_obj.clone());
            for inbound in node.inbound {
                if !inbound.is_empty {
                    let inbound_obj = SelectableObject::Session(inbound.clone());
                    self.model
                        .selectables
                        .lock()
                        .await
                        .insert(inbound.clone().dnet_id, inbound_obj.clone());
                    let info_obj = SelectableObject::Slot(inbound.info.clone());
                    self.model
                        .selectables
                        .lock()
                        .await
                        .insert(inbound.info.clone().dnet_id, info_obj.clone());
                }
            }
            for outbound in node.outbound {
                if !outbound.is_empty {
                    let outbound_obj = SelectableObject::Session(outbound.clone());
                    self.model
                        .selectables
                        .lock()
                        .await
                        .insert(outbound.clone().dnet_id, outbound_obj.clone());
                    let info_obj = SelectableObject::Slot(outbound.info.clone());
                    self.model
                        .selectables
                        .lock()
                        .await
                        .insert(outbound.info.clone().dnet_id, info_obj.clone());
                }
            }
        }
        Ok(())
    }

    async fn parse_session(
        &self,
        reply: &Value,
        node_id: &String,
        sort: Session,
    ) -> DnetViewResult<Vec<SessionInfo>> {
        let session_id = make_session_id(&node_id, &sort)?;
        let mut session_info: Vec<SessionInfo> = Vec::new();

        // Dnetview is not enabled.
        if reply.is_null() {
            let sort2 = Session::Null;
            let info_id = make_null_id(&node_id)?;
            let node_id = node_id.to_string();
            let addr = "Null".to_string();
            let random_id = 0;
            let remote_id = "Null".to_string();
            let log = Vec::new();
            let is_empty = true;

            let slot = SlotInfo::new(
                info_id.clone(),
                node_id.clone(),
                addr,
                random_id,
                remote_id,
                log,
                is_empty,
            );
            let is_empty = true;

            let addr = "Null".to_string();
            let state = None;
            let session = SessionInfo::new(
                // ..
                info_id.clone(),
                node_id.clone(),
                addr,
                state,
                slot,
                sort2.clone(),
                is_empty,
            );
            session_info.push(session);

            return Ok(session_info)
        }

        let sessions = reply.as_array().unwrap();

        for session in sessions {
            // TODO: display empty sessions?
            if !session.is_null() {
                match session.as_object() {
                    Some(obj) => {
                        let addr = obj.get("addr").unwrap().as_str().unwrap().to_string();

                        let state: Option<String> = match obj.get("state") {
                            Some(state) => Some(state.as_str().unwrap().to_string()),
                            None => None,
                        };

                        let info: serde_json::Map<String, Value> =
                            serde_json::from_value(obj.get("info").unwrap().clone()).unwrap();

                        let slot_addr = info.get("addr").unwrap().as_str().unwrap().to_string();
                        let random_id = info.get("random_id").unwrap().as_u64().unwrap();
                        let remote_id =
                            info.get("remote_id").unwrap().as_str().unwrap().to_string();
                        let info_id = make_info_id(&random_id)?;

                        let log: Vec<(NanoTimestamp, String, String)> =
                            serde_json::from_value(info.get("log").unwrap().clone()).unwrap();

                        // ...
                        let node_id = node_id.to_string();
                        let is_empty = false;

                        let slot = SlotInfo::new(
                            info_id.clone(),
                            node_id.clone(),
                            slot_addr,
                            random_id,
                            remote_id,
                            log,
                            is_empty,
                        );

                        let session = SessionInfo::new(
                            session_id.clone(),
                            node_id.clone(),
                            addr.clone(),
                            state,
                            slot.clone(),
                            sort.clone(),
                            is_empty,
                        );
                        session_info.push(session);
                    }
                    None => {
                        return Err(DnetViewError::ValueIsNotObject)
                    }
                }
            }
        }

        Ok(session_info)
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
                Err(DnetViewError::ValueIsNotObject)
            }
        }
    }
}
