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

use super::{p2p::P2pPtr, sled::SledDb};
use darkfi::{event_graph, event_graph::event, net, Error};
use pyo3::{
    prelude::PyModule,
    pyclass, pyfunction, pymethods,
    types::{PyAny, PyModuleMethods},
    wrap_pyfunction, Bound, PyResult, Python,
};
use pyo3_async_runtimes;
use sled_overlay::sled;
use smol::Executor;
use std::{self, ops::Deref, path::PathBuf, sync::Arc};

#[pyclass]
pub struct EventGraphPtr(pub event_graph::EventGraphPtr);

#[pyclass]
pub struct EventGraph(pub event_graph::EventGraph);

#[pyfunction]
fn new_event_graph<'a>(
    py: Python<'a>,
    p2p: &P2pPtr,
    sled_db: &SledDb,
    datastore: PathBuf,
    replay_mode: bool,
    dag_tree_name: String,
    days_rotation: u64,
) -> PyResult<EventGraphPtr> {
    //TODO (research) do we need to implement executors in python?
    // Executor require lifetime, but pyclass forbid use of lifetimes
    // because lifetime has no meaning in python which is
    // a reference-counted language
    let ex = Arc::new(Executor::new());
    let p2p_ptr: net::P2pPtr = p2p.0.clone();
    let sled_db_bind: sled::Db = sled_db.0.clone();
    let eg_res: Result<Arc<event_graph::EventGraph>, Error> =
        pyo3_async_runtimes::async_std::run(py, async move {
            let event_graph = event_graph::EventGraph::new(
                p2p_ptr,
                sled_db_bind,
                datastore,
                replay_mode,
                &*dag_tree_name,
                days_rotation,
                ex.clone(),
            );
            let eg_res = ex.run(event_graph).await;
            Ok(eg_res)
        })
        .unwrap();
    let eg: Arc<event_graph::EventGraph> = eg_res.unwrap();
    //note! pyclass implements IntoPy<PyObject> for EventGraphPtr
    let eg_pyclass: EventGraphPtr = EventGraphPtr(eg);
    Ok(eg_pyclass)
}

#[pyclass]
pub struct Hash(pub blake3::Hash);

#[pymethods]
impl Hash {
    fn __str__<'a>(&'a self) -> PyResult<String> {
        let str_hash = String::from(self.0.to_hex().as_str());
        Ok(str_hash)
    }
}

#[pyclass]
pub struct Event(pub event::Event);

#[pyfunction]
fn new_event<'a>(
    py: Python<'a>,
    data: Vec<u8>,
    eg_py: &Bound<EventGraphPtr>,
) -> PyResult<Bound<'a, PyAny>> {
    let eg_ptr: event_graph::EventGraphPtr = eg_py.borrow().deref().0.clone();
    pyo3_async_runtimes::async_std::future_into_py(py, async move {
        let eg: &event_graph::EventGraph = eg_ptr.deref();
        let ev = event::Event::new(data, eg).await;
        Ok(Event(ev))
    })
}

#[pymethods]
impl Event {
    fn id<'a>(&'a self) -> PyResult<Hash> {
        let event: event_graph::Event = self.0.clone();
        let hash: blake3::Hash = event.id();
        Ok(Hash(hash))
    }
}

async fn dag_insert_wait(
    w8_time: u64,
    eg_ptr: event_graph::EventGraphPtr,
    events_native: Vec<event::Event>,
) -> Vec<blake3::Hash> {
    let fut = eg_ptr.dag_insert(&events_native[..]);
    let ids_res: Result<Vec<blake3::Hash>, Error> = fut.await;
    let ids: Vec<blake3::Hash> = ids_res.unwrap();
    async_std::task::sleep(std::time::Duration::from_secs(w8_time)).await;
    ids
}

#[pymethods]
impl EventGraphPtr {
    fn dag_sync<'a>(&'a self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_async_runtimes::async_std::future_into_py(py, async move {
            eg_ptr.dag_sync().await.unwrap();
            Ok(())
        })
    }

    fn dag_insert<'a>(&'a self, py: Python<'a>, events: Vec<Bound<Event>>) -> PyResult<Vec<Hash>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        let events_native: Vec<event::Event> =
            events.iter().map(|i| i.borrow().deref().0.clone()).collect();
        pyo3_async_runtimes::async_std::run(py, async move {
            let ids =
                eg_ptr.p2p.executor.run(dag_insert_wait(5, eg_ptr.clone(), events_native)).await;
            let ids_native: Vec<Hash> = ids.iter().map(|i| Hash(i.clone())).collect();
            Ok(ids_native)
        })
    }

    fn dag_get<'a>(
        &'a self,
        py: Python<'a>,
        event_id_native: &Bound<Hash>,
    ) -> PyResult<Bound<'a, PyAny>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        let event_id: blake3::Hash = event_id_native.borrow().deref().0.clone();
        pyo3_async_runtimes::async_std::future_into_py(py, async move {
            let event_res: Result<Option<event::Event>, Error> = eg_ptr.dag_get(&event_id).await;
            let event: event::Event = event_res
                .unwrap()
                .expect(&format!("expecting event in return with id: {}", event_id).to_string());
            let event_native: Event = Event(event);
            Ok(event_native)
        })
    }

    fn dag_len(&self) -> usize {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        eg_ptr.dag_len()
    }

    fn order_events<'a>(&'a self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_async_runtimes::async_std::future_into_py(py, async move {
            eg_ptr.order_events().await;
            Ok(())
        })
    }

    fn deg_enable<'a>(&'a self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_async_runtimes::async_std::future_into_py(py, async move {
            eg_ptr.deg_enable().await;
            Ok(())
        })
    }

    fn deg_disable<'a>(&'a self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_async_runtimes::async_std::future_into_py(py, async move {
            eg_ptr.deg_disable().await;
            Ok(())
        })
    }

    fn deg_subscribe<'a>(&'a self, py: Python<'a>) -> PyResult<Bound<'a, PyAny>> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_async_runtimes::async_std::future_into_py(py, async move {
            eg_ptr.deg_subscribe().await;
            Ok(())
        })
    }
}

pub fn create_module(py: Python<'_>) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new_bound(py, "event_graph")?;
    submod.add_class::<EventGraphPtr>()?;
    submod.add_class::<EventGraph>()?;
    submod.add_class::<Event>()?;
    submod.add_class::<Hash>()?;
    submod.add_function(wrap_pyfunction!(new_event_graph, &submod)?)?;
    submod.add_function(wrap_pyfunction!(new_event, &submod)?)?;
    Ok(submod)
}
