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
    prelude::PyModule,
    pyclass, pyfunction, pymethods,
    types::{PyAny, PyModuleMethods},
    wrap_pyfunction, Bound, PyResult, Python,
};
use pyo3_async_runtimes;
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

#[pyclass(eq, eq_int)]
#[derive(PartialEq)]
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

#[pyclass]
struct MagicBytes(pub net::settings::MagicBytes);

#[pymethods]
impl MagicBytes {
    #[new]
    fn new(bytes: [u8; 4]) -> Self {
        Self(net::settings::MagicBytes(bytes))
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
    inbound_addrs: Vec<Bound<Url>>,
    external_addrs: Vec<Bound<Url>>,
    peers: Vec<Bound<Url>>,
    seeds: Vec<Bound<Url>>,
    magic_bytes: &MagicBytes,
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
        inbound_addrs: inbound_addrs.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        external_addrs: external_addrs.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        peers: peers.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        seeds: seeds.iter().map(|i| i.borrow().deref().0.clone()).collect(),
        magic_bytes: magic_bytes.0.clone(),
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

#[pyclass]
pub struct P2p(pub net::P2p);

#[pyfunction]
fn new_p2p<'a>(py: Python<'a>, settings: &'a Settings) -> PyResult<Bound<'a, PyAny>> {
    let set: net::Settings = settings.0.clone();

    pyo3_async_runtimes::async_std::future_into_py(py, async move {
        let ex = Arc::new(smol::Executor::new());
        let fut = net::P2p::new(set, ex);
        let p2p_ptr_res: Result<net::P2pPtr, darkfi::Error> = fut.await;
        let net_p2p_ptr_res: net::P2pPtr = match p2p_ptr_res {
            Ok(p2p) => p2p,
            Err(e) => panic!("unwraping p2p ptr failed: {}", e),
        };
        let p2p_ptr: P2pPtr = P2pPtr(net_p2p_ptr_res);
        Ok(p2p_ptr)
    })
}

async fn start_p2p_and_wait(w8_time: u64, p2p_ptr: net::P2pPtr) {
    p2p_ptr.start().await.unwrap();
    async_std::task::sleep(std::time::Duration::from_secs(w8_time)).await;
}

#[pyfunction]
fn start_p2p<'a>(
    py: Python<'a>,
    w8_time: u64,
    net_p2p_ptr: &'a P2pPtr,
) -> PyResult<Bound<'a, PyAny>> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let ex = p2p_ptr.clone().executor.clone();
    let start_p2p_fut = start_p2p_and_wait(w8_time, p2p_ptr.clone());
    pyo3_async_runtimes::async_std::future_into_py(py, async move {
        ex.run(start_p2p_fut).await;
        Ok(())
    })
}

#[pyfunction]
fn is_connected<'a>(net_p2p_ptr: &'a P2pPtr) -> PyResult<bool> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let is_connected = p2p_ptr.is_connected();
    Ok(is_connected)
}

#[pyfunction]
fn get_greylist_length<'a>(net_p2p_ptr: &'a P2pPtr) -> PyResult<usize> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    Ok(p2p_ptr.hosts().container.fetch_all(net::hosts::HostColor::Grey).len())
}

async fn stop_p2p_and_wait(w8_time: u64, p2p_ptr: net::P2pPtr) {
    p2p_ptr.stop().await;
    async_std::task::sleep(std::time::Duration::from_secs(w8_time)).await;
}

#[pyfunction]
fn get_whitelist_length<'a>(net_p2p_ptr: &'a P2pPtr) -> PyResult<usize> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    Ok(p2p_ptr.hosts().container.fetch_all(net::hosts::HostColor::White).len())
}

#[pyfunction]
fn get_goldlist_length<'a>(net_p2p_ptr: &'a P2pPtr) -> PyResult<usize> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    Ok(p2p_ptr.hosts().container.fetch_all(net::hosts::HostColor::Gold).len())
}

#[pyfunction]
fn stop_p2p<'a>(
    py: Python<'a>,
    w8_time: u64,
    net_p2p_ptr: &'a P2pPtr,
) -> PyResult<Bound<'a, PyAny>> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let ex = p2p_ptr.executor.clone();
    pyo3_async_runtimes::async_std::future_into_py(py, async move {
        ex.run(stop_p2p_and_wait(w8_time, p2p_ptr.clone())).await;
        Ok(())
    })
}

async fn broadcast_and_wait(w8_time: u64, p2p_ptr: net::P2pPtr, event: event_graph::Event) {
    p2p_ptr.broadcast(&event_graph::proto::EventPut(event)).await;
    async_std::task::sleep(std::time::Duration::from_secs(w8_time)).await;
}

#[pyfunction]
fn broadcast_p2p<'a>(
    py: Python<'a>,
    w8_time: u64,
    net_p2p_ptr: &'a P2pPtr,
    event_py: &Bound<Event>,
) -> PyResult<Bound<'a, PyAny>> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let ex = p2p_ptr.executor.clone();
    let event: event_graph::Event = event_py.borrow().deref().0.clone();
    pyo3_async_runtimes::async_std::future_into_py(py, async move {
        ex.run(broadcast_and_wait(w8_time, p2p_ptr.clone(), event.clone())).await;
        Ok(())
    })
}

#[pyfunction]
fn register_protocol_p2p<'a>(
    py: Python<'a>,
    net_p2p_ptr: &'a P2pPtr,
    event_graph_py: &Bound<EventGraphPtr>,
) -> PyResult<Bound<'a, PyAny>> {
    let p2p_ptr: net::P2pPtr = net_p2p_ptr.0.clone();
    let event_graph_ptr: event_graph::EventGraphPtr = event_graph_py.borrow().deref().0.clone();
    pyo3_async_runtimes::async_std::future_into_py(py, async move {
        *event_graph_ptr.synced.write().await = true;
        let registry = p2p_ptr.protocol_registry();
        registry
            .register(net::session::SESSION_DEFAULT, move |channel, _| {
                let event_graph_ = event_graph_ptr.clone();
                async move {
                    event_graph::proto::ProtocolEventGraph::init(event_graph_, channel)
                        .await
                        .unwrap()
                }
            })
            .await;
        async_std::task::sleep(std::time::Duration::from_secs(3)).await;
        Ok(())
    })
}

pub fn create_module(py: Python<'_>) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new_bound(py, "event_graph")?;
    submod.add_class::<P2pPtr>()?;
    submod.add_class::<P2p>()?;
    submod.add_class::<Settings>()?;
    submod.add_class::<Url>()?;
    submod.add_class::<MagicBytes>()?;
    submod.add_function(wrap_pyfunction!(new_version, &submod)?)?;
    submod.add_function(wrap_pyfunction!(new_settings, &submod)?)?;
    submod.add_function(wrap_pyfunction!(get_strict_banpolicy, &submod)?)?;
    submod.add_function(wrap_pyfunction!(get_relaxed_banpolicy, &submod)?)?;
    submod.add_function(wrap_pyfunction!(parse_url, &submod)?)?;
    submod.add_function(wrap_pyfunction!(new_p2p, &submod)?)?;
    submod.add_function(wrap_pyfunction!(start_p2p, &submod)?)?;
    submod.add_function(wrap_pyfunction!(get_greylist_length, &submod)?)?;
    submod.add_function(wrap_pyfunction!(get_whitelist_length, &submod)?)?;
    submod.add_function(wrap_pyfunction!(get_goldlist_length, &submod)?)?;
    submod.add_function(wrap_pyfunction!(stop_p2p, &submod)?)?;
    submod.add_function(wrap_pyfunction!(broadcast_p2p, &submod)?)?;
    submod.add_function(wrap_pyfunction!(register_protocol_p2p, &submod)?)?;
    submod.add_function(wrap_pyfunction!(is_connected, &submod)?)?;
    Ok(submod)
}
