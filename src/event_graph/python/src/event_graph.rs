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
use darkfi::{event_graph, event_graph::event, net};
use pyo3::{
    prelude::PyModule, pyclass, pyfunction, pymethods, types::PyAny, wrap_pyfunction, PyCell,
    PyResult, Python,
};
use pyo3_asyncio;
use sled_overlay::sled;
use smol::Executor;
use std::{self, ops::Deref, path::PathBuf, sync::Arc};

#[pyclass]
pub struct EventGraphPtr(event_graph::EventGraphPtr);

//#[pyclass]
//pub struct EventGraph(event_graph::EventGraph);

#[pyfunction]
fn new_event_graph<'a>(
    py: Python<'a>,
    p2p: &P2pPtr,
    sled_db: &SledDb,
    datastore: PathBuf,
    replay_mode: bool,
    dag_tree_name: String,
    days_rotation: u64,
) -> PyResult<&'a PyAny> {
    //TODO (research) do we need to implement executors in python?
    // Executor require lifetime, but pyclass forbid use of lifetimes
    // because lifetime has no meaning in python which is
    // a reference-counted language
    let ex = Arc::new(Executor::new());
    let dag_tree_name_bind = dag_tree_name.clone();
    let p2p_ptr: net::P2pPtr = p2p.0.clone();
    let sled_db_bind: sled::Db = sled_db.0.clone();
    pyo3_asyncio::async_std::future_into_py(py, async move {
        let event_graph = event_graph::EventGraph::new(
            p2p_ptr,
            sled_db_bind,
            datastore,
            replay_mode,
            &*dag_tree_name_bind,
            days_rotation,
            ex,
        );
        let eg_res: Result<Arc<event_graph::EventGraph>, darkfi::Error> = event_graph.await;
        let eg: Arc<event_graph::EventGraph> = eg_res.unwrap();
        //note! pyclass implements IntoPy<PyObject> for EventGraphPtr
        let eg_pyclass: EventGraphPtr = EventGraphPtr(eg);
        Ok(eg_pyclass)
    })
}

#[pyclass]
pub struct Event(pub event::Event);
//TODO implement new event

#[pyclass]
pub struct Hash(pub blake3::Hash);
//TODO implement new hash

//#[pyclass]
//pub struct MessageInfo(event_graph::deg::MessageInfo);
// TODO impl new MessageInfo/constructor/builder

/*
// note! only unit variants supported py pyclass, so implemnet MessageInfo class with boolean type send/recev being true or false. or enum send/recv embedded in the struct.
#[pyclass]
pub enum DegEvent {
    SendMessage(MessageInfo),
    RecvMessage(MessageInfo),
}
*/

#[pymethods]
impl EventGraphPtr {
    // FIXME can't get conversion from PyAny to EventGraphPtr  because it doesn't impl clone, in otherwords can you get future_into_py return anything other than PyAny if it doesn't implement clone?
    /*
    #[new]
    fn new(
        py: Python,
        p2p: &P2pPtr,
        sled_db: &SledDb,
        datastore: PathBuf,
        replay_mode: bool,
        dag_tree_name: String,
        days_rotation: u64,
    ) -> PyResult<Self> {
        let eg_pyany = new_event_graph(py, p2p, sled_db, datastore, replay_mode, dag_tree_name, days_rotation)?;
        let eg : EventGraphPtr = eg_pyany.into();
        Ok(eg)
    }
    */

    fn dag_async<'a>(&'a self, py: Python<'a>) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            let _ = eg_ptr.dag_sync().await;
            Ok(())
        })
    }

    fn dag_insert<'a>(
        &'a self,
        py: Python<'a>,
        events: Vec<&PyCell<Event>>,
    ) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        let events_native: Vec<event::Event> =
            events.iter().map(|i| i.borrow().deref().0.clone()).collect();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            let ids_res: Result<Vec<blake3::Hash>, darkfi::Error> =
                eg_ptr.dag_insert(&events_native[..]).await;
            let ids: Vec<blake3::Hash> = ids_res.unwrap();
            let ids_native: Vec<Hash> = ids.iter().map(|i| Hash(i.clone())).collect();
            Ok(ids_native)
        })
    }

    fn dag_get<'a>(
        &'a self,
        py: Python<'a>,
        event_id_native: &PyCell<Hash>,
    ) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        let event_id: blake3::Hash = event_id_native.borrow().deref().0.clone();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            let event_res: Result<Option<event::Event>, darkfi::Error> =
                eg_ptr.dag_get(&event_id).await;
            let event: event::Event = event_res.unwrap().expect("expecting event in return");
            let event_native: Event = Event(event);
            Ok(event_native)
        })
    }

    fn order_events<'a>(&'a self, py: Python<'a>) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            eg_ptr.order_events().await;
            Ok(())
        })
    }

    fn deg_enable<'a>(&'a self, py: Python<'a>) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            eg_ptr.deg_enable().await;
            Ok(())
        })
    }

    fn deg_disable<'a>(&'a self, py: Python<'a>) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            eg_ptr.deg_disable().await;
            Ok(())
        })
    }

    fn deg_subscribe<'a>(&'a self, py: Python<'a>) -> PyResult<&'a PyAny> {
        let eg_ptr: event_graph::EventGraphPtr = self.0.clone();
        pyo3_asyncio::async_std::future_into_py(py, async move {
            eg_ptr.deg_subscribe().await;
            Ok(())
        })
    }

    /*
    //TODO impl
    fn deg_notify<'a>(&'a self, py: Python<'a>, event: &PyCell<DegEvent>) -> PyResult<&PyAny> {
        let eg_ptr : event_graph::EventGraphPtr = self.0.clone();
        let event_native: event_graph::deg::DegEvent = match event.borrow().deref() {
            DegEvent::SendMessage(m) => event_graph::deg::DegEvent::SendMessage(m),
            DegEvent::RecvMessage(m) => event_graph::deg::DegEvent::RecvMessage(m),
        };
        pyo3_asyncio::async_std::future_into_py(py, async move {
            eg_ptr.deg_notify(&event_native).await;
            Ok(())
        })
    }
    */
}

pub fn create_module(py: Python<'_>) -> PyResult<&PyModule> {
    let submod = PyModule::new(py, "event_graph")?;
    submod.add_class::<EventGraphPtr>()?;
    //submod.add_class::<EventGraph>()?;
    submod.add_class::<Event>()?;
    submod.add_class::<Hash>()?;
    submod.add_function(wrap_pyfunction!(new_event_graph, submod)?)?;
    Ok(submod)
}
