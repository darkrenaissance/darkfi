#![feature(deadline_api)]

use std::{
    sync::{Arc, Mutex},
    thread,
};

mod error;

mod expr;

mod gfx;
use gfx::run_gui;

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
    run_gui(scene_graph);
}

/*
use rustpython_vm::{self as pyvm, convert::ToPyObject};

fn main() {
    let code_obj = pyvm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let source = r#"
max(1 + lw/3, 4*10) + foo(2, True)"#;
        let code_obj = vm
            .compile(source, pyvm::compiler::Mode::Eval, "<embedded>".to_owned())
            .map_err(|err| vm.new_syntax_error(&err, Some(source))).unwrap();
        code_obj
    });

    fn foo(x: u32, y: bool) -> u32 {
        if y {
            2 * x
        } else {
            x
        }
    }

    let res = pyvm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let globals = vm.ctx.new_dict();
        globals.set_item("lw", vm.ctx.new_int(110).to_pyobject(vm), vm).unwrap();
        globals.set_item("lh", vm.ctx.new_int(4).to_pyobject(vm), vm).unwrap();
        globals.set_item("foo", vm.new_function("foo", foo).into(), vm).unwrap();

        let scope = pyvm::scope::Scope::new(None, globals);

        vm.run_code_obj(code_obj, scope).unwrap()
    });
    println!("{:?}", res);
}
*/

