/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use crate::event_graph::{Event, EventGraphPtr};
use darkfi::{event_graph, net};
use pyo3::{
    prelude::PyModule, pyclass, pyfunction, pymethods, types::PyAny, wrap_pyfunction, PyCell,
    PyResult, Python,
};
use pyo3_asyncio;
use semver;
use smol;
use std::{ops::Deref, sync::Arc};
use url;

#[pyfunction]
fn parse_url(url: &str) -> PyResult<String> {
    match url::Url::parse(url) {
        Ok(parsed) => Ok(parsed.into()),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e.to_string())),
    }
}

#[pyclass]
pub struct Settings(pub net::Settings);

#[pyclass]
pub struct Version(pub semver::Version);

#[pyclass]
pub enum BanPolicy {
    Strict,
    Relaxed,
}

#[pyclass]
struct Url(pub url::Url);

#[pymethods]
impl Url {
    #[new]
    fn new(url_str: String) -> Self {
        let url_res: PyResult<String> = parse_url(&url_str);
        Self(url::Url::parse(&url_res.unwrap()).unwrap())
    }
}

#[pyfunction]
fn get_strict_banpolicy() -> PyResult<BanPolicy> {
    Ok(BanPolicy::Strict)
}

#[pyfunction]
fn get_relaxed_banpolicy() -> PyResult<BanPolicy> {
    Ok(BanPolicy::Relaxed)
}

#[pyfunction]
fn new_version(major: u64, minor: u64, patch: u64, prerelease: String) -> PyResult<Version> {
    let version = semver::Version {
        major,
        minor,
        patch,
        pre: semver::Prerelease::new(&prerelease).unwrap(),
        build: semver::BuildMetadata::EMPTY,
    };
    Ok(Version(version))
}

type BlacklistEntry = (String, Vec<String>, Vec<u16>);

#[pyfunction]
fn new_settings(
    node_id: String,
    inbound_addrs: Vec<&PyCell<Url>>,
    external_addrs: Vec<&PyCell<Url>>,
    peers: Vec<&PyCell<Url>>,
    seeds: Vec<&PyCell<Url>>,
    app_version: &Version,
    allowed_transports: Vec<String>,
    transport_mixing: bool,
    outbound_connections: usize,
    inbound_connections: usize,
    outbound_connect_timeout: u64,
    channel_handshake_timeout: u64,
    channel_hearbeat_interval: u64,
    localnet: bool,
    outbound_peer_discovery_cooloff_time: u64,
    outbound_peer_discovery_attempt_time: u64,
    p2p_datastore: String, //option
    hostlist: String,      //option
    greylist_refinery_interval: u64,
    white_connect_percent: usize,
    gold_connect_count: usize,
    slot_preference_strict: bool,
    time_with_no_connections: u64,
    blacklist: Vec<BlacklistEntry>,
    ban_policy: &BanPolicy,
) -> PyResult<Settings> {
    let settings = net::Settings {
        node_id,
        inbound_addrs: inbound_addrs.iter().map(|x| x.borrow().deref().0.clone()).collect(),
        external_addrs: external_addrs.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        peers: peers.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        seeds: seeds.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        app_version: app_version.0.clone(),
        allowed_transports,
        transport_mixing,
        outbound_connections,
        inbound_connections,
        outbound_connect_timeout,
        channel_handshake_timeout,
        channel_heartbeat_interval: channel_hearbeat_interval,
        localnet,
        outbound_peer_discovery_cooloff_time,
        outbound_peer_discovery_attempt_time,
        p2p_datastore: Some(p2p_datastore),
        hostlist: Some(hostlist),
        greylist_refinery_interval,
        white_connect_percent,
        gold_connect_count,
        slot_preference_strict,
        time_with_no_connections,
        blacklist,
        ban_policy: match ban_policy {
            BanPolicy::Strict => net::BanPolicy::Strict,
            BanPolicy::Relaxed => net::BanPolicy::Relaxed,
        },
    };
    Ok(Settings(settings))
}

#[pyclass]
pub struct P2pPtr(pub net::P2pPtr);

#[pymethods]
impl P2pPtr {
    // FIXME can't get conversion from PyAny to EventGraphPtr  because it doesn't impl clone, in otherwords can you get future_into_py return anything other than PyAny if it doesn't implement clone?
    /*
    #[new]
    fn new<'a>(py: Python<'a>, settings: &'a Settings) -> PyResult<Self> {
        let p2p_ptr_pyany : PyResult<&PyAny> = new_p2p(py, settings);
        // if we return PyResult<P2pPtr> pyany.borrow_mut() require implementation of From<P2pPtr> for PyClassInitializer<p2p::P2p> which is required by Result<P2pPtr, PyErr>: IntoPyCallbackOutput<_>
        // Ok(p2p_ptr.borrow_mut())
        // if we return PyResult<&'a PyAny>, we get error trait From<&PyAny> is not implemented for PyClassInitializer<p2p::P2p> which is required by Result<&PyAny, PyErr>: IntoPyCallbackOutput<_>
        //p2p_ptr
        let p2p_ptr : P2pPtr = p2p_ptr_pyany.unwrap().borrow();
        Ok(p2p_ptr)
    }
     */
}

#[pyclass]
pub struct P2p(pub net::P2p);

#[pyfunction]
fn new_p2p<'a>(py: Python<'a>, settings: &'a Settings) -> PyResult<&'a PyAny> {
    let set: net::Settings = settings.0.clone();
    let ex = Arc::new(smol::Executor::new());
    let fut = net::P2p::new(set, ex);
    pyo3_asyncio::async_std::future_into_py(py, async move {
        let p2p_ptr_res: Result<net::P2pPtr, darkfi::Error> = fut.await;
        let net_p2p_ptr_res: net::P2pPtr = match p2p_ptr_res {
            Ok(p2p) => p2p,
            Err(e) => panic!("unwraping p2p ptr failed: {}", e),
        };
        let p2p_ptr: P2pPtr = P2pPtr(net_p2p_ptr_res);
        Ok(p2p_ptr)
    })
}

#[pyfunction]
fn start_p2p<'a>(py: Python<'a>, net_p2p_ptr: &'a P2pPtr) -> PyResult<&'a PyAny> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    pyo3_asyncio::async_std::future_into_py(py, async move {
        let _ = p2p_ptr.start().await;
        Ok(())
    })
}

#[pyfunction]
fn broadcast_p2p<'a>(
    py: Python<'a>,
    net_p2p_ptr: &'a P2pPtr,
    event_py: &PyCell<Event>,
) -> PyResult<&'a PyAny> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let event: event_graph::Event = event_py.borrow().deref().0.clone();
    pyo3_asyncio::async_std::future_into_py(py, async move {
        let _ = p2p_ptr.broadcast(&event_graph::proto::EventPut(event)).await;
        Ok(())
    })
}

#[pyfunction]
fn register_protocol_p2p<'a>(
    py: Python<'a>,
    net_p2p_ptr: &'a P2pPtr,
    event_graph: &PyCell<EventGraphPtr>,
) -> PyResult<&'a PyAny> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let eg: event_graph::EventGraphPtr = event_graph.borrow().deref().0.clone();
    pyo3_asyncio::async_std::future_into_py(py, async move {
        *eg.synced.write().await = true;
        let registry = p2p_ptr.protocol_registry();
        registry
            .register(net::session::SESSION_DEFAULT, move |channel, _| {
                let event_graph_ = eg.clone();
                async move {
                    event_graph::proto::ProtocolEventGraph::init(event_graph_, channel)
                        .await
                        .unwrap()
                }
            })
            .await;
        Ok(())
    })
}

pub fn create_module(py: Python<'_>) -> PyResult<&PyModule> {
    let submod = PyModule::new(py, "event_graph")?;
    submod.add_class::<P2pPtr>()?;
    submod.add_class::<P2p>()?;
    submod.add_class::<Settings>()?;
    submod.add_class::<Url>()?;
    submod.add_function(wrap_pyfunction!(new_version, submod)?)?;
    submod.add_function(wrap_pyfunction!(new_settings, submod)?)?;
    submod.add_function(wrap_pyfunction!(get_strict_banpolicy, submod)?)?;
    submod.add_function(wrap_pyfunction!(get_relaxed_banpolicy, submod)?)?;
    submod.add_function(wrap_pyfunction!(parse_url, submod)?)?;
    submod.add_function(wrap_pyfunction!(new_p2p, submod)?)?;
    submod.add_function(wrap_pyfunction!(start_p2p, submod)?)?;
    submod.add_function(wrap_pyfunction!(broadcast_p2p, submod)?)?;
    submod.add_function(wrap_pyfunction!(register_protocol_p2p, submod)?)?;
    Ok(submod)
}
