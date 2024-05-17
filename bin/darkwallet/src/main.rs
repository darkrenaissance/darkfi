#![feature(deadline_api)]

use std::{
    sync::{Arc, Mutex},
    thread,
};

mod error;

mod expr;

mod gfx;
use gfx::init_gui;

mod net;
use net::ZeroMQAdapter;

mod scene;
use scene::{SceneGraph, SceneGraphPtr};

mod prop;

mod shader;

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

fn init_zmq(scene_graph: SceneGraphPtr) {
    // detach thread
    let _ = thread::spawn(move || {
        let mut zmq_rpc = ZeroMQAdapter::new(scene_graph);
        zmq_rpc.poll();
    });
}

fn main() {
    let scene_graph = Arc::new(Mutex::new(SceneGraph::new()));
    init_zmq(scene_graph.clone());
    init_gui(scene_graph);
}

/*
use pyo3::{prelude::*, types::{PyDict, IntoPyDict}, py_run};

fn main() -> PyResult<()> {
    Python::with_gil(|py| {
        let null = ();
        // https://stackoverflow.com/questions/35804961/python-eval-is-it-still-dangerous-if-i-disable-builtins-and-attribute-access
        // See safe_eval() by tardyp and astrun
        // We don't care about resource usage, just accessing system resources.
        // Can also use restrictedpython lib to eval the code.
        // Also PyPy sandboxing
        // and starlark / starlark-rust
        py_run!(py, null, r#"
__builtins__.__dict__['__import__'] = None
__builtins__.__dict__['open'] = None
        "#);

        let locals = PyDict::new_bound(py);
        locals.set_item("lw", 110)?;
        locals.set_item("lh", 4)?;

        let code = "min(1 + lw/3, 4*10)";
        let user: f32 = py.eval_bound(code, None, Some(&locals))?.extract()?;

        println!("{}", user);
        Ok(())
    })
}
*/
